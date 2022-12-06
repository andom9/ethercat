mod configure_for_op;
mod error;
pub mod mailbox;
mod read_write_as;
pub use configure_for_op::*;
pub use error::*;
pub use read_write_as::*;

use crate::{
    frame::MailboxFrame,
    interface::{
        PduInterface, PduSocket, PhyError, RawEthernetDevice, SlaveAddress, SocketHandle,
        SocketInterface, TargetSlave,
    },
    register::{AlStatusCode, RxErrorCounter, SiiData},
    slave::{AlState, Network, Slave, SlaveConfig},
    task::{
        loop_task::*, AlStateTransferTask, AlStateTransferTaskError, CyclicTask,
        EtherCatSystemTime, MailboxTask, MailboxTaskError, NetworkInitTaskError, SdoErrorKind,
        SiiTaskError, TaskError, MAX_SM_SIZE,
    },
};

use self::mailbox::{MailboxManager, MailboxReqIfWrapper, MailboxSessionId};

const LOGICAL_START_ADDRESS: u32 = 0x1000;
const NUM_SOCKETS: usize = 7;

#[derive(Debug)]
pub struct EtherCatMaster<'frame, 'socket, 'slave, 'pdo_mapping, 'pdo_entry, D>
where
    D: RawEthernetDevice,
{
    sif: SocketInterface<'frame, 'socket, D, NUM_SOCKETS>,
    network: Network<'slave, 'pdo_mapping, 'pdo_entry>,
    gp_socket_handle: SocketHandle,
    cycle_count: usize,
    //mailbox
    mailbox_handle: SocketHandle,
    mailbox_manager: MailboxManager,
    //process data
    process_data_handle: Option<SocketHandle>,
    process_data_task: ProcessTask,
    //dc drift
    dc_handle: SocketHandle,
    dc_task: Option<DcSyncTask>,
    //alstate read
    al_state_handle: SocketHandle,
    al_state_task: AlStateReadTask,
    //rx error
    rx_error_handle: SocketHandle,
    rx_error_task: RxErrorReadTask,
    //alstate transfer
    al_tf_handle: SocketHandle,
    al_tf_task: AlStateTransferTask,
}

impl<'frame, 'socket, 'slave, 'pdo_mapping, 'pdo_entry, D>
    EtherCatMaster<'frame, 'socket, 'slave, 'pdo_mapping, 'pdo_entry, D>
