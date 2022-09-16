use core::time::Duration;

use crate::error::EcError;
use crate::frame::{MailboxHeader, MAX_PDU_DATAGRAM};
use crate::hal::*;
use crate::memory::SiiData;
use crate::memory::SyncManagerControl;
use crate::memory::{AlStatusCode, FmmuRegister};
use crate::memory::{RxErrorCounter, SyncManagerActivation};
use crate::network::NetworkDescription;
use crate::network::PdoMapping;
use crate::network::Slave;
use crate::network::{AlState, FmmuConfig};
use crate::task::socket::{CommandSocket, SocketHandle, SocketOption, SocketsInterface};
use crate::task::{tasks::*, *};

#[derive(Debug)]
pub struct EtherCatMaster<'packet, 'socket_buf, 'slaves, 'pdo_mapping, 'pdo_entry, D>
where
    D: for<'d> RawEthernetDevice<'d>,
{
    sif: SocketsInterface<'packet, 'socket_buf, D, 5>,
    network: NetworkDescription<'slaves, 'pdo_mapping, 'pdo_entry>,
    gp_socket_handle: SocketHandle,
    process_data_image_size: usize,
    expected_lrw_wkc: u16,
    //process data
    process_data_handle: Option<SocketHandle>,
    process_data_task: Option<LogicalProcessData>,
    //dc drift
    dc_handle: SocketHandle,
    dc_task: Option<DcDriftCompensator>,
    //alstate
    al_state_handle: SocketHandle,
    al_state_task: AlStateReader,
    //rxerror
    rx_error_handle: SocketHandle,
    rx_error_task: RxErrorChecker,
}

impl<'packet, 'socket_buf, 'slaves, 'pdo_mapping, 'pdo_entry, D>
    EtherCatMaster<'packet, 'socket_buf, 'slaves, 'pdo_mapping, 'pdo_entry, D>
