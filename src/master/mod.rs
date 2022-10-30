mod error;
pub mod mailbox;
pub mod sdo;
pub use error::*;

use crate::{
    frame::{MailboxFrame, MAX_PDU_DATAGRAM},
    interface::{
        PduInterface, PduSocket, PhyError, RawEthernetDevice, SlaveAddress, SocketHandle,
        SocketInterface, TargetSlave,
    },
    register::{
        od::OdPdoEntry, AlStatusCode, CyclicOperationStartTime, DcActivation, FmmuRegister,
        RxErrorCounter, SiiData, Sync0CycleTime, Sync1CycleTime, SyncManagerActivation,
        SyncManagerControl,
    },
    slave::{AlState, Direction, FmmuConfig, Network, PdoMapping, Slave, SlaveConfig, SyncMode},
    task::{
        loop_task::*, AlStateTransferTaskError, CyclicTask, EtherCatSystemTime, MailboxTask,
        MailboxTaskError, NetworkInitTaskError, SdoTaskError, SiiTaskError, TaskError, MAX_SM_SIZE,
    },
};

use self::mailbox::MailboxManager;

const LOGICAL_START_ADDRESS: u32 = 0x1000;
const NUM_SOCKETS: usize = 5;

#[derive(Debug)]
pub struct EtherCatMaster<'frame, 'socket, 'slave, 'pdo_mapping, 'pdo_entry, D>
where
    D: for<'d> RawEthernetDevice<'d>,
{
    sif: SocketInterface<'frame, 'socket, D, NUM_SOCKETS>,
    network: Network<'slave, 'pdo_mapping, 'pdo_entry>,
    gp_socket_handle: SocketHandle,
    cycle_count: usize,
    //mailbox
    mailbox_handle: SocketHandle,
    mailbox_task: MailboxManager,
    //process data
    process_data_handle: Option<SocketHandle>,
    process_data_task: ProcessTask,
    //dc drift
    dc_handle: SocketHandle,
    dc_task: Option<DcSyncTask>,
    //alstate
    al_state_handle: SocketHandle,
    al_state_task: AlStateReadTask,
    //rx error
    rx_error_handle: SocketHandle,
    rx_error_task: RxErrorReadTask,
}

impl<'frame, 'socket, 'slave, 'pdo_mapping, 'pdo_entry, D>
    EtherCatMaster<'frame, 'socket, 'slave, 'pdo_mapping, 'pdo_entry, D>