where
    D: RawEthernetDevice,
{
    pub fn new(
        slave_buf: &'slave mut [(Option<Slave>, SlaveConfig<'pdo_mapping, 'pdo_entry>)],
        socket_buffer: &'socket mut [u8],
        iface: PduInterface<'frame, D>,
    ) -> Self {
        assert!(!slave_buf.is_empty());

        const MINIMUM_REQUIRED_BUFFER_SIZE: usize = AlStateReadTask::required_buffer_size()
            + RxErrorReadTask::required_buffer_size()
            + DcSyncTask::required_buffer_size()
            + AlStateTransferTask::required_buffer_size()
            + MAX_SM_SIZE as usize
            + MAX_SM_SIZE as usize;
        assert!(MINIMUM_REQUIRED_BUFFER_SIZE < socket_buffer.len());

        let (pdu_buffer1, rest) =
            socket_buffer.split_at_mut(AlStateReadTask::required_buffer_size());
        let (pdu_buffer2, rest) = rest.split_at_mut(RxErrorReadTask::required_buffer_size());
        let (pdu_buffer3, rest) = rest.split_at_mut(DcSyncTask::required_buffer_size());
        let (pdu_buffer4, rest) = rest.split_at_mut(MAX_SM_SIZE as usize);
        let (pdu_buffer5, rest) = rest.split_at_mut(MAX_SM_SIZE as usize);
        let (pdu_buffer6, _) = rest.split_at_mut(AlStateTransferTask::required_buffer_size());

        let mut sif = SocketInterface::new(iface);
        let al_state_handle = sif.add_socket(PduSocket::new(pdu_buffer1)).unwrap();
        let rx_error_handle = sif.add_socket(PduSocket::new(pdu_buffer2)).unwrap();
        let dc_handle = sif.add_socket(PduSocket::new(pdu_buffer3)).unwrap();
        let gp_socket_handle = sif.add_socket(PduSocket::new(pdu_buffer4)).unwrap();
        let mailbox_handle = sif.add_socket(PduSocket::new(pdu_buffer5)).unwrap();
        let al_tf_handle = sif.add_socket(PduSocket::new(pdu_buffer6)).unwrap();

        let network = Network::new(slave_buf);
        Self {
            sif,
            network,
            gp_socket_handle,
            cycle_count: 0,
            mailbox_handle,
            mailbox_manager: MailboxManager::new(MailboxTask::new()),
            process_data_handle: None,
            process_data_task: ProcessTask::new(LOGICAL_START_ADDRESS, 0, 0),
            dc_handle,
            dc_task: None,
            al_state_task: AlStateReadTask::new(),
            al_state_handle,
            rx_error_task: RxErrorReadTask::new(),
            rx_error_handle,
            al_tf_handle,
            al_tf_task: AlStateTransferTask::new(),
        }
    }

    pub fn init(&mut self) -> Result<(), TaskError<NetworkInitTaskError>> {
        let Self { network, sif, .. } = self;
        sif.init(&self.gp_socket_handle, network)?;
        self.al_state_task
            .set_target(TargetSlave::All(network.num_slaves()));
        Ok(())
    }

    pub fn init_dc(&mut self) -> Result<(), TaskError<()>> {
        let Self { network, sif, .. } = self;
        sif.init_dc(&self.gp_socket_handle, network)?;

        let mut firt_dc_slave = None;
        let mut dc_count = 0;
        for (i, (slave, _)) in network.slaves().enumerate() {
            if slave.info().support_dc() {
                dc_count += 1;
                if firt_dc_slave.is_none() {
                    firt_dc_slave = Some(i);
                }
            }
        }
        if let Some(firt_dc_slave) = firt_dc_slave {
            self.dc_task = Some(DcSyncTask::new(firt_dc_slave as u16, dc_count));
        }
        Ok(())
    }

    pub fn network<'a>(&'a self) -> &'a Network<'slave, 'pdo_mapping, 'pdo_entry> {
        &self.network
    }

    /// This method must be repeated until the cycle count returned is increased.
    pub fn process(&mut self, sys_time: EtherCatSystemTime) -> Result<usize, PhyError> {
        let is_tx_rx_ok = self.sif.poll_tx_rx()?;
        if !is_tx_rx_ok {
            return Ok(self.cycle_count);
        }

        let Self {
            network,
            mailbox_handle,
            mailbox_manager,
            process_data_handle,
            process_data_task,
            dc_handle,
            dc_task,
            al_state_handle,
            al_state_task,
            rx_error_handle,
            rx_error_task,
            al_tf_handle,
            al_tf_task,
            ..
        } = self;

        // process data + mb polling
        if let (Some(ref handle), ref mut task) = (process_data_handle, process_data_task) {
            {
                let socket = self.sif.get_socket_mut(handle).unwrap();
                task.process_one_step(socket, sys_time);
            }
            let mb_socket = self.sif.get_socket_mut(mailbox_handle).unwrap();
            mailbox_manager.process_one_step(&network, mb_socket, sys_time);
            let socket = self.sif.get_socket(handle).unwrap();
            mailbox_manager.find_slave_with_mailbox_from_process_data(
                network,
                LOGICAL_START_ADDRESS,
                socket.data_buf(),
            );
        }

        // comp dc drift
        if let (ref handle, Some(ref mut task)) = (dc_handle, dc_task) {
            let socket = self.sif.get_socket_mut(handle).unwrap();
            task.process_one_step(socket, sys_time);
        }

        // check rx error
        {
            let socket = self.sif.get_socket_mut(rx_error_handle).unwrap();
            rx_error_task.process_one_step(socket, sys_time);
        }

        // check al state + al status code
        {
            let socket = self.sif.get_socket_mut(al_state_handle).unwrap();
            al_state_task.process_one_step(socket, sys_time);
        }

        // transfer al state
        {
            let socket = self.sif.get_socket_mut(al_tf_handle).unwrap();
            al_tf_task.process_one_step(socket, sys_time);
        }

        self.cycle_count = self.cycle_count.overflowing_add(1).0;
        Ok(self.cycle_count)
    }

    pub fn received_mailbox<'a>(
        &'a self,
    ) -> Option<Result<(MailboxSessionId, MailboxFrame<&'a [u8]>), TaskError<MailboxTaskError>>>
    {
        let mb_socket = self.sif.get_socket(&self.mailbox_handle).unwrap();
        self.mailbox_manager.received_mailbox(mb_socket)
    }

    pub fn try_get_mailbox_request_interface<'a>(
        &'a mut self,
    ) -> Option<MailboxReqIfWrapper<'a, 'a, 'frame, 'socket, D>> {
        let mif = self.mailbox_manager.try_get_mailbox_request_interface()?;

        Some(MailboxReqIfWrapper::new(
            mif,
            &mut self.sif,
            self.mailbox_handle.clone(),
        ))
    }

    pub fn rx_error_count(&self) -> &RxErrorCounter<[u8; RxErrorCounter::SIZE]> {
        self.rx_error_task.rx_error_count()
    }

    pub fn al_state(&self) -> (Option<AlState>, Option<AlStatusCode>) {
        self.al_state_task.last_al_state()
    }

    pub fn invalid_wkc_count(&self) -> usize {
        self.process_data_task.invalid_wkc_count
    }

    pub fn lost_frame_count(&self) -> usize {
        self.sif.lost_frame_count
    }

    pub fn detected_slave_count(&self) -> usize {
        todo!()
    }

    pub fn request_al_state(&mut self, al_state: AlState) -> bool {
        let Self {
            network,
            al_tf_task,
            ..
        } = self;
        if !al_tf_task.is_busy() {
            al_tf_task.start(TargetSlave::All(network.num_slaves()), al_state);
            true
        } else {
            false
        }
    }

    pub fn read_al_state(
        &mut self,
        target_slave: TargetSlave,
    ) -> Result<(AlState, Option<AlStatusCode>), TaskError<()>> {
        self.sif.read_al_state(&self.gp_socket_handle, target_slave)
    }

    pub fn change_al_state(
        &mut self,
        target_slave: TargetSlave,
        al_state: AlState,
    ) -> Result<AlState, TaskError<AlStateTransferTaskError>> {
        self.sif
            .change_al_state(&self.gp_socket_handle, target_slave, al_state)
    }

    pub fn read_sii(
        &mut self,
        slave_address: SlaveAddress,
        sii_address: u16,
    ) -> Result<(SiiData<[u8; SiiData::SIZE]>, usize), TaskError<SiiTaskError>> {
        self.sif
            .read_sii(&self.gp_socket_handle, slave_address, sii_address)
    }

    pub fn read_sdo(
        &mut self,
        slave_address: SlaveAddress,
        index: u16,
        sub_index: u8,
    ) -> Result<&[u8], TaskError<SdoErrorKind>> {
        let (slave, _) = self.network.slave(slave_address).expect("slave not found");
        self.sif
            .read_sdo(&self.gp_socket_handle, slave, index, sub_index)
    }

    pub fn write_sdo(
        &mut self,
        slave_address: SlaveAddress,
        index: u16,
        sub_index: u8,
        data: &[u8],
    ) -> Result<(), TaskError<SdoErrorKind>> {
        let (slave, _) = self.network.slave(slave_address).expect("slave not found");
        self.sif
            .write_sdo(&self.gp_socket_handle, slave, index, sub_index, data)
    }

    pub fn read_pdo(
        &self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
        buf: &mut [u8],
    ) -> Option<()> {
        let handle = self.process_data_handle.as_ref().map(|handle| handle)?;
        let pdo_image = self.sif.get_socket(handle)?;
        let (_, config) = self.network().slave(slave_address)?;
        config
            .input_process_data_mappings()
            .get(pdo_map_index)
            .map(|pdo_map| pdo_map.entries.get(pdo_entry_index))
            .flatten()
            .map(|pdo_entry| {
                pdo_entry.read_to_buffer(LOGICAL_START_ADDRESS, pdo_image.data_buf(), buf)
            })
            .flatten()
    }

    pub fn write_pdo(
        &mut self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
        data: &[u8],
    ) -> Option<()> {
        let Self {
            sif,
            process_data_handle,
            ..
        } = self;

        let handle = process_data_handle.as_ref().map(|handle| handle)?;

        let pdo_image = sif.get_socket_mut(handle)?;

        let (_, config) = self.network.slave(slave_address)?;

        config
            .output_process_data_mappings()
            .get(pdo_map_index)
            .map(|pdo_map| pdo_map.entries.get(pdo_entry_index))
            .flatten()
            .map(|pdo_entry| {
                pdo_entry.write_from_buffer(LOGICAL_START_ADDRESS, pdo_image.data_buf_mut(), data)
            })
            .flatten()
    }

    pub fn read_register(
        &mut self,
        target_slave: TargetSlave,
        register_address: u16,
        data_size: usize,
    ) -> Result<&[u8], TaskError<()>> {
        let Self {
            sif,
            gp_socket_handle,
            ..
        } = self;
        sif.read_register(&gp_socket_handle, target_slave, register_address, data_size)
    }

    pub fn write_register(
        &mut self,
        target_slave: TargetSlave,
        register_address: u16,
        data: &[u8],
    ) -> Result<(), TaskError<()>> {
        let Self {
            sif,
            gp_socket_handle,
            ..
        } = self;
        sif.write_register(&gp_socket_handle, target_slave, register_address, data)
    }
}
