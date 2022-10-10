mod master_error;
pub use master_error::*;

use crate::{
    frame::{AbortCode, MailboxHeader, MAX_PDU_DATAGRAM},
    hal::RawEthernetDevice,
    interface::{
        CommandInterface, CommandSocket, PhyError, SlaveAddress, SocketHandle, SocketOption,
        SocketsInterface, TargetSlave,
    },
    network::{
        AlState, FmmuConfig, NetworkDescription, OperationMode, PdoEntry, PdoMapping, Slave,
        SlaveConfig,
    },
    register::{
        od::OdPdoEntry, AlStatusCode, CyclicOperationStartTime, DcActivation, FmmuRegister,
        RxErrorCounter, SiiData, Sync0CycleTime, Sync1CycleTime, SyncManagerActivation,
        SyncManagerControl,
    },
    task::{
        AlStateReader, AlStateTransferError, Cyclic, DcDriftCompensator, EtherCatSystemTime,
        LogicalProcessData, MailboxTaskError, NetworkInitializerError, RxErrorChecker,
        SdoTaskError, SiiTaskError, TaskError, TaskSpecificErrorKind, MAX_SM_SIZE,
    },
};

const START_LOGICAL_ADDRESS: u32 = 0x1000;

#[derive(Debug)]
pub struct EtherCatMaster<'frame, 'socket, 'slave, 'pdo_mapping, 'pdo_entry, D>
where
    D: for<'d> RawEthernetDevice<'d>,
{
    sif: SocketsInterface<'frame, 'socket, D, 5>,
    network: NetworkDescription<'slave, 'pdo_mapping, 'pdo_entry>,
    gp_socket_handle: SocketHandle,
    cycle_count: usize,
    //process data
    process_data_handle: Option<SocketHandle>,
    process_data_task: LogicalProcessData,
    //dc drift
    dc_handle: SocketHandle,
    dc_task: Option<DcDriftCompensator>,
    //alstate
    al_state_handle: SocketHandle,
    al_state_task: AlStateReader,
    //rx error
    rx_error_handle: SocketHandle,
    rx_error_task: RxErrorChecker,
}

impl<'frame, 'socket, 'slave, 'pdo_mapping, 'pdo_entry, D>
    EtherCatMaster<'frame, 'socket, 'slave, 'pdo_mapping, 'pdo_entry, D>