where
    D: for<'d> RawEthernetDevice<'d>,
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
            + MAX_SM_SIZE as usize
            + MAX_SM_SIZE as usize;
        assert!(MINIMUM_REQUIRED_BUFFER_SIZE < socket_buffer.len());
        let (pdu_buffer1, rest) =
            socket_buffer.split_at_mut(AlStateReadTask::required_buffer_size());
        let (pdu_buffer2, rest) = rest.split_at_mut(RxErrorReadTask::required_buffer_size());
        let (pdu_buffer3, rest) = rest.split_at_mut(DcSyncTask::required_buffer_size());
        let (pdu_buffer4, rest) = rest.split_at_mut(MAX_SM_SIZE as usize);
        let (pdu_buffer5, _) = rest.split_at_mut(MAX_SM_SIZE as usize);

        let mut sif = SocketInterface::new(iface);
        let al_state_handle = sif.add_socket(PduSocket::new(pdu_buffer1)).unwrap();
        let rx_error_handle = sif.add_socket(PduSocket::new(pdu_buffer2)).unwrap();
        let dc_handle = sif.add_socket(PduSocket::new(pdu_buffer3)).unwrap();
        let gp_socket_handle = sif.add_socket(PduSocket::new(pdu_buffer4)).unwrap();
        let mailbox_handle = sif.add_socket(PduSocket::new(pdu_buffer5)).unwrap();

        let network = Network::new(slave_buf);
        Self {
            sif,
            network,
            gp_socket_handle,
            cycle_count: 0,
            mailbox_handle,
            mailbox_task: MailboxManager::new(MailboxTask::new()),
            process_data_handle: None,
            process_data_task: ProcessTask::new(LOGICAL_START_ADDRESS, 0, 0),
            dc_handle,
            dc_task: None,
            al_state_task: AlStateReadTask::new(),
            al_state_handle,
            rx_error_task: RxErrorReadTask::new(),
            rx_error_handle,
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

    /// Easy setup API. Use this in PreOperational state.
    pub fn configure_slaves_for_operation(&mut self) -> Result<(), ConfigError> {
        self.configure_pdo_image()?;
        self.configure_sync_mode()?;
        Ok(())
    }

    pub fn network<'a>(&'a self) -> &'a Network<'slave, 'pdo_mapping, 'pdo_entry> {
        &self.network
    }

    /// Return process data size
    pub fn process_data_size(&self) -> usize {
        self.process_data_task.image_size()
    }

    /// If the buffer size is smaller than the image size, return false.
    pub fn register_process_data_buffer(&mut self, buf: &'socket mut [u8]) -> bool {
        if buf.len() < self.process_data_size() {
            return false;
        }
        let process_data_handle = self.sif.add_socket(PduSocket::new(buf)).unwrap();
        self.process_data_handle = Some(process_data_handle);
        true
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
            mailbox_task,
            process_data_handle,
            process_data_task,
            dc_handle,
            dc_task,
            al_state_handle,
            al_state_task,
            rx_error_handle,
            rx_error_task,
            ..
        } = self;

        // process data + mb polling
        if let (Some(ref handle), ref mut task) = (process_data_handle, process_data_task) {
            {
                let socket = self.sif.get_socket_mut(handle).unwrap();
                task.process_one_step(socket, sys_time);
            }
            let mb_socket = self.sif.get_socket_mut(mailbox_handle).unwrap();
            mailbox_task.process_one_step(&network, mb_socket, sys_time);
            let socket = self.sif.get_socket(handle).unwrap();
            mailbox_task.find_slave_with_mailbox_from_process_data(
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

        self.cycle_count = self.cycle_count.overflowing_add(1).0;
        Ok(self.cycle_count)
    }

    pub fn try_get_sdo_task(&mut self) {
        todo!()
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

    pub fn read_mailbox(
        &mut self,
        slave_address: SlaveAddress,
        wait_full: bool,
    ) -> Result<(MailboxFrame<&[u8]>, &[u8]), TaskError<MailboxTaskError>> {
        let (slave, _) = self.network.slave(slave_address).expect("slave not found");
        let slave_info = slave.info();
        self.sif
            .read_mailbox(&self.gp_socket_handle, slave_info, wait_full)
    }

    pub fn write_mailbox(
        &mut self,
        slave_address: SlaveAddress,
        mb_header: &MailboxFrame<[u8; MailboxFrame::HEADER_SIZE]>,
        mb_data: &[u8],
        wait_empty: bool,
    ) -> Result<(), TaskError<MailboxTaskError>> {
        let (slave, _) = self.network.slave(slave_address).expect("slave not found");
        let slave_info = slave.info();
        self.sif.write_mailbox(
            &self.gp_socket_handle,
            slave_info,
            mb_header,
            mb_data,
            wait_empty,
        )
    }

    pub fn read_sdo(
        &mut self,
        slave_address: SlaveAddress,
        index: u16,
        sub_index: u8,
    ) -> Result<&[u8], TaskError<SdoTaskError>> {
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
    ) -> Result<(), TaskError<SdoTaskError>> {
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

    pub fn read_pdo_as_bool(
        &self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
    ) -> Option<bool> {
        let mut buf = [0];
        self.read_pdo(slave_address, pdo_map_index, pdo_entry_index, &mut buf)?;
        Some(buf[0] & 1 == 1)
    }

    pub fn read_pdo_as_u8(
        &self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
    ) -> Option<u8> {
        let mut buf = [0];
        self.read_pdo(slave_address, pdo_map_index, pdo_entry_index, &mut buf)?;
        Some(u8::from_le_bytes(buf))
    }

    pub fn read_pdo_as_i8(
        &self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
    ) -> Option<i8> {
        let mut buf = [0];
        self.read_pdo(slave_address, pdo_map_index, pdo_entry_index, &mut buf)?;
        Some(i8::from_le_bytes(buf))
    }

    pub fn read_pdo_as_u16(
        &self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
    ) -> Option<u16> {
        let mut buf = [0; 2];
        self.read_pdo(slave_address, pdo_map_index, pdo_entry_index, &mut buf)?;
        Some(u16::from_le_bytes(buf))
    }

    pub fn read_pdo_as_i16(
        &self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
    ) -> Option<i16> {
        let mut buf = [0; 2];
        self.read_pdo(slave_address, pdo_map_index, pdo_entry_index, &mut buf)?;
        Some(i16::from_le_bytes(buf))
    }

    pub fn read_pdo_as_u32(
        &self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
    ) -> Option<u32> {
        let mut buf = [0; 4];
        self.read_pdo(slave_address, pdo_map_index, pdo_entry_index, &mut buf)?;
        Some(u32::from_le_bytes(buf))
    }

    pub fn read_pdo_as_i32(
        &self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
    ) -> Option<i32> {
        let mut buf = [0; 4];
        self.read_pdo(slave_address, pdo_map_index, pdo_entry_index, &mut buf)?;
        Some(i32::from_le_bytes(buf))
    }

    pub fn read_pdo_as_u64(
        &self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
    ) -> Option<u64> {
        let mut buf = [0; 8];
        self.read_pdo(slave_address, pdo_map_index, pdo_entry_index, &mut buf)?;
        Some(u64::from_le_bytes(buf))
    }

    pub fn read_pdo_as_i64(
        &self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
    ) -> Option<i64> {
        let mut buf = [0; 8];
        self.read_pdo(slave_address, pdo_map_index, pdo_entry_index, &mut buf)?;
        Some(i64::from_le_bytes(buf))
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

    pub fn write_pdo_as_bool(
        &mut self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
        data: bool,
    ) -> Option<()> {
        let buf = [data as u8];
        self.write_pdo(slave_address, pdo_map_index, pdo_entry_index, &buf)
    }

    pub fn write_pdo_as_u8(
        &mut self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
        data: u8,
    ) -> Option<()> {
        let buf = data.to_le_bytes();
        self.write_pdo(slave_address, pdo_map_index, pdo_entry_index, &buf)
    }

    pub fn write_pdo_as_i8(
        &mut self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
        data: i8,
    ) -> Option<()> {
        let buf = data.to_le_bytes();
        self.write_pdo(slave_address, pdo_map_index, pdo_entry_index, &buf)
    }

    pub fn write_pdo_as_u16(
        &mut self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
        data: u16,
    ) -> Option<()> {
        let buf = data.to_le_bytes();
        self.write_pdo(slave_address, pdo_map_index, pdo_entry_index, &buf)
    }

    pub fn write_pdo_as_i16(
        &mut self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
        data: i16,
    ) -> Option<()> {
        let buf = data.to_le_bytes();
        self.write_pdo(slave_address, pdo_map_index, pdo_entry_index, &buf)
    }

    pub fn write_pdo_as_u32(
        &mut self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
        data: u32,
    ) -> Option<()> {
        let buf = data.to_le_bytes();
        self.write_pdo(slave_address, pdo_map_index, pdo_entry_index, &buf)
    }

    pub fn write_pdo_as_i32(
        &mut self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
        data: i32,
    ) -> Option<()> {
        let buf = data.to_le_bytes();
        self.write_pdo(slave_address, pdo_map_index, pdo_entry_index, &buf)
    }

    pub fn write_pdo_as_u64(
        &mut self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
        data: u64,
    ) -> Option<()> {
        let buf = data.to_le_bytes();
        self.write_pdo(slave_address, pdo_map_index, pdo_entry_index, &buf)
    }

    pub fn write_pdo_as_i64(
        &mut self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
        data: i64,
    ) -> Option<()> {
        let buf = data.to_le_bytes();
        self.write_pdo(slave_address, pdo_map_index, pdo_entry_index, &buf)
    }

    /// Easy API for configuration of PDO mappings
    fn configure_pdo_image(&mut self) -> Result<(), ConfigError> {
        self.set_pdo_config_to_od()?;
        self.set_pdo_to_sm()?;
        let (image_size, expected_wkc) = self.configure_fmmu()?;
        self.process_data_task.set_image_size(image_size);
        self.process_data_task.set_expected_wkc(expected_wkc);
        self.set_logical_address_to_pdo_entry_config();
        Ok(())
    }

    /// Set the PDO map config to object dictionary.
    fn set_pdo_config_to_od(&mut self) -> Result<(), ConfigError> {
        let Self {
            network,
            sif,
            gp_socket_handle,
            ..
        } = self;
        let handle = &gp_socket_handle;
        for (slave, slave_config) in network.slaves_mut() {
            set_pdo_config_to_od_utility(slave, slave_config, sif, handle, true)?;
            set_pdo_config_to_od_utility(slave, slave_config, sif, handle, false)?;
        }
        Ok(())
    }

    /// Assign the PDO map to the sync manager.
    fn set_pdo_to_sm(&mut self) -> Result<(), ConfigError> {
        let Self {
            network,
            sif,
            gp_socket_handle,
            ..
        } = self;
        let handle = &gp_socket_handle;
        for (slave, _) in network.slaves_mut() {
            if let Some(ram_address) = slave.info().process_data_physical_start_address() {
                let next_ram_address =
                    set_pdo_to_sm_utility(slave, sif, handle, Direction::Output, ram_address)?;
                set_pdo_to_sm_utility(slave, sif, handle, Direction::Input, next_ram_address)?;
            }
        }
        Ok(())
    }

    /// Return image size and expected wkc.
    fn set_logical_address_to_fmmu_config(&mut self) -> (usize, u16) {
        let mut expected_wkc = 0;
        let mut bit_address = (LOGICAL_START_ADDRESS * 8) as u64;
        for (slave, _) in self.network.slaves_mut() {
            let mut has_tx_data = false;
            let mut has_rx_data = false;

            for fmmu_config in slave
                .fmmu_config_mut()
                .iter_mut()
                .filter_map(|f| f.as_mut())
                .filter(|f| f.bit_length() != 0)
            {
                fmmu_config.set_logical_address(Some((bit_address >> 3) as u32));
                fmmu_config.set_start_bit((bit_address % 8) as u8);
                bit_address += fmmu_config.bit_length() as u64;

                match fmmu_config.direction() {
                    Direction::Output => has_rx_data = true,
                    Direction::Input => has_tx_data = true,
                }
            }

            if has_tx_data {
                expected_wkc += 1;
            }
            if has_rx_data {
                expected_wkc += 2;
            }
        }
        let size = if bit_address % 8 == 0 {
            (bit_address >> 3) - LOGICAL_START_ADDRESS as u64
        } else {
            (bit_address >> 3) + 1 - LOGICAL_START_ADDRESS as u64
        };
        assert!(
            size <= MAX_PDU_DATAGRAM as u64,
            "process data size must be less than or equal to 1468 for now"
        );
        (size as usize, expected_wkc)
    }

    fn set_logical_address_to_pdo_entry_config(&mut self) {
        for (slave, config) in self.network.slaves_mut() {
            // i=0 -> RX
            // i=1 -> TX
            for i in 0..2 {
                if slave.fmmu_config()[i].is_none()
                    || slave.fmmu_config()[i]
                        .as_ref()
                        .unwrap()
                        .logical_address()
                        .is_none()
                {
                    continue;
                }
                let fmmu_config = slave.fmmu_config()[i].as_ref().unwrap();
                let start_address = fmmu_config.logical_address().unwrap();
                let start_bit = fmmu_config.start_bit() as u64;
                let mut total_bits = (start_address * 8) as u64 + start_bit;
                let pdo_maps = if i == 0 {
                    config.output_process_data_mappings_mut()
                } else {
                    config.input_process_data_mappings_mut()
                };
                if pdo_maps.is_empty() {
                    continue;
                }
                for pdo_map in pdo_maps {
                    for pdo_entry in pdo_map.entries.iter_mut() {
                        let mod8 = total_bits % 8;
                        let (addr, s_bit) = if mod8 == 0 {
                            (total_bits >> 3, 0)
                        } else {
                            ((total_bits >> 3) + 1, mod8)
                        };
                        pdo_entry.set_start_bit(s_bit as u8);
                        pdo_entry.set_logical_address(Some(addr as u32));
                        total_bits += pdo_entry.bit_length() as u64;
                    }
                }
                assert_eq!(
                    fmmu_config.bit_length() as u64,
                    total_bits - ((start_address * 8) as u64 + start_bit)
                );
            }
        }
    }

    /// Set the logical address, physical address, and size for each slave FMMU.
    /// Return process data_image size and expected wkc.
    pub fn configure_fmmu(&mut self) -> Result<(usize, u16), ConfigError> {
        let (image_size, expected_wkc) = self.set_logical_address_to_fmmu_config();
        let Self {
            network,
            sif,
            gp_socket_handle,
            ..
        } = self;
        for (slave, _) in network.slaves() {
            for (i, fmmu) in slave
                .fmmu_config()
                .iter()
                .enumerate()
                .filter(|(_, f)| f.is_some())
            {
                let fmmu = fmmu.as_ref().unwrap();
                if fmmu.logical_address().is_none() || fmmu.byte_length() == 0 {
                    continue;
                }
                let mut fmmu_reg = FmmuRegister::new();
                fmmu_reg.set_logical_start_address(fmmu.logical_address().unwrap());
                let byte_length = fmmu.byte_length();
                let end_bit = fmmu.end_bit();
                fmmu_reg.set_length(byte_length);
                fmmu_reg.set_logical_end_bit(end_bit);
                fmmu_reg.set_physical_start_address(fmmu.physical_address());
                fmmu_reg.set_physical_start_bit(0);
                match fmmu.direction() {
                    Direction::Output => {
                        fmmu_reg.set_read_enable(false);
                        fmmu_reg.set_write_enable(true);
                        dbg!(&fmmu_reg.read_enable());
                        dbg!(&fmmu_reg.write_enable());
                    }
                    Direction::Input => {
                        fmmu_reg.set_read_enable(true);
                        fmmu_reg.set_write_enable(false);
                        dbg!(&fmmu_reg.read_enable());
                        dbg!(&fmmu_reg.write_enable());
                    }
                }
                fmmu_reg.set_enable(true);
                let addr = FmmuRegister::ADDRESS + (i as u16) * FmmuRegister::SIZE as u16;
                sif.write_register(
                    gp_socket_handle,
                    slave.info().slave_address().into(),
                    addr,
                    &fmmu_reg.0,
                )
                .map_err(|err| ConfigError {
                    slave_address: slave.info().slave_address(),
                    kind: ConfigErrorKind::SetFmmuRegister(RegisterError {
                        address: addr,
                        error: err,
                    }),
                })?;
            }
        }
        Ok((image_size, expected_wkc))
    }

    fn configure_sync_mode(&mut self) -> Result<(), ConfigError> {
        let Self {
            sif,
            network,
            gp_socket_handle,
            ..
        } = self;
        //TODO：そもそも同期パラメタを持たないスレーブがあるので、どうにかする。
        //TODO：同期パラメタ数は1~3で、オプションなので、どうにかする。
        for (slave, config) in network.slaves().filter(|(s, _)| s.info().support_coe()) {
            // Set Operation Mode
            let index = 0x1C00;
            let sub_index = slave.info().process_data_rx_sm_number().unwrap() + 1;
            let has_rx_sm = match sif
                .read_sdo(&gp_socket_handle, slave, index, sub_index)
                .map_err(|err| ConfigError {
                    slave_address: slave.info().slave_address(),
                    kind: ConfigErrorKind::GetSyncManagerCommunicationType(SdoError {
                        index,
                        sub_index,
                        error: err,
                    }),
                })?[0]
            {
                0 => false,
                3 => true,
                _ => panic!("unsupported sm type"),
            };
            if has_rx_sm {
                let addr = 0x1C30 + slave.info().process_data_rx_sm_number().unwrap() as u16;
                sif.write_sdo(
                    &gp_socket_handle,
                    slave,
                    addr,
                    1,
                    &[config.sync_mode as u8, 0],
                )
                .map_err(|err| ConfigError {
                    slave_address: slave.info().slave_address(),
                    kind: ConfigErrorKind::SetSyncManagerSyncType(SdoError {
                        index: addr,
                        sub_index: 1,
                        error: err,
                    }),
                })?;
                // Cycle time
                sif.write_sdo(
                    &gp_socket_handle,
                    slave,
                    addr,
                    2,
                    &config.cycle_time_ns.to_le_bytes(),
                )
                .map_err(|err| ConfigError {
                    slave_address: slave.info().slave_address(),
                    kind: ConfigErrorKind::SetSyncManagerCycleTime(SdoError {
                        index: addr,
                        sub_index: 2,
                        error: err,
                    }),
                })?;
            }
            let index = 0x1C00;
            let sub_index = slave.info().process_data_tx_sm_number().unwrap() + 1;
            let has_tx_sm = match sif
                .read_sdo(&gp_socket_handle, slave, index, sub_index)
                .map_err(|err| ConfigError {
                    slave_address: slave.info().slave_address(),
                    kind: ConfigErrorKind::GetSyncManagerCommunicationType(SdoError {
                        index,
                        sub_index,
                        error: err,
                    }),
                })?[0]
            {
                0 => false,
                4 => true,
                _ => panic!("unsupproted sm type"),
            };
            if has_tx_sm {
                let addr = 0x1C30 + slave.info().process_data_tx_sm_number().unwrap() as u16;
                sif.write_sdo(
                    &gp_socket_handle,
                    slave,
                    addr,
                    1,
                    &[config.sync_mode as u8, 0],
                )
                .map_err(|err| ConfigError {
                    slave_address: slave.info().slave_address(),
                    kind: ConfigErrorKind::SetSyncManagerSyncType(SdoError {
                        index: addr,
                        sub_index: 1,
                        error: err,
                    }),
                })?;
                // Cycle time
                sif.write_sdo(
                    &gp_socket_handle,
                    slave,
                    addr,
                    2,
                    &config.cycle_time_ns.to_le_bytes(),
                )
                .map_err(|err| ConfigError {
                    slave_address: slave.info().slave_address(),
                    kind: ConfigErrorKind::SetSyncManagerCycleTime(SdoError {
                        index: addr,
                        sub_index: 2,
                        error: err,
                    }),
                })?;
            }

            // Set interval of sync signal
            sif.write_register(
                &gp_socket_handle,
                slave.info().slave_address().into(),
                Sync0CycleTime::ADDRESS,
                &config.cycle_time_ns.to_le_bytes(),
            )
            .map_err(|err| ConfigError {
                slave_address: slave.info().slave_address(),
                kind: ConfigErrorKind::SetSync0CycleTime(RegisterError {
                    address: Sync0CycleTime::ADDRESS,
                    error: err,
                }),
            })?;
            sif.write_register(
                &gp_socket_handle,
                slave.info().slave_address().into(),
                Sync1CycleTime::ADDRESS,
                &(config.cycle_time_ns >> 1).to_le_bytes(),
            )
            .map_err(|err| ConfigError {
                slave_address: slave.info().slave_address(),
                kind: ConfigErrorKind::SetSync1CycleTime(RegisterError {
                    address: Sync1CycleTime::ADDRESS,
                    error: err,
                }),
            })?;
            // Start SYNC Signal
            sif.write_register(
                &gp_socket_handle,
                slave.info().slave_address().into(),
                CyclicOperationStartTime::ADDRESS,
                &0_u64.to_le_bytes(), //TODO
            )
            .map_err(|err| ConfigError {
                slave_address: slave.info().slave_address(),
                kind: ConfigErrorKind::SetSyncSignalStartTime(RegisterError {
                    address: CyclicOperationStartTime::ADDRESS,
                    error: err,
                }),
            })?;

            // cycle permission
            let mut dc_actiation = DcActivation::new();
            if let SyncMode::Sync0Event | SyncMode::Sync1Event = config.sync_mode {
                dc_actiation.set_cyclic_operation_enable(true);
                dc_actiation.set_sync0_activate(true);
                dc_actiation.set_sync1_activate(true);
            } else {
                dc_actiation.set_cyclic_operation_enable(false);
                dc_actiation.set_sync0_activate(false);
                dc_actiation.set_sync1_activate(false);
            }
            sif.write_register(
                &gp_socket_handle,
                slave.info().slave_address().into(),
                DcActivation::ADDRESS,
                &dc_actiation.0,
            )
            .map_err(|err| ConfigError {
                slave_address: slave.info().slave_address(),
                kind: ConfigErrorKind::ActivateDc(RegisterError {
                    address: DcActivation::ADDRESS,
                    error: err,
                }),
            })?;
        }
        Ok(())
    }
}

/// Set PDO map to obejct dictionary.
fn set_pdo_config_to_od_utility<
    'frame,
    'socket,
    'pdo_mapping,
    'pdo_entry,
    D: for<'d> RawEthernetDevice<'d>,
>(
    slave: &mut Slave,
    slave_config: &mut SlaveConfig<'pdo_mapping, 'pdo_entry>,
    sif: &mut SocketInterface<'frame, 'socket, D, NUM_SOCKETS>,
    handle: &SocketHandle,
    is_output: bool,
) -> Result<(), ConfigError> {
    let (pdo_mappings, sm_number) = if is_output {
        (
            slave_config.output_process_data_mappings(),
            slave.info().process_data_rx_sm_number(),
        )
    } else {
        (
            slave_config.input_process_data_mappings(),
            slave.info().process_data_tx_sm_number(),
        )
    };
    if let (Some(sm_number), pdo_mappings) = (sm_number, pdo_mappings) {
        let sm_assign = 0x1C10 + sm_number as u16;

        // Clear PDO mappings
        sif.write_sdo(handle, slave, sm_assign, 0, &[0])
            .map_err(|err| ConfigError {
                slave_address: slave.info().slave_address(),
                kind: ConfigErrorKind::ClearPdoMappings(SdoError {
                    index: sm_assign,
                    sub_index: 0,
                    error: err,
                }),
            })?;

        let mut map_index = 0;
        for pdo_map in pdo_mappings.iter() {
            if pdo_map.entries.is_empty() {
                continue;
            }
            map_index += 1;
            let PdoMapping {
                is_fixed,
                index: pdo_map_index,
                entries,
            } = pdo_map;
            // Assign pdo map to SM
            sif.write_sdo(
                handle,
                slave,
                sm_assign,
                map_index,
                &pdo_map_index.to_le_bytes(),
            )
            .map_err(|err| ConfigError {
                slave_address: slave.info().slave_address(),
                kind: ConfigErrorKind::AssignPdoMapToSyncManager(SdoError {
                    index: sm_assign,
                    sub_index: map_index,
                    error: err,
                }),
            })?;
            if *is_fixed {
                continue;
            }

            // Clear PDO entry of PDO map
            sif.write_sdo(handle, slave, *pdo_map_index, 0, &[0])
                .map_err(|err| ConfigError {
                    slave_address: slave.info().slave_address(),
                    kind: ConfigErrorKind::ClearPdoEntries(SdoError {
                        index: *pdo_map_index,
                        sub_index: 0,
                        error: err,
                    }),
                })?;
            let mut entry_index = 0;
            for entry in entries.iter() {
                let mut od_pdo_entry = OdPdoEntry::new();
                od_pdo_entry.set_index(entry.index());
                od_pdo_entry.set_sub_index(entry.sub_index());
                od_pdo_entry.set_bit_length(entry.bit_length());
                entry_index += 1;
                // Assign PDO entry to PDO map
                sif.write_sdo(handle, slave, *pdo_map_index, entry_index, &od_pdo_entry.0)
                    .map_err(|err| ConfigError {
                        slave_address: slave.info().slave_address(),
                        kind: ConfigErrorKind::AssignPdoEntryToPdoMap(SdoError {
                            index: *pdo_map_index,
                            sub_index: entry_index,
                            error: err,
                        }),
                    })?;
            }
            // How many entries were assigned to the PDO map?
            sif.write_sdo(handle, slave, *pdo_map_index, 0, &entry_index.to_le_bytes())
                .map_err(|err| ConfigError {
                    slave_address: slave.info().slave_address(),
                    kind: ConfigErrorKind::SetNumberOfPdoEntries(SdoError {
                        index: *pdo_map_index,
                        sub_index: 0,
                        error: err,
                    }),
                })?;
        }
        // How many PDO maps were assigned to the SM?
        sif.write_sdo(handle, slave, sm_assign, 0, &map_index.to_le_bytes())
            .map_err(|err| ConfigError {
                slave_address: slave.info().slave_address(),
                kind: ConfigErrorKind::SetNumberOfPdoMappings(SdoError {
                    index: sm_assign,
                    sub_index: 0,
                    error: err,
                }),
            })?;
    }
    Ok(())
}

/// Assign PDO map to SM.
/// Return next pdo ram address
/// NOTE: output = RX of slave.
fn set_pdo_to_sm_utility<
    'frame,
    'socket,
    'pdo_mapping,
    'pdo_entry,
    D: for<'d> RawEthernetDevice<'d>,
>(
    slave: &mut Slave,
    sif: &mut SocketInterface<'frame, 'socket, D, NUM_SOCKETS>,
    handle: &SocketHandle,
    direction: Direction,
    start_ram_address: u16,
) -> Result<u16, ConfigError> {
    let num_sm_comm = sif
        .read_sdo(handle, slave, 0x1C00, 0)
        .map_err(|err| ConfigError {
            slave_address: slave.info().slave_address(),
            kind: ConfigErrorKind::GetNumberOfSyncManagerChannel(SdoError {
                index: 0x1C00,
                sub_index: 0,
                error: err,
            }),
        })?;
    assert!(4 <= num_sm_comm[0]);

    let sm_number = match direction {
        Direction::Output => slave.info().process_data_rx_sm_number(),
        Direction::Input => slave.info().process_data_tx_sm_number(),
    };

    if let Some(sm_number) = sm_number {
        // Read SM type
        let is_pdo_map_none = if let Direction::Output = direction {
            let sm_type = sif
                .read_sdo(handle, slave, 0x1C00, sm_number + 1)
                .map_err(|err| ConfigError {
                    slave_address: slave.info().slave_address(),
                    kind: ConfigErrorKind::GetSyncManagerCommunicationType(SdoError {
                        index: 0x1C00,
                        sub_index: sm_number + 1,
                        error: err,
                    }),
                })?[0];
            match sm_type {
                0 => true,
                3 => false,
                _ => panic!("unsupported sm type"),
            }
        } else {
            let sm_type = sif
                .read_sdo(handle, slave, 0x1C00, sm_number + 1)
                .map_err(|err| ConfigError {
                    slave_address: slave.info().slave_address(),
                    kind: ConfigErrorKind::GetSyncManagerCommunicationType(SdoError {
                        index: 0x1C00,
                        sub_index: sm_number + 1,
                        error: err,
                    }),
                })?[0];
            match sm_type {
                0 => true,
                4 => false,
                _ => panic!("unsupproted sm type"),
            }
        };

        let mut pdo_map_bit_length = 0;
        let sm_assign = 0x1C10 + sm_number as u16;
        if !is_pdo_map_none {
            // Read PDO Maps and Entries from Obeject Dictiory.
            let num_maps = sif
                .read_sdo(handle, slave, sm_assign, 0)
                .map_err(|err| ConfigError {
                    slave_address: slave.info().slave_address(),
                    kind: ConfigErrorKind::GetNumberOfPdoMappings(SdoError {
                        index: sm_assign,
                        sub_index: 0,
                        error: err,
                    }),
                })?[0] as usize;
            for index in 1..(num_maps + 1) {
                let map_address = sif
                    .read_sdo(handle, slave, sm_assign, index as u8)
                    .map_err(|err| ConfigError {
                        slave_address: slave.info().slave_address(),
                        kind: ConfigErrorKind::GetPdoMappingAddress(SdoError {
                            index: sm_assign,
                            sub_index: index as u8,
                            error: err,
                        }),
                    })?;
                let map_address = u16::from_le_bytes([map_address[0], map_address[1]]);
                let num_entry =
                    sif.read_sdo(handle, slave, map_address, 0)
                        .map_err(|err| ConfigError {
                            slave_address: slave.info().slave_address(),
                            kind: ConfigErrorKind::GetNumberOfPdoEntries(SdoError {
                                index: map_address,
                                sub_index: 0,
                                error: err,
                            }),
                        })?[0] as usize;
                for entry_index in 1..(num_entry + 1) {
                    let entry = sif
                        .read_sdo(handle, slave, map_address, entry_index as u8)
                        .map_err(|err| ConfigError {
                            slave_address: slave.info().slave_address(),
                            kind: ConfigErrorKind::GetPdoEntrtyAddress(SdoError {
                                index: map_address,
                                sub_index: entry_index as u8,
                                error: err,
                            }),
                        })?;
                    let entry = OdPdoEntry(entry);
                    pdo_map_bit_length += entry.bit_length() as u32;
                }
            }
        }
        let size = if pdo_map_bit_length % 8 == 0 {
            pdo_map_bit_length >> 3
        } else {
            (pdo_map_bit_length >> 3) + 1
        };
        let size = size as u16;

        // Configure sm control register
        let sm_control_address = SyncManagerControl::ADDRESS + 0x08 * sm_number as u16;
        let mut sm_control = SyncManagerControl::new();
        sm_control.set_physical_start_address(start_ram_address);
        sm_control.set_length(size);
        sm_control.set_buffer_type(0b00); //buffer mode
        if let Direction::Output = direction {
            sm_control.set_direction(1); //pdi read access
        } else {
            sm_control.set_direction(0); //pdi write access
        }
        sm_control.set_dls_user_event_enable(true);
        sif.write_register(
            handle,
            slave.info().slave_address().into(),
            sm_control_address,
            &sm_control.0,
        )
        .unwrap(); //unwrap for now

        // Configure sm activation register
        let sm_activation_address = SyncManagerActivation::ADDRESS + 0x08 * sm_number as u16;
        let mut sm_active = SyncManagerActivation::new();
        if size != 0 {
            sm_active.set_channel_enable(true);
        } else {
            sm_active.set_channel_enable(false);
        }
        sm_active.set_repeat(false);
        sif.write_register(
            handle,
            slave.info().slave_address().into(),
            sm_activation_address,
            &sm_active.0,
        )
        .unwrap(); //unwrap for now

        // Set FMMU config of slave struct
        let fmmu_config = FmmuConfig::new(start_ram_address, size * 8, direction);
        if let Direction::Output = direction {
            slave.fmmu_config_mut()[0] = Some(fmmu_config);
        } else {
            slave.fmmu_config_mut()[1] = Some(fmmu_config);
        }
        Ok(start_ram_address + size * 3)
    } else {
        Ok(start_ram_address)
    }
}