where
    D: for<'d> RawEthernetDevice<'d>,
{
    pub fn new(
        slave_buf: &'slaves mut [Option<Slave<'pdo_mapping, 'pdo_entry>>],
        pdu_buffer: &'socket_buf mut [u8],
        iface: CommandInterface<'packet, D>,
    ) -> Self {
        assert!(!slave_buf.is_empty());

        const MINIMUM_REQUIRED_BUFFER_SIZE: usize = AlStateReader::required_buffer_size()
            + RxErrorChecker::required_buffer_size()
            + DcDriftCompensator::required_buffer_size()
            + MAX_SM_SIZE as usize;
        assert!(MINIMUM_REQUIRED_BUFFER_SIZE < pdu_buffer.len());
        let (pdu_buffer1, rest) = pdu_buffer.split_at_mut(AlStateReader::required_buffer_size());
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
            process_data_image_size: 0,
            expected_lrw_wkc: 0,
            process_data_handle: None,
            process_data_task: None,
            dc_handle,
            dc_task: None,
            al_state_task: AlStateReader::new(),
            al_state_handle,
            rx_error_task: RxErrorChecker::new(),
            rx_error_handle,
        }
    }

    pub fn network<'a>(&'a self) -> &'a NetworkDescription<'slaves, 'pdo_mapping, 'pdo_entry> {
        &self.network
    }

    pub fn setup_pdo_and_change_to_safe_operational_state(
        &mut self,
        proces_data_buf: &'socket_buf mut [u8],
    ) {
        self.initilize_slaves().unwrap();
        self.synchronize_dc().unwrap();
        let num_slaves = self.network.len() as u16;
        self.change_al_state(TargetSlave::All(num_slaves), AlState::PreOperational)
            .unwrap();
        self.configure_pdo_image().unwrap();
        assert!(self.register_process_data_buffer(proces_data_buf));
        assert!(self.prepare_process_data_task());
        self.change_al_state(TargetSlave::All(num_slaves), AlState::SafeOperational)
            .unwrap();
    }

    /// プロセスデータ用のバッファを割り当てる。
    /// バッファがPDOイメージサイズより小さいとfalseを返す。
    pub fn register_process_data_buffer(&mut self, buf: &'socket_buf mut [u8]) -> bool {
        if self.process_data_image_size() < buf.len() {
            return false;
        }
        let process_data_handle = self.sif.add_socket(CommandSocket::new(buf)).unwrap();
        self.process_data_handle = Some(process_data_handle);
        true
    }

    pub fn prepare_process_data_task(&mut self) -> bool {
        if self.expected_lrw_wkc != 0 {
            self.process_data_task = Some(LogicalProcessData::new(
                0,
                self.expected_lrw_wkc,
                self.process_data_image_size(),
            ));
            true
        } else {
            self.process_data_task = None;
            false
        }
    }

    /// イメージのサイズを返す。
    pub fn process_data_image_size(&self) -> usize {
        self.process_data_image_size
    }

    /// 一サイクル分の処理を行う。
    pub fn process_one_cycle(
        &mut self,
        sys_time: EtherCatSystemTime,
    ) -> Result<bool, CommandInterfaceError> {
        let is_tx_rx_ok = self.sif.poll_tx_rx()?;
        if !is_tx_rx_ok {
            return Ok(false);
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

        let num_slaves = self.network.len() as u16;

        // process data + mb polling
        if let (Some(ref handle), Some(ref mut task)) = (process_data_handle, process_data_task) {
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

        Ok(true)
    }

    pub fn rx_error_count(&self) -> &RxErrorCounter<[u8; RxErrorCounter::SIZE]> {
        self.rx_error_task.rx_error_count()
    }

    pub fn last_al_state(&self) -> (Option<AlState>, Option<AlStatusCode>) {
        self.al_state_task.last_al_state()
    }

    pub fn process_data_invalid_wkc_count(&self) -> usize {
        if let Some(ref task) = self.process_data_task {
            task.invalid_wkc_count
        } else {
            0
        }
    }

    pub fn lost_frame_count(&self) -> usize {
        self.sif.lost_frame_count
    }

    pub fn initilize_slaves(&mut self) -> Result<(), EcError<NetworkInitializerError>> {
        let Self { network, sif, .. } = self;
        sif.initilize_slaves(&self.gp_socket_handle, network)
    }

    pub fn synchronize_dc(&mut self) -> Result<(), EcError<()>> {
        let Self { network, sif, .. } = self;
        sif.synchronize_dc(&self.gp_socket_handle, network)?;

        let mut firt_dc_slave = None;
        let mut dc_count = 0;
        for (i, slave) in network.slaves().enumerate() {
            if slave.info().support_dc {
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
    ) -> Result<(AlState, Option<AlStatusCode>), EcError<()>> {
        self.sif.read_al_state(&self.gp_socket_handle, target_slave)
    }

    pub fn change_al_state(
        &mut self,
        target_slave: TargetSlave,
        al_state: AlState,
    ) -> Result<AlState, EcError<AlStateTransferError>> {
        self.sif
            .change_al_state(&self.gp_socket_handle, target_slave, al_state)
    }

    pub fn read_sii(
        &mut self,
        slave_address: SlaveAddress,
        sii_address: u16,
    ) -> Result<(SiiData<[u8; SiiData::SIZE]>, usize), EcError<SiiTaskError>> {
        self.sif
            .read_sii(&self.gp_socket_handle, slave_address, sii_address)
    }

    pub fn read_mailbox(
        &mut self,
        slave_address: SlaveAddress,
        wait_full: bool,
    ) -> Result<(MailboxHeader<&[u8]>, &[u8]), EcError<MailboxTaskError>> {
        let slave = self.network.slave(slave_address).expect("slave not found");
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
    ) -> Result<(), EcError<MailboxTaskError>> {
        let slave = self.network.slave(slave_address).expect("slave not found");
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
    ) -> Result<&[u8], EcError<SdoTaskError>> {
        let slave = self.network.slave(slave_address).expect("slave not found");
        self.sif
            .read_sdo(&self.gp_socket_handle, slave, index, sub_index)
    }

    pub fn write_sdo(
        &mut self,
        slave_address: SlaveAddress,
        index: u16,
        sub_index: u8,
        data: &[u8],
    ) -> Result<(), EcError<SdoTaskError>> {
        let slave = self.network.slave(slave_address).expect("slave not found");
        self.sif
            .write_sdo(&self.gp_socket_handle, slave, index, sub_index, data)
    }

    pub fn configure_pdo_image(&mut self) -> Result<(), EcError<SdoTaskError>> {
        self.set_pdo_mappings_to_sm()?;
        let (image_size, expected_wkc) = self.set_logical_address_to_fmmu_context();
        self.set_fmmu_from_fmmu_context()
            .map_err(|err| EcError::<SdoTaskError>::from(err))?;
        self.process_data_image_size = image_size;
        self.expected_lrw_wkc = expected_wkc;
        Ok(())
    }

    // シンクマネージャーにCoEのPDOマップを割り当てる。
    fn set_pdo_mappings_to_sm(&mut self) -> Result<(), EcError<SdoTaskError>> {
        let Self {
            network,
            sif,
            gp_socket_handle,
            ..
        } = self;
        let handle = &gp_socket_handle;
        for slave in network.slaves_mut() {
            set_pdo_mappings_to_sm_utility(slave, sif, handle, false)?;
            set_pdo_mappings_to_sm_utility(slave, sif, handle, true)?;
        }
        Ok(())
    }

    // return image size and expected wkc
    fn set_logical_address_to_fmmu_context(&mut self) -> (usize, u16) {
        let mut expected_wkc = 0;
        let mut start_address = 0;
        for slave in self.network.slaves_mut() {
            let mut has_tx_data = false;
            let mut has_rx_data = false;

            for fmmu_config in slave.fmmu.iter_mut() {
                if let Some(fmmu_config) = fmmu_config.as_mut() {
                    fmmu_config.logical_start_address = Some(start_address);
                    start_address += fmmu_config.byte_length() as u32;
                    if fmmu_config.is_output() {
                        has_tx_data = true;
                    } else {
                        has_rx_data = true;
                    }
                }
            }

            if has_tx_data {
                expected_wkc += 1;
            }
            if has_rx_data {
                expected_wkc += 2;
            }
        }
        assert!(
            start_address <= MAX_PDU_DATAGRAM as u32,
            "process data size must be less than or equal to 1468 for now"
        );
        (start_address as usize, expected_wkc)
    }

    ///各スレーブのFMMUに論理アドレス、物理アドレス、サイズを設定する。
    fn set_fmmu_from_fmmu_context(&mut self) -> Result<(), EcError<()>> {
        let Self {
            network,
            sif,
            gp_socket_handle,
            ..
        } = self;
        for slave in network.slaves() {
            for (i, fmmu) in slave.fmmu.iter().enumerate() {
                if fmmu.is_none() {
                    continue;
                }
                let fmmu = fmmu.as_ref().unwrap();
                if fmmu.logical_start_address.is_none() {
                    continue;
                }
                let mut fmmu_reg = FmmuRegister::new();
                fmmu_reg.set_logical_start_address(fmmu.logical_start_address.unwrap());
                fmmu_reg.set_length(fmmu.byte_length());
                fmmu_reg.set_logical_end_bit(0);
                fmmu_reg.set_physical_start_address(fmmu.physical_address);
                fmmu_reg.set_physical_start_bit(0);
                if fmmu.is_output() {
                    fmmu_reg.set_read_enable(false);
                    fmmu_reg.set_write_enable(true);
                } else {
                    fmmu_reg.set_read_enable(true);
                    fmmu_reg.set_write_enable(false);
                }
                fmmu_reg.set_enable(true);
                sif.write_register(
                    &gp_socket_handle,
                    slave.info().slave_address().into(),
                    FmmuRegister::ADDRESS + (i as u16) * FmmuRegister::SIZE as u16,
                    &fmmu_reg.0,
                )?;
            }
        }
        Ok(())
    }
}

fn set_pdo_mappings_to_sm_utility<
    'packet,
    'socket_buf,
    'pdo_mapping,
    'pdo_entry,
    D: for<'d> RawEthernetDevice<'d>,
    const S: usize,
>(
    slave: &mut Slave<'pdo_mapping, 'pdo_entry>,
    sif: &mut SocketsInterface<'packet, 'socket_buf, D, S>,
    handle: &SocketHandle,
    is_output: bool, //output = rx of slave
) -> Result<(), EcError<SdoTaskError>> {
    let (pdo_mappings, sm_number) = if is_output {
        (
            slave.rx_process_data_mappings(),
            slave.info().process_data_rx_sm_number(),
        )
    } else {
        (
            slave.tx_process_data_mappings(),
            slave.info().process_data_tx_sm_number(),
        )
    };

    let mut pdo_map_size = 0;
    if let (Some(sm_number), Some(pdo_mappings)) = (sm_number, pdo_mappings) {
        let sm_assign = 0x1c10 + sm_number as u16;
        let sm_address = SyncManagerControl::ADDRESS + 0x08 * sm_number as u16;

        // SMへのPDOマップ割り当てをクリア
        sif.write_sdo(handle, slave, sm_assign, 0, &[0])?;

        //let pdo_maps = slave.pdo_mappings().unwrap();
        let mut map_index = 0;
        for pdo_map in pdo_mappings.iter() {
            if pdo_map.entries.is_empty() {
                continue;
            }
            map_index += 1;
            let PdoMapping {
                is_fixed,
                index: pdo_index,
                entries,
            } = pdo_map;
            //SMへPDOマップを割り当て
            sif.write_sdo(
                handle,
                slave,
                sm_assign,
                map_index,
                &pdo_index.to_le_bytes(),
            )?;
            if *is_fixed {
                continue;
            }
            // PDOマップへのPDOエントリー割り当てをクリア
            sif.write_sdo(handle, slave, *pdo_index, 0, &[0])?;
            let mut entry_index = 0;
            for entry in entries.iter() {
                let mut data: u32 = 0;
                data |= (entry.index as u32) << 16;
                data |= (entry.sub_index as u32) << 8;
                data |= entry.bit_length as u32;
                entry_index += 1;
                // PDOマップへPDOエントリーを割り当て
                sif.write_sdo(
                    handle,
                    slave,
                    //slave_info,
                    *pdo_index,
                    entry_index,
                    &data.to_le_bytes(),
                )?;
                let bit_diff = entry.bit_length() * 8 - entry.bit_length;
                //パディング
                if bit_diff != 0 {
                    entry_index += 1;
                    sif.write_sdo(
                        handle,
                        slave,
                        *pdo_index,
                        entry_index,
                        &(bit_diff as u32).to_le_bytes(),
                    )?;
                }
                pdo_map_size += entry.byte_length();
            }
            //PDOマップに何個のエントリーを割り当てたか？
            sif.write_sdo(handle, slave, *pdo_index, 0, &entry_index.to_le_bytes())?;
        }

        //SMに何個のPDOを割り当てたか？
        sif.write_sdo(handle, slave, sm_assign, 0, &map_index.to_be_bytes())?;

        //SMの設定
        let mut sm_control = SyncManagerControl::new();
        sm_control.set_physical_start_address(slave.info().pdo_start_address.unwrap());
        sm_control.set_length(pdo_map_size);
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
            sm_address,
            &sm_control.0,
        )
        .unwrap(); //unwrap for now
        let mut sm_active = SyncManagerActivation::new();
        sm_active.set_channel_enable(true);
        sm_active.set_repeat(false);
        sif.write_register(
            handle,
            slave.info().slave_address().into(),
            sm_address,
            &sm_active.0,
        )
        .unwrap(); //unwrap for now

        //FMMU設定
        let fmmu_config = FmmuConfig::new(sm_address, pdo_map_size * 8, is_output);
        slave.fmmu[1] = Some(fmmu_config);
    }
    Ok(())
}