where
    D: for<'d> RawEthernetDevice<'d>,
{
    pub fn new(
        slave_buf: &'slave mut [(Option<Slave>, SlaveConfig<'pdo_mapping, 'pdo_entry>)],
        socket_buffer: &'socket mut [u8],
        iface: CommandInterface<'frame, D>,
    ) -> Self {
        assert!(!slave_buf.is_empty());

        const MINIMUM_REQUIRED_BUFFER_SIZE: usize = AlStateReader::required_buffer_size()
            + RxErrorChecker::required_buffer_size()
            + DcDriftCompensator::required_buffer_size()
            + MAX_SM_SIZE as usize;
        assert!(MINIMUM_REQUIRED_BUFFER_SIZE < socket_buffer.len());
        let (pdu_buffer1, rest) = socket_buffer.split_at_mut(AlStateReader::required_buffer_size());
        let (pdu_buffer2, rest) = rest.split_at_mut(RxErrorChecker::required_buffer_size());
        let (pdu_buffer3, rest) = rest.split_at_mut(DcDriftCompensator::required_buffer_size());
        let (pdu_buffer4, _) = rest.split_at_mut(256);

        let sockets = [
            SocketOption::default(), // al state
            SocketOption::default(), // rx error
            SocketOption::default(), // dc comp
            SocketOption::default(), // general purpose
            SocketOption::default(), // pdo
        ];
        let mut sif = SocketsInterface::new(iface, sockets);
        let al_state_handle = sif.add_socket(CommandSocket::new(pdu_buffer1)).unwrap();
        let rx_error_handle = sif.add_socket(CommandSocket::new(pdu_buffer2)).unwrap();
        let dc_handle = sif.add_socket(CommandSocket::new(pdu_buffer3)).unwrap();
        let gp_socket_handle = sif.add_socket(CommandSocket::new(pdu_buffer4)).unwrap();

        let network = NetworkDescription::new(slave_buf);
        Self {
            sif,
            network,
            gp_socket_handle,
            cycle_count: 0,
            process_data_handle: None,
            process_data_task: LogicalProcessData::new(START_LOGICAL_ADDRESS, 0, 0),
            dc_handle,
            dc_task: None,
            al_state_task: AlStateReader::new(),
            al_state_handle,
            rx_error_task: RxErrorChecker::new(),
            rx_error_handle,
        }
    }

    pub fn network<'a>(&'a self) -> &'a NetworkDescription<'slave, 'pdo_mapping, 'pdo_entry> {
        &self.network
    }

    /// Return process data imeze size
    pub fn process_data_image_size(&self) -> usize {
        self.process_data_task.image_size()
    }

    /// If the buffer size is smaller than the image size, return false.
    pub fn register_process_data_buffer(&mut self, buf: &'socket mut [u8]) -> bool {
        if buf.len() < self.process_data_image_size() {
            return false;
        }
        let process_data_handle = self.sif.add_socket(CommandSocket::new(buf)).unwrap();
        self.process_data_handle = Some(process_data_handle);
        true
    }

    /// This method must be repeated until the cycle count returned is increased.
    pub fn process_one_cycle(&mut self, sys_time: EtherCatSystemTime) -> Result<usize, PhyError> {
        let is_tx_rx_ok = self.sif.poll_tx_rx()?;
        if !is_tx_rx_ok {
            return Ok(self.cycle_count);
        }

        let Self {
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

        let num_slaves = self.network.num_slaves();

        // process data + mb polling
        if let (Some(ref handle), ref mut task) = (process_data_handle, process_data_task) {
            let socket = self.sif.get_socket_mut(handle).unwrap();
            let recv_data = socket.get_recieved_command();
            if let Some(recv_data) = recv_data {
                task.recieve_and_process(&recv_data, sys_time);
            }
            socket.set_command(|buf| task.next_command(buf));
            //TODO: mailbox polling
        }

        // dc drift comp
        if let (ref handle, Some(ref mut task)) = (dc_handle, dc_task) {
            let socket = self.sif.get_socket_mut(handle).unwrap();
            let recv_data = socket.get_recieved_command();
            if let Some(recv_data) = recv_data {
                task.recieve_and_process(&recv_data, sys_time);
            }
            socket.set_command(|buf| task.next_command(buf));
        }

        // rx error check
        {
            let socket = self.sif.get_socket_mut(rx_error_handle).unwrap();
            let recv_data = socket.get_recieved_command();
            if let Some(recv_data) = recv_data {
                rx_error_task.recieve_and_process(&recv_data, sys_time);
            }
            rx_error_task.set_target(TargetSlave::All(num_slaves));
            socket.set_command(|buf| rx_error_task.next_command(buf));
        }

        // al state + al status code
        {
            let socket = self.sif.get_socket_mut(al_state_handle).unwrap();
            let recv_data = socket.get_recieved_command();
            if let Some(recv_data) = recv_data {
                al_state_task.recieve_and_process(&recv_data, sys_time);
            }
            socket.set_command(|buf| al_state_task.next_command(buf));
        }

        self.cycle_count = self.cycle_count.overflowing_add(1).0;
        Ok(self.cycle_count)
    }

    pub fn rx_error_count(&self) -> &RxErrorCounter<[u8; RxErrorCounter::SIZE]> {
        self.rx_error_task.rx_error_count()
    }

    pub fn al_state(&self) -> (Option<AlState>, Option<AlStatusCode>) {
        self.al_state_task.last_al_state()
    }

    pub fn invalid_wkc_count_of_process_data(&self) -> usize {
        self.process_data_task.invalid_wkc_count
    }

    pub fn lost_frame_count(&self) -> usize {
        self.sif.lost_frame_count
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
            .tx_process_data_mappings()
            .map(|pdo_maps| pdo_maps.get(pdo_map_index))
            .flatten()
            .map(|pdo_map| pdo_map.entries.get(pdo_entry_index))
            .flatten()
            .map(|pdo_entry| pdo_entry.read(START_LOGICAL_ADDRESS, pdo_image.data_buf(), buf))
            .flatten()
    }

    pub fn read_pdo_bool(
        &self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
    ) -> Option<bool> {
        let mut buf = [0];
        self.read_pdo(slave_address, pdo_map_index, pdo_entry_index, &mut buf)?;
        Some(buf[0] & 1 == 1)
    }

    pub fn read_pdo_u8(
        &self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
    ) -> Option<u8> {
        let mut buf = [0];
        self.read_pdo(slave_address, pdo_map_index, pdo_entry_index, &mut buf)?;
        Some(u8::from_le_bytes(buf))
    }

    pub fn read_pdo_i8(
        &self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
    ) -> Option<i8> {
        let mut buf = [0];
        self.read_pdo(slave_address, pdo_map_index, pdo_entry_index, &mut buf)?;
        Some(i8::from_le_bytes(buf))
    }

    pub fn read_pdo_u16(
        &self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
    ) -> Option<u16> {
        let mut buf = [0; 2];
        self.read_pdo(slave_address, pdo_map_index, pdo_entry_index, &mut buf)?;
        Some(u16::from_le_bytes(buf))
    }

    pub fn read_pdo_i16(
        &self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
    ) -> Option<i16> {
        let mut buf = [0; 2];
        self.read_pdo(slave_address, pdo_map_index, pdo_entry_index, &mut buf)?;
        Some(i16::from_le_bytes(buf))
    }

    pub fn read_pdo_u32(
        &self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
    ) -> Option<u32> {
        let mut buf = [0; 4];
        self.read_pdo(slave_address, pdo_map_index, pdo_entry_index, &mut buf)?;
        Some(u32::from_le_bytes(buf))
    }

    pub fn read_pdo_i32(
        &self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
    ) -> Option<i32> {
        let mut buf = [0; 4];
        self.read_pdo(slave_address, pdo_map_index, pdo_entry_index, &mut buf)?;
        Some(i32::from_le_bytes(buf))
    }

    pub fn read_pdo_u64(
        &self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
    ) -> Option<u64> {
        let mut buf = [0; 8];
        self.read_pdo(slave_address, pdo_map_index, pdo_entry_index, &mut buf)?;
        Some(u64::from_le_bytes(buf))
    }

    pub fn read_pdo_i64(
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
            .rx_process_data_mappings()
            .map(|pdo_maps| pdo_maps.get(pdo_map_index))
            .flatten()
            .map(|pdo_map| pdo_map.entries.get(pdo_entry_index))
            .flatten()
            .map(|pdo_entry| pdo_entry.write(START_LOGICAL_ADDRESS, pdo_image.data_buf_mut(), data))
            .flatten()
    }

    pub fn write_pdo_bool(
        &mut self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
        data: bool,
    ) -> Option<()> {
        let buf = [data as u8];
        self.write_pdo(slave_address, pdo_map_index, pdo_entry_index, &buf)
    }

    pub fn write_pdo_u8(
        &mut self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
        data: u8,
    ) -> Option<()> {
        let buf = data.to_le_bytes();
        self.write_pdo(slave_address, pdo_map_index, pdo_entry_index, &buf)
    }

    pub fn write_pdo_i8(
        &mut self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
        data: i8,
    ) -> Option<()> {
        let buf = data.to_le_bytes();
        self.write_pdo(slave_address, pdo_map_index, pdo_entry_index, &buf)
    }

    pub fn write_pdo_u16(
        &mut self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
        data: u16,
    ) -> Option<()> {
        let buf = data.to_le_bytes();
        self.write_pdo(slave_address, pdo_map_index, pdo_entry_index, &buf)
    }

    pub fn write_pdo_i16(
        &mut self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
        data: i16,
    ) -> Option<()> {
        let buf = data.to_le_bytes();
        self.write_pdo(slave_address, pdo_map_index, pdo_entry_index, &buf)
    }

    pub fn write_pdo_u32(
        &mut self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
        data: u32,
    ) -> Option<()> {
        let buf = data.to_le_bytes();
        self.write_pdo(slave_address, pdo_map_index, pdo_entry_index, &buf)
    }

    pub fn write_pdo_i32(
        &mut self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
        data: i32,
    ) -> Option<()> {
        let buf = data.to_le_bytes();
        self.write_pdo(slave_address, pdo_map_index, pdo_entry_index, &buf)
    }

    pub fn write_pdo_u64(
        &mut self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
        data: u64,
    ) -> Option<()> {
        let buf = data.to_le_bytes();
        self.write_pdo(slave_address, pdo_map_index, pdo_entry_index, &buf)
    }

    pub fn write_pdo_i64(
        &mut self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
        data: i64,
    ) -> Option<()> {
        let buf = data.to_le_bytes();
        self.write_pdo(slave_address, pdo_map_index, pdo_entry_index, &buf)
    }

    pub fn initilize_slaves(&mut self) -> Result<(), TaskError<NetworkInitializerError>> {
        let Self { network, sif, .. } = self;
        sif.initilize_slaves(&self.gp_socket_handle, network)
    }

    pub fn synchronize_dc(&mut self) -> Result<(), TaskError<()>> {
        let Self { network, sif, .. } = self;
        sif.synchronize_dc(&self.gp_socket_handle, network)?;

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
            self.dc_task = Some(DcDriftCompensator::new(firt_dc_slave as u16, dc_count));
        }
        Ok(())
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
    ) -> Result<AlState, TaskError<AlStateTransferError>> {
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
    ) -> Result<(MailboxHeader<&[u8]>, &[u8]), TaskError<MailboxTaskError>> {
        let (slave, _) = self.network.slave(slave_address).expect("slave not found");
        let slave_info = slave.info();
        self.sif
            .read_mailbox(&self.gp_socket_handle, slave_info, wait_full)
    }

    pub fn write_mailbox(
        &mut self,
        slave_address: SlaveAddress,
        mb_header: &[u8; MailboxHeader::SIZE],
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

    /// Easy setup API.
    /// If the buffer size is smaller than the image size, a panic will occur.
    pub fn configure_slave_settings(
        &mut self,
        proces_data_buf: &'socket mut [u8],
    ) -> Result<(), TaskError<TaskSpecificErrorKind>> {
        //let num_slaves = self.network.num_slaves();
        //self.change_al_state(TargetSlave::All(num_slaves), AlState::PreOperational)?;
        self.configure_pdo_image()?;
        assert!(self.register_process_data_buffer(proces_data_buf));
        self.configure_sync_mode()?;
        Ok(())
    }

    /// Easy API for configuration of PDO mappings
    fn configure_pdo_image(&mut self) -> Result<(), TaskError<SdoTaskError>> {
        self.set_pdo_config_to_od()?;
        self.set_pdo_to_sm()?;
        let (image_size, expected_wkc) = self.configure_fmmu()?;
        self.process_data_task.set_image_size(image_size);
        self.process_data_task.set_expected_wkc(expected_wkc);
        self.set_logical_address_to_pdo_entry_config();
        Ok(())
    }

    /// Set the PDO map config to object dictionary.
    fn set_pdo_config_to_od(&mut self) -> Result<(), TaskError<SdoTaskError>> {
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
    fn set_pdo_to_sm(&mut self) -> Result<(), TaskError<SdoTaskError>> {
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
                    set_pdo_to_sm_utility(slave, sif, handle, true, ram_address)?;
                set_pdo_to_sm_utility(slave, sif, handle, false, next_ram_address)?;
            }
        }
        Ok(())
    }

    /// Return image size and expected wkc.
    fn set_logical_address_to_fmmu_config(&mut self) -> (usize, u16) {
        let mut expected_wkc = 0;
        let mut bit_address = (START_LOGICAL_ADDRESS * 8) as u64;
        for (slave, _) in self.network.slaves_mut() {
            let mut has_tx_data = false;
            let mut has_rx_data = false;

            for fmmu_config in slave
                .fmmu_config_mut()
                .iter_mut()
                .filter_map(|f| f.as_mut())
                .filter(|f| f.bit_length != 0)
            {
                fmmu_config.logical_start_address = Some((bit_address >> 3) as u32);
                fmmu_config.start_bit = (bit_address % 8) as u8;
                bit_address += fmmu_config.bit_length as u64;

                if fmmu_config.is_output() {
                    has_rx_data = true;
                } else {
                    has_tx_data = true;
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
            (bit_address >> 3) - START_LOGICAL_ADDRESS as u64
        } else {
            (bit_address >> 3) + 1 - START_LOGICAL_ADDRESS as u64
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
                        .logical_start_address
                        .is_none()
                {
                    continue;
                }
                let fmmu_config = slave.fmmu_config()[i].as_ref().unwrap();
                let start_address = fmmu_config.logical_start_address.unwrap();
                let start_bit = fmmu_config.start_bit as u64;
                let mut total_bits = (start_address * 8) as u64 + start_bit;
                let pdo_maps = if i == 0 {
                    config.rx_process_data_mappings_mut()
                } else {
                    config.tx_process_data_mappings_mut()
                };
                if pdo_maps.is_none() {
                    continue;
                }
                for pdo_map in pdo_maps.unwrap() {
                    for pdo_entry in pdo_map.entries.iter_mut() {
                        let mod8 = total_bits % 8;
                        let (addr, s_bit) = if mod8 == 0 {
                            (total_bits >> 3, 0)
                        } else {
                            ((total_bits >> 3) + 1, mod8)
                        };
                        pdo_entry.start_bit = s_bit as u8;
                        pdo_entry.logical_start_address = Some(addr as u32);
                        total_bits += pdo_entry.bit_length() as u64;
                    }
                }
                assert_eq!(
                    fmmu_config.bit_length as u64,
                    total_bits - ((start_address * 8) as u64 + start_bit)
                );
            }
        }
    }

    /// Set the logical address, physical address, and size for each slave FMMU.
    /// Return image size and expected wkc.
    fn configure_fmmu(&mut self) -> Result<(usize, u16), TaskError<()>> {
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
                if fmmu.logical_start_address.is_none() || fmmu.byte_length() == 0 {
                    continue;
                }
                let mut fmmu_reg = FmmuRegister::new();
                fmmu_reg.set_logical_start_address(fmmu.logical_start_address.unwrap());
                let (byte_length, end_bit) = fmmu.byte_length_and_end_bit();
                fmmu_reg.set_length(byte_length);
                fmmu_reg.set_logical_end_bit(end_bit);
                fmmu_reg.set_physical_start_address(fmmu.physical_address);
                fmmu_reg.set_physical_start_bit(0);
                if fmmu.is_output() {
                    fmmu_reg.set_read_enable(false);
                    fmmu_reg.set_write_enable(true);
                    dbg!(&fmmu_reg.read_enable());
                    dbg!(&fmmu_reg.write_enable());
                } else {
                    fmmu_reg.set_read_enable(true);
                    fmmu_reg.set_write_enable(false);
                    dbg!(&fmmu_reg.read_enable());
                    dbg!(&fmmu_reg.write_enable());
                }
                fmmu_reg.set_enable(true);
                sif.write_register(
                    gp_socket_handle,
                    slave.info().slave_address().into(),
                    FmmuRegister::ADDRESS + (i as u16) * FmmuRegister::SIZE as u16,
                    &fmmu_reg.0,
                )?;
                dbg!(slave.info().slave_address());
                dbg!(FmmuRegister::ADDRESS + (i as u16) * FmmuRegister::SIZE as u16);
                dbg!(&fmmu_reg.logical_start_address());
                dbg!(&fmmu_reg.length());
                dbg!(&fmmu_reg.logical_start_bit());
                dbg!(&fmmu_reg.logical_end_bit());
                dbg!(&fmmu_reg.physical_start_address());
                dbg!(&fmmu_reg.physical_start_bit());
                dbg!(&fmmu_reg.read_enable());
                dbg!(&fmmu_reg.write_enable());
                dbg!(&fmmu_reg.enable());
            }
        }
        Ok((image_size, expected_wkc))
    }

    pub fn configure_sync_mode(&mut self) -> Result<(), TaskError<SdoTaskError>> {
        let Self {
            sif,
            network,
            gp_socket_handle,
            ..
        } = self;
        for (slave, config) in network.slaves().filter(|(s, _)| s.info().support_coe()) {
            // Set Operation Mode
            let has_rx_sm = match sif.read_sdo(
                &gp_socket_handle,
                slave,
                0x1C00,
                slave.info().process_data_rx_sm_number().unwrap() + 1,
            )?[0]
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
                    &[config.operation_mode as u8, 0],
                )?;
                // Cycle time
                sif.write_sdo(
                    &gp_socket_handle,
                    slave,
                    addr,
                    2,
                    &config.cycle_time_ns.to_le_bytes(),
                )?;
            }

            let has_tx_sm = match sif.read_sdo(
                &gp_socket_handle,
                slave,
                0x1C00,
                slave.info().process_data_tx_sm_number().unwrap() + 1,
            )?[0]
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
                    &[config.operation_mode as u8, 0],
                )?;
                // Cycle time
                sif.write_sdo(
                    &gp_socket_handle,
                    slave,
                    addr,
                    2,
                    &config.cycle_time_ns.to_le_bytes(),
                )?;
            }

            // Set interval of sync signal
            sif.write_register(
                &gp_socket_handle,
                slave.info().slave_address().into(),
                Sync0CycleTime::ADDRESS,
                &config.cycle_time_ns.to_le_bytes(),
            )?;
            sif.write_register(
                &gp_socket_handle,
                slave.info().slave_address().into(),
                Sync1CycleTime::ADDRESS,
                &(config.cycle_time_ns >> 1).to_le_bytes(),
            )?;
            // Start SYNC Signal
            sif.write_register(
                &gp_socket_handle,
                slave.info().slave_address().into(),
                CyclicOperationStartTime::ADDRESS,
                &0_u64.to_le_bytes(), //TODO
            )?;

            // cycle permission
            let mut dc_actiation = DcActivation::new();
            if let OperationMode::Sync0Event | OperationMode::Sync1Event = config.operation_mode {
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
            )?;
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
    const S: usize,
>(
    slave: &mut Slave,
    slave_config: &mut SlaveConfig<'pdo_mapping, 'pdo_entry>,
    sif: &mut SocketsInterface<'frame, 'socket, D, S>,
    handle: &SocketHandle,
    is_output: bool,
) -> Result<(), TaskError<SdoTaskError>> {
    let (pdo_mappings, sm_number) = if is_output {
        (
            slave_config.rx_process_data_mappings(),
            slave.info().process_data_rx_sm_number(),
        )
    } else {
        (
            slave_config.tx_process_data_mappings(),
            slave.info().process_data_tx_sm_number(),
        )
    };
    if let (Some(sm_number), Some(pdo_mappings)) = (sm_number, pdo_mappings) {
        let sm_assign = 0x1C10 + sm_number as u16;

        // Clear PDO mappings
        sif.write_sdo(handle, slave, sm_assign, 0, &[0])?;

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
            )?;
            if *is_fixed {
                continue;
            }

            // Clear PDO entry of PDO map
            sif.write_sdo(handle, slave, *pdo_map_index, 0, &[0])?;
            let mut entry_index = 0;
            for entry in entries.iter() {
                // let mut data: u32 = 0;
                // data |= (entry.index as u32) << 16;
                // data |= (entry.sub_index as u32) << 8;
                // data |= entry.bit_length as u32;
                let mut od_pdo_entry = OdPdoEntry::new();
                od_pdo_entry.set_index(entry.index);
                od_pdo_entry.set_sub_index(entry.sub_index);
                od_pdo_entry.set_bit_length(entry.bit_length);
                entry_index += 1;
                // Assign PDO entry to PDO map
                sif.write_sdo(handle, slave, *pdo_map_index, entry_index, &od_pdo_entry.0)?;
            }
            // How many entries were assigned to the PDO map?
            sif.write_sdo(handle, slave, *pdo_map_index, 0, &entry_index.to_le_bytes())?;
        }
        // How many PDO maps were assigned to the SM?
        sif.write_sdo(handle, slave, sm_assign, 0, &map_index.to_le_bytes())?;
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
    const S: usize,
>(
    slave: &mut Slave,
    //slave_config: &mut SlaveConfig<'pdo_mapping, 'pdo_entry>,
    sif: &mut SocketsInterface<'frame, 'socket, D, S>,
    handle: &SocketHandle,
    is_output: bool,
    start_ram_address: u16,
) -> Result<u16, TaskError<SdoTaskError>> {
    let num_sm_comm = sif.read_sdo(handle, slave, 0x1C00, 0)?;
    assert!(4 <= num_sm_comm[0]);

    let sm_number = if is_output {
        slave.info().process_data_rx_sm_number()
    } else {
        slave.info().process_data_tx_sm_number()
    };

    if let Some(sm_number) = sm_number {
        // Read SM type
        let is_pdo_map_none = if is_output {
            let sm_type = sif.read_sdo(handle, slave, 0x1C00, sm_number + 1)?[0];
            match sm_type {
                0 => true,
                3 => false,
                _ => panic!("unsupported sm type"),
            }
        } else {
            let sm_type = sif.read_sdo(handle, slave, 0x1C00, sm_number + 1)?[0];
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
            let num_maps = sif.read_sdo(handle, slave, sm_assign, 0)?[0] as usize;
            for index in 1..(num_maps + 1) {
                let map_address = sif.read_sdo(handle, slave, sm_assign, index as u8)?;
                let map_address = u16::from_le_bytes([map_address[0], map_address[1]]);
                let num_entry = sif.read_sdo(handle, slave, map_address, 0)?[0] as usize;
                for entry_index in 1..(num_entry + 1) {
                    let entry = sif.read_sdo(handle, slave, map_address, entry_index as u8)?;
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
        if is_output {
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
        let fmmu_config = FmmuConfig::new(start_ram_address, size * 8, is_output);
        if is_output {
            slave.fmmu_config_mut()[0] = Some(fmmu_config);
        } else {
            slave.fmmu_config_mut()[1] = Some(fmmu_config);
        }
        Ok(start_ram_address + size * 3)
    } else {
        Ok(start_ram_address)
    }
}
