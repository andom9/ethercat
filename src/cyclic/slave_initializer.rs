use super::EtherCATSystemTime;
use super::{al_state_transfer::*, sii_reader::*};
use crate::cyclic::Cyclic;
use crate::error::*;
use crate::interface::*;
use crate::network::*;
use crate::register::{application::*, datalink::*};
use crate::slave::*;
use crate::util::*;
use bit_field::BitField;
use embedded_hal::timer::*;
use fugit::*;

#[derive(Debug, Clone)]
pub enum InitError {
    Common(CommonError),
    AlStateTransition(AlStateTransitionError),
    SII(SIIError),
    FailedToLoadEEPROM,
}

impl From<CommonError> for InitError {
    fn from(err: CommonError) -> Self {
        Self::Common(err)
    }
}

impl From<AlStateTransitionError> for InitError {
    fn from(err: AlStateTransitionError) -> Self {
        Self::AlStateTransition(err)
    }
}

impl From<SIIError> for InitError {
    fn from(err: SIIError) -> Self {
        Self::SII(err)
    }
}

#[derive(Debug)]
enum InitilizerState {
    Idle,
    Error(InitError),
    SetLoopPort,
    RequestInitState,
    WaitInitState,
    ResetErrorCount,
    SetWatchDogDivider,
    DisableDLWatchDog,
    DisableSMWatchDog,
    CheckDLStatus,
    CheckDLInfo,
    ClearFMMU0,
    ClearFMMU1,
    ClearSM0,
    ClearSM1,
    ClearSM2,
    ClearSM3,
    GetVenderID,
    WaitVenderID,
    GetProductCode,
    WaitProductCode,
    GetRevision,
    WaitRevision,
    GetProtocol,
    WaitProtocol,
    GetRxMailboxSize,
    WaitRxMailboxSize,
    GetRxMailboxOffset,
    WaitRxMailboxOffset,
    GetTxMailboxSize,
    WaitTxMailboxSize,
    GetTxMailboxOffset,
    WaitTxMailboxOffset,
    SetSM0,
    SetSM1,
    SetStationAddress,
    ClearDCActivation,
    ClearCyclicOperationStartTime,
    ClearSync0CycleTime,
    ClearSync1CycleTime,
    ClearLatchEdge,
    ClearLatchEvent,
    ClearLatch0PositiveEdgeValue,
    ClearLatch0NegativeEdgeValue,
    ClearLatch1PositiveEdgeValue,
    ClearLatch1NegativeEdgeValue,
    Complete,
}

#[derive(Debug)]
pub struct SlaveInitilizer {
    inner: InnerFunction,
    slave_address: SlaveAddress,
    state: InitilizerState,
    command: Command,
    buffer: [u8; buffer_size()],
    slave_info: Option<Slave>,
}

impl SlaveInitilizer {
    pub fn new() -> Self {
        Self {
            inner: InnerFunction::This,
            slave_address: SlaveAddress::SlaveNumber(0),
            state: InitilizerState::Idle,
            command: Command::default(),
            buffer: [0; buffer_size()],
            slave_info: None,
        }
    }

    pub fn start(&mut self, slave_position: u16) {
        self.slave_address = SlaveAddress::SlaveNumber(slave_position);
        self.state = InitilizerState::SetLoopPort;
        self.slave_info = Some(Slave::default());
    }

    pub fn wait(&mut self) -> nb::Result<Option<Slave>, InitError> {
        match &self.state {
            InitilizerState::Complete => Ok(core::mem::take(&mut self.slave_info)),
            InitilizerState::Error(err) => Err(nb::Error::Other(err.clone())),
            _ => Err(nb::Error::WouldBlock),
        }
    }

    //pub fn start(&mut self, slave_position: u16) -> bool {
    //    match self.state {
    //        InitilizerState::Idle | InitilizerState::Complete | InitilizerState::Error(_) => {
    //            self.reset();
    //            self.slave_address = SlaveAddress::SlaveNumber(slave_position);
    //            self.state = InitilizerState::SetLoopPort;
    //            true
    //        }
    //        _ => false,
    //    }
    //}

    //pub fn reset(&mut self) {
    //    self.inner.into_owned();
    //    self.state = InitilizerState::SetLoopPort;
    //    self.command = Command::default();
    //    self.buffer.fill(0);
    //    self.slave_info = SlaveInfo::default();
    //}

    //pub fn error(&self) -> Option<InitError> {
    //    if let InitilizerState::Error(err) = &self.state {
    //        Some(err.clone())
    //    } else {
    //        None
    //    }
    //}

    //pub fn wait_slave_info(&self) -> Result<Option<&SlaveInfo>, InitError> {
    //    if let InitilizerState::Error(err) = &self.state {
    //        Err(err.clone())
    //    } else {
    //        if let InitilizerState::Complete = self.state {
    //            Ok(Some(&self.slave_info))
    //        } else {
    //            Ok(None)
    //        }
    //    }
    //}

    //pub(crate) fn take_timer(mut self) -> &'a mut T {
    //    self.inner.into_owned();
    //    if let InnerFunction::Owned(timer) = self.inner {
    //        timer
    //    } else {
    //        unreachable!()
    //    }
    //}
}

impl Cyclic for SlaveInitilizer {
    fn next_command(
        &mut self,
        desc: &mut NetworkDescription,
        sys_time: EtherCATSystemTime,
    ) -> Option<(Command, &[u8])> {
        let command_and_data = match self.state {
            InitilizerState::Idle => None,
            InitilizerState::Error(_) => None,
            InitilizerState::Complete => None,
            InitilizerState::SetLoopPort => {
                let command = Command::new_write(self.slave_address, DLControl::ADDRESS);
                self.buffer.fill(0);
                // ループポートを設定する。
                // ・EtherCAT以外のフレームを削除する。
                // ・ソースMACアドレスを変更して送信する。
                // ・ポートを自動開閉する。
                let mut dl_control = DLControl(self.buffer);
                dl_control.set_forwarding_rule(true);
                dl_control.set_tx_buffer_size(7);
                dl_control.set_enable_alias_address(false);
                Some((command, &self.buffer[..DLControl::SIZE]))
            }
            InitilizerState::RequestInitState => {
                self.inner.into_al_state_transfer();
                let al_transfer = self.inner.al_state_transfer().unwrap();
                al_transfer.start(self.slave_address, Some(AlState::Init));
                al_transfer.next_command(desc, sys_time)
            }
            InitilizerState::WaitInitState => {
                let al_transfer = self.inner.al_state_transfer().unwrap();
                al_transfer.next_command(desc, sys_time)
            }
            InitilizerState::ResetErrorCount => {
                let command = Command::new_write(self.slave_address, RxErrorCounter::ADDRESS);
                self.buffer.fill(0);
                Some((command, &self.buffer[..RxErrorCounter::SIZE]))
            }
            InitilizerState::SetWatchDogDivider => {
                let command = Command::new_write(self.slave_address, WatchDogDivider::ADDRESS);
                self.buffer.fill(0);
                let mut watchdog_div = WatchDogDivider(self.buffer);
                watchdog_div.set_watch_dog_divider(2498); //100us(default)
                Some((command, &self.buffer[..WatchDogDivider::SIZE]))
            }
            InitilizerState::DisableDLWatchDog => {
                let command = Command::new_write(self.slave_address, DLUserWatchDog::ADDRESS);
                self.buffer.fill(0);
                // disable dl watch dog
                Some((command, &self.buffer[..DLUserWatchDog::SIZE]))
            }
            InitilizerState::DisableSMWatchDog => {
                let command =
                    Command::new_write(self.slave_address, SyncManagerChannelWatchDog::ADDRESS);
                self.buffer.fill(0);
                // disable sm watch dog
                Some((command, &self.buffer[..SyncManagerChannelWatchDog::SIZE]))
            }
            InitilizerState::CheckDLStatus => {
                // ポートと、EEPROMのロード状況を確認する。
                let command = Command::new_read(self.slave_address, DLStatus::ADDRESS);
                self.buffer.fill(0);
                Some((command, &self.buffer[..DLStatus::SIZE]))
            }
            InitilizerState::CheckDLInfo => {
                // 各種サポート状況の確認
                let command = Command::new_read(self.slave_address, DLInformation::ADDRESS);
                self.buffer.fill(0);
                Some((command, &self.buffer[..DLInformation::SIZE]))
            }
            InitilizerState::ClearFMMU0 => {
                let command = Command::new_write(self.slave_address, FMMURegister::ADDRESS0);
                self.buffer.fill(0);
                // disable dl watch dog
                Some((command, &self.buffer[..FMMURegister::SIZE]))
            }
            InitilizerState::ClearFMMU1 => {
                let command = Command::new_write(self.slave_address, FMMURegister::ADDRESS1);
                self.buffer.fill(0);
                // disable dl watch dog
                Some((command, &self.buffer[..FMMURegister::SIZE]))
            }
            InitilizerState::ClearSM0 => {
                let command = Command::new_write(self.slave_address, SyncManagerRegister::ADDRESS0);
                self.buffer.fill(0);
                // disable dl watch dog
                Some((command, &self.buffer[..SyncManagerRegister::SIZE]))
            }
            InitilizerState::ClearSM1 => {
                let command = Command::new_write(self.slave_address, SyncManagerRegister::ADDRESS1);
                self.buffer.fill(0);
                // disable dl watch dog
                Some((command, &self.buffer[..SyncManagerRegister::SIZE]))
            }
            InitilizerState::ClearSM2 => {
                let command = Command::new_write(self.slave_address, SyncManagerRegister::ADDRESS2);
                self.buffer.fill(0);
                // disable dl watch dog
                Some((command, &self.buffer[..SyncManagerRegister::SIZE]))
            }
            InitilizerState::ClearSM3 => {
                let command = Command::new_write(self.slave_address, SyncManagerRegister::ADDRESS3);
                self.buffer.fill(0);
                // disable dl watch dog
                Some((command, &self.buffer[..SyncManagerRegister::SIZE]))
            }
            InitilizerState::GetVenderID => {
                self.inner.into_sii();
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.start(self.slave_address, sii_reg::VenderID::ADDRESS);
                sii_reader.next_command(desc, sys_time)
            }
            InitilizerState::WaitVenderID => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.next_command(desc, sys_time)
            }
            InitilizerState::GetProductCode => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.start(self.slave_address, sii_reg::ProductCode::ADDRESS);
                sii_reader.next_command(desc, sys_time)
            }
            InitilizerState::WaitProductCode => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.next_command(desc, sys_time)
            }
            InitilizerState::GetRevision => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.start(self.slave_address, sii_reg::RevisionNumber::ADDRESS);
                sii_reader.next_command(desc, sys_time)
            }
            InitilizerState::WaitRevision => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.next_command(desc, sys_time)
            }
            InitilizerState::GetProtocol => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.start(self.slave_address, sii_reg::MailboxProtocol::ADDRESS);
                sii_reader.next_command(desc, sys_time)
            }
            InitilizerState::WaitProtocol => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.next_command(desc, sys_time)
            }
            InitilizerState::GetRxMailboxSize => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.start(self.slave_address, sii_reg::StandardRxMailboxSize::ADDRESS);
                sii_reader.next_command(desc, sys_time)
            }
            InitilizerState::WaitRxMailboxSize => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.next_command(desc, sys_time)
            }
            InitilizerState::GetRxMailboxOffset => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.start(
                    self.slave_address,
                    sii_reg::StandardRxMailboxOffset::ADDRESS,
                );
                sii_reader.next_command(desc, sys_time)
            }
            InitilizerState::WaitRxMailboxOffset => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.next_command(desc, sys_time)
            }
            InitilizerState::GetTxMailboxSize => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.start(self.slave_address, sii_reg::StandardTxMailboxSize::ADDRESS);
                sii_reader.next_command(desc, sys_time)
            }
            InitilizerState::WaitTxMailboxSize => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.next_command(desc, sys_time)
            }
            InitilizerState::GetTxMailboxOffset => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.start(
                    self.slave_address,
                    sii_reg::StandardTxMailboxOffset::ADDRESS,
                );
                sii_reader.next_command(desc, sys_time)
            }
            InitilizerState::WaitTxMailboxOffset => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.next_command(desc, sys_time)
            }
            InitilizerState::SetSM0 => {
                let command = Command::new_write(self.slave_address, SyncManagerRegister::ADDRESS0);
                self.buffer.fill(0);
                if let Some(SyncManager::MailboxRx(ref sm0_info)) =
                    self.slave_info.as_mut().unwrap().info.sm0
                {
                    let mut sm = SyncManagerRegister(self.buffer);
                    sm.set_physical_start_address(sm0_info.start_address);
                    sm.set_length(sm0_info.size);
                    sm.set_buffer_type(0b10); //mailbox
                    sm.set_direction(1); //slave read access
                    sm.set_dls_user_event_enable(true);
                    sm.set_watchdog_enable(false); //まずはfalse
                    sm.set_channel_enable(true);
                    sm.set_repeat(false);
                    sm.set_dc_event_w_bus_w(false);
                    sm.set_dc_event_w_loc_w(false);
                }
                Some((command, &self.buffer[..SyncManagerRegister::SIZE]))
            }
            InitilizerState::SetSM1 => {
                let command = Command::new_write(self.slave_address, SyncManagerRegister::ADDRESS1);
                self.buffer.fill(0);
                if let Some(SyncManager::MailboxTx(ref sm1_info)) =
                    self.slave_info.as_mut().unwrap().info.sm1
                {
                    let mut sm = SyncManagerRegister(self.buffer);
                    sm.set_physical_start_address(sm1_info.start_address);
                    sm.set_length(sm1_info.size);
                    sm.set_buffer_type(0b10); //mailbox
                    sm.set_direction(0); //slave write access
                    sm.set_dls_user_event_enable(true);
                    sm.set_watchdog_enable(false); //まずはfalse
                    sm.set_channel_enable(true);
                    sm.set_repeat(false);
                    sm.set_dc_event_w_bus_w(false);
                    sm.set_dc_event_w_loc_w(false);
                }
                Some((command, &self.buffer[..SyncManagerRegister::SIZE]))
            }
            InitilizerState::SetStationAddress => {
                let command = Command::new_write(self.slave_address, FixedStationAddress::ADDRESS);
                self.buffer.fill(0);
                let mut st_addr = FixedStationAddress(self.buffer);
                let addr = match self.slave_address {
                    SlaveAddress::SlaveNumber(addr) => addr,
                    SlaveAddress::StationAddress(addr) => addr,
                };
                st_addr.set_configured_station_address(addr);
                Some((command, &self.buffer[..FixedStationAddress::SIZE]))
            }
            InitilizerState::ClearDCActivation => {
                let command = Command::new_write(self.slave_address, DCActivation::ADDRESS);
                self.buffer.fill(0);
                Some((command, &self.buffer[..DCActivation::SIZE]))
            }
            InitilizerState::ClearCyclicOperationStartTime => {
                let command =
                    Command::new_write(self.slave_address, CyclicOperationStartTime::ADDRESS);
                self.buffer.fill(0);
                Some((command, &self.buffer[..CyclicOperationStartTime::SIZE]))
            }
            InitilizerState::ClearSync0CycleTime => {
                let command = Command::new_write(self.slave_address, Sync0CycleTime::ADDRESS);
                self.buffer.fill(0);
                Some((command, &self.buffer[..Sync0CycleTime::SIZE]))
            }
            InitilizerState::ClearSync1CycleTime => {
                let command = Command::new_write(self.slave_address, Sync1CycleTime::ADDRESS);
                self.buffer.fill(0);
                Some((command, &self.buffer[..Sync1CycleTime::SIZE]))
            }
            InitilizerState::ClearLatchEdge => {
                let command = Command::new_write(self.slave_address, LatchEdge::ADDRESS);
                self.buffer.fill(0);
                Some((command, &self.buffer[..LatchEdge::SIZE]))
            }
            InitilizerState::ClearLatchEvent => {
                let command = Command::new_write(self.slave_address, LatchEvent::ADDRESS);
                self.buffer.fill(0);
                Some((command, &self.buffer[..LatchEvent::SIZE]))
            }
            InitilizerState::ClearLatch0PositiveEdgeValue => {
                let command =
                    Command::new_write(self.slave_address, Latch0PositiveEdgeValue::ADDRESS);
                self.buffer.fill(0);
                Some((command, &self.buffer[..Latch0PositiveEdgeValue::SIZE]))
            }
            InitilizerState::ClearLatch0NegativeEdgeValue => {
                let command =
                    Command::new_write(self.slave_address, Latch0NegativeEdgeValue::ADDRESS);
                self.buffer.fill(0);
                Some((command, &self.buffer[..Latch0NegativeEdgeValue::SIZE]))
            }
            InitilizerState::ClearLatch1PositiveEdgeValue => {
                let command =
                    Command::new_write(self.slave_address, Latch1PositiveEdgeValue::ADDRESS);
                self.buffer.fill(0);
                Some((command, &self.buffer[..Latch1PositiveEdgeValue::SIZE]))
            }
            InitilizerState::ClearLatch1NegativeEdgeValue => {
                let command =
                    Command::new_write(self.slave_address, Latch1NegativeEdgeValue::ADDRESS);
                self.buffer.fill(0);
                Some((command, &self.buffer[..Latch1NegativeEdgeValue::SIZE]))
            }
        };
        if let Some((command, _)) = command_and_data {
            self.command = command;
        }
        command_and_data
    }

    fn recieve_and_process(
        &mut self,
        command: Command,
        data: &[u8],
        wkc: u16,
        desc: &mut NetworkDescription,
        sys_time: EtherCATSystemTime,
    ) -> bool {
        if command != self.command {
            self.state = InitilizerState::Error(InitError::Common(CommonError::PacketDropped));
        }
        if wkc != 1 {
            self.state = InitilizerState::Error(InitError::Common(CommonError::UnexpectedWKC(wkc)));
        }

        match self.state {
            InitilizerState::Error(_) => {}
            InitilizerState::Idle => {}
            InitilizerState::Complete => {}
            InitilizerState::SetLoopPort => {
                self.state = InitilizerState::RequestInitState;
            }
            InitilizerState::RequestInitState => {
                let al_transfer = self.inner.al_state_transfer().unwrap();
                al_transfer.recieve_and_process(command, data, wkc, desc, sys_time);
                self.state = InitilizerState::WaitInitState;
            }
            InitilizerState::WaitInitState => {
                let al_transfer = self.inner.al_state_transfer().unwrap();
                al_transfer.recieve_and_process(command, data, wkc, desc, sys_time);
                match al_transfer.wait() {
                    Ok(AlState::Init) => {
                        self.slave_info.as_mut().unwrap().al_state = AlState::Init;
                        self.state = InitilizerState::ResetErrorCount;
                    }
                    Ok(_) => unreachable!(),
                    Err(nb::Error::WouldBlock) => {}
                    Err(nb::Error::Other(err)) => {
                        self.state = InitilizerState::Error(InitError::AlStateTransition(err));
                    }
                }
            }
            InitilizerState::ResetErrorCount => self.state = InitilizerState::SetWatchDogDivider,
            InitilizerState::SetWatchDogDivider => self.state = InitilizerState::DisableDLWatchDog,
            InitilizerState::DisableDLWatchDog => self.state = InitilizerState::DisableSMWatchDog,
            InitilizerState::DisableSMWatchDog => self.state = InitilizerState::CheckDLStatus,
            InitilizerState::CheckDLStatus => {
                let dl_status = DLStatus(data);
                if !dl_status.pdi_operational() {
                    self.state = InitilizerState::Error(InitError::FailedToLoadEEPROM);
                } else {
                    let slave = self.slave_info.as_mut().unwrap();
                    slave.linked_ports[0] = dl_status.signal_detection_port0();
                    slave.linked_ports[1] = dl_status.signal_detection_port1();
                    slave.linked_ports[2] = dl_status.signal_detection_port2();
                    slave.linked_ports[3] = dl_status.signal_detection_port3();
                    self.state = InitilizerState::CheckDLInfo;
                }
            }
            InitilizerState::CheckDLInfo => {
                let dl_info = DLInformation(data);
                let slave = self.slave_info.as_mut().unwrap();
                slave.info.ports[0] = dl_info.port0_type();
                slave.info.ports[1] = dl_info.port1_type();
                slave.info.ports[2] = dl_info.port2_type();
                slave.info.ports[3] = dl_info.port3_type();

                slave.info.support_dc = dl_info.dc_supported();
                slave.info.is_dc_range_64bits = dl_info.dc_range();
                slave.info.support_fmmu_bit_operation = !dl_info.fmmu_bit_operation_not_supported();
                slave.info.support_lrw = !dl_info.not_lrw_supported(); //これが無いと事実上プロセスデータに対応しない。
                slave.info.support_rw = !dl_info.not_bafrw_supported(); //これが無いと事実上DCに対応しない。
                slave.info.ram_size_kb = dl_info.ram_size();
                //fmmuの確認
                //2個はないと入出力のどちらかしかできないはず。
                let number_of_fmmu = dl_info.number_of_supported_fmmu_entities();
                //if number_of_fmmu >= 1 {
                //    self.slave_info.fmmu0 = Some(0x0600);
                //}
                //if number_of_fmmu >= 2 {
                //    self.slave_info.fmmu1 = Some(0x0610);
                //}
                slave.info.number_of_fmmu = number_of_fmmu;
                slave.info.number_of_sm = dl_info.number_of_supported_sm_channels();
                self.state = InitilizerState::ClearFMMU0;
            }
            InitilizerState::ClearFMMU0 => self.state = InitilizerState::ClearFMMU1,
            InitilizerState::ClearFMMU1 => self.state = InitilizerState::ClearSM0,
            InitilizerState::ClearSM0 => self.state = InitilizerState::ClearSM1,
            InitilizerState::ClearSM1 => self.state = InitilizerState::ClearSM2,
            InitilizerState::ClearSM2 => self.state = InitilizerState::ClearSM3,
            InitilizerState::ClearSM3 => self.state = InitilizerState::GetVenderID,
            InitilizerState::GetVenderID => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.recieve_and_process(command, data, wkc, desc, sys_time);
                self.state = InitilizerState::WaitVenderID;
            }
            InitilizerState::WaitVenderID => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.recieve_and_process(command, data, wkc, desc, sys_time);
                match sii_reader.wait() {
                    Ok((data, _size)) => {
                        self.slave_info.as_mut().unwrap().info.id.vender_id =
                            data.sii_data() as u16;
                        self.state = InitilizerState::GetProductCode
                    }
                    Err(nb::Error::WouldBlock) => {}
                    Err(nb::Error::Other(err)) => {
                        self.state = InitilizerState::Error(InitError::SII(err));
                    }
                }
            }
            InitilizerState::GetProductCode => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.recieve_and_process(command, data, wkc, desc, sys_time);
                self.state = InitilizerState::WaitProductCode;
            }
            InitilizerState::WaitProductCode => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.recieve_and_process(command, data, wkc, desc, sys_time);
                match sii_reader.wait() {
                    Ok((data, _size)) => {
                        self.slave_info.as_mut().unwrap().info.id.product_code =
                            data.sii_data() as u16;
                        self.state = InitilizerState::GetRevision
                    }
                    Err(nb::Error::WouldBlock) => {}
                    Err(nb::Error::Other(err)) => {
                        self.state = InitilizerState::Error(InitError::SII(err));
                    }
                }
            }
            InitilizerState::GetRevision => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.recieve_and_process(command, data, wkc, desc, sys_time);
                self.state = InitilizerState::WaitRevision;
            }
            InitilizerState::WaitRevision => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.recieve_and_process(command, data, wkc, desc, sys_time);
                match sii_reader.wait() {
                    Ok((data, _size)) => {
                        self.slave_info.as_mut().unwrap().info.id.revision_number =
                            data.sii_data() as u16;
                        self.state = InitilizerState::GetProtocol
                    }
                    Err(nb::Error::WouldBlock) => {}
                    Err(nb::Error::Other(err)) => {
                        self.state = InitilizerState::Error(InitError::SII(err));
                    }
                }
            }
            InitilizerState::GetProtocol => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.recieve_and_process(command, data, wkc, desc, sys_time);
                self.state = InitilizerState::WaitProtocol;
            }
            InitilizerState::WaitProtocol => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.recieve_and_process(command, data, wkc, desc, sys_time);
                match sii_reader.wait() {
                    Ok((data, _size)) => {
                        self.slave_info.as_mut().unwrap().info.support_coe = data.0[0].get_bit(2);
                        self.state = InitilizerState::GetRxMailboxSize
                    }
                    Err(nb::Error::WouldBlock) => {}
                    Err(nb::Error::Other(err)) => {
                        self.state = InitilizerState::Error(InitError::SII(err));
                    }
                }
            }
            InitilizerState::GetRxMailboxSize => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.recieve_and_process(command, data, wkc, desc, sys_time);
                self.state = InitilizerState::WaitRxMailboxSize;
            }
            InitilizerState::WaitRxMailboxSize => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.recieve_and_process(command, data, wkc, desc, sys_time);
                match sii_reader.wait() {
                    Ok((data, _size)) => {
                        if self.slave_info.as_ref().unwrap().info.number_of_sm >= 4
                            && data.sii_data() != 0
                        {
                            self.slave_info.as_mut().unwrap().info.sm0 =
                                Some(SyncManager::MailboxRx(MailboxSyncManager {
                                    size: data.sii_data() as u16,
                                    start_address: 0,
                                }));
                            self.slave_info.as_mut().unwrap().info.sm2 =
                                Some(SyncManager::ProcessDataRx);
                        } else if self.slave_info.as_ref().unwrap().info.number_of_sm >= 2 {
                            self.slave_info.as_mut().unwrap().info.sm0 =
                                Some(SyncManager::ProcessDataRx);
                        }
                        self.state = InitilizerState::GetRxMailboxOffset
                    }
                    Err(nb::Error::WouldBlock) => {}
                    Err(nb::Error::Other(err)) => {
                        self.state = InitilizerState::Error(InitError::SII(err));
                    }
                }
            }
            InitilizerState::GetRxMailboxOffset => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.recieve_and_process(command, data, wkc, desc, sys_time);
                self.state = InitilizerState::WaitRxMailboxOffset;
            }
            InitilizerState::WaitRxMailboxOffset => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.recieve_and_process(command, data, wkc, desc, sys_time);
                match sii_reader.wait() {
                    Ok((data, _size)) => {
                        match self.slave_info.as_mut().unwrap().info.sm0 {
                            Some(SyncManager::MailboxRx(ref mut sm)) => {
                                sm.start_address = data.sii_data() as u16
                            }
                            _ => {}
                        }

                        self.state = InitilizerState::GetTxMailboxSize
                    }
                    Err(nb::Error::WouldBlock) => {}
                    Err(nb::Error::Other(err)) => {
                        self.state = InitilizerState::Error(InitError::SII(err));
                    }
                }
            }
            InitilizerState::GetTxMailboxSize => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.recieve_and_process(command, data, wkc, desc, sys_time);
                self.state = InitilizerState::WaitTxMailboxSize;
            }
            InitilizerState::WaitTxMailboxSize => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.recieve_and_process(command, data, wkc, desc, sys_time);
                match sii_reader.wait() {
                    Ok((data, _size)) => {
                        if self.slave_info.as_ref().unwrap().info.number_of_sm >= 4
                            && data.sii_data() != 0
                        {
                            self.slave_info.as_mut().unwrap().info.sm1 =
                                Some(SyncManager::MailboxTx(MailboxSyncManager {
                                    size: data.sii_data() as u16,
                                    start_address: 0,
                                }));
                        } else if self.slave_info.as_ref().unwrap().info.number_of_sm >= 4 {
                            self.slave_info.as_mut().unwrap().info.sm3 =
                                Some(SyncManager::ProcessDataTx);
                        }
                        self.state = InitilizerState::GetTxMailboxOffset
                    }
                    Err(nb::Error::WouldBlock) => {}
                    Err(nb::Error::Other(err)) => {
                        self.state = InitilizerState::Error(InitError::SII(err));
                    }
                }
            }
            InitilizerState::GetTxMailboxOffset => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.recieve_and_process(command, data, wkc, desc, sys_time);
                self.state = InitilizerState::WaitTxMailboxOffset;
            }
            InitilizerState::WaitTxMailboxOffset => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.recieve_and_process(command, data, wkc, desc, sys_time);
                match sii_reader.wait() {
                    Ok((data, _size)) => {
                        match self.slave_info.as_mut().unwrap().info.sm1 {
                            Some(SyncManager::MailboxTx(ref mut sm)) => {
                                sm.start_address = data.sii_data() as u16
                            }
                            _ => {}
                        }
                        set_process_data_sm_size_offset(
                            &mut self.slave_info.as_mut().unwrap().info,
                        );

                        self.state = InitilizerState::SetSM0
                    }
                    Err(nb::Error::WouldBlock) => {}
                    Err(nb::Error::Other(err)) => {
                        self.state = InitilizerState::Error(InitError::SII(err));
                    }
                }
            }
            InitilizerState::SetSM0 => self.state = InitilizerState::SetSM1,
            InitilizerState::SetSM1 => self.state = InitilizerState::SetStationAddress,
            InitilizerState::SetStationAddress => self.state = InitilizerState::ClearDCActivation,
            InitilizerState::ClearDCActivation => {
                self.state = InitilizerState::ClearCyclicOperationStartTime
            }
            InitilizerState::ClearCyclicOperationStartTime => {
                self.state = InitilizerState::ClearSync0CycleTime
            }
            InitilizerState::ClearSync0CycleTime => {
                self.state = InitilizerState::ClearSync1CycleTime
            }
            InitilizerState::ClearSync1CycleTime => self.state = InitilizerState::ClearLatchEdge,
            InitilizerState::ClearLatchEdge => self.state = InitilizerState::ClearLatchEvent,
            InitilizerState::ClearLatchEvent => {
                self.state = InitilizerState::ClearLatch0PositiveEdgeValue
            }
            InitilizerState::ClearLatch0PositiveEdgeValue => {
                self.state = InitilizerState::ClearLatch0NegativeEdgeValue
            }
            InitilizerState::ClearLatch0NegativeEdgeValue => {
                self.state = InitilizerState::ClearLatch1PositiveEdgeValue
            }
            InitilizerState::ClearLatch1PositiveEdgeValue => {
                self.state = InitilizerState::ClearLatch1NegativeEdgeValue
            }
            InitilizerState::ClearLatch1NegativeEdgeValue => self.state = InitilizerState::Complete,
        }

        if let InitilizerState::Error(_) = self.state {
            false
        } else {
            true
        }
    }
}

const fn buffer_size() -> usize {
    let mut size = 0;
    size = const_max(size, DLControl::SIZE);
    size = const_max(size, RxErrorCounter::SIZE);
    size = const_max(size, WatchDogDivider::SIZE);
    size = const_max(size, DLUserWatchDog::SIZE);
    size = const_max(size, SyncManagerChannelWatchDog::SIZE);
    size = const_max(size, DLStatus::SIZE);
    size = const_max(size, DLInformation::SIZE);
    size = const_max(size, FMMURegister::SIZE);
    size = const_max(size, SyncManagerRegister::SIZE);
    size = const_max(size, FixedStationAddress::SIZE);
    size = const_max(size, DCActivation::SIZE);
    size = const_max(size, CyclicOperationStartTime::SIZE);
    size = const_max(size, Sync0CycleTime::SIZE);
    size = const_max(size, Sync1CycleTime::SIZE);
    size = const_max(size, LatchEdge::SIZE);
    size = const_max(size, LatchEvent::SIZE);
    size = const_max(size, Latch0PositiveEdgeValue::SIZE);
    size = const_max(size, Latch0NegativeEdgeValue::SIZE);
    size = const_max(size, Latch1PositiveEdgeValue::SIZE);
    size = const_max(size, Latch1NegativeEdgeValue::SIZE);
    size = const_max(size, Latch0NegativeEdgeValue::SIZE);
    size = const_max(size, Latch0NegativeEdgeValue::SIZE);
    size
}

fn set_process_data_sm_size_offset(slave: &mut SlaveInfo) {
    if let (Some(SyncManager::MailboxRx(ref sm0)), Some(SyncManager::MailboxTx(ref sm1))) =
        (&slave.sm0, &slave.sm1)
    {
        let sm_address0 = sm0.start_address;
        let sm_size0 = sm0.size;
        let sm_address1 = sm1.start_address;
        let sm_size1 = sm1.size;
        let sm_start_address = sm_address0.min(sm_address1);
        let size1 = if sm_start_address > 0x1000 {
            sm_start_address - 0x1000
        } else {
            0
        };
        let sm_end_address = (sm_address0 + sm_size0 - 1).max(sm_address1 + sm_size1 - 1);
        let end_address = slave.ram_size_kb as u16 * 0x0400 - 1;
        let size2 = if end_address > sm_end_address {
            end_address - sm_end_address
        } else {
            0
        };
        if size1 > size2 {
            slave.pdo_start_address = Some(0x1000);
            slave.pdo_ram_size = size1;
        } else {
            slave.pdo_start_address = Some(sm_end_address + 1);
            slave.pdo_ram_size = size2;
        }
    } else {
        slave.pdo_start_address = None;
    }
}

#[derive(Debug)]
enum InnerFunction {
    This,
    SII(SIIReader),
    ALStateTransfer(ALStateTransfer),
}

impl Default for InnerFunction {
    fn default() -> Self {
        Self::This
    }
}

impl InnerFunction {
    //fn into_this(&mut self) {
    //    if let Self::This = &self {
    //        return;
    //    }
    //    *self = Self::This;
    //    //match core::mem::take(self) {
    //    //    Self::Taken => unreachable!(),
    //    //    Self::Owned(_) => unreachable!(),
    //    //    Self::ALStateTransfer(al_transfer) => {
    //    //        *self = InnerFunction::Owned(al_transfer.take_timer());
    //    //    }
    //    //    Self::SII(sii) => {
    //    //        *self = InnerFunction::Owned(sii.take_timer());
    //    //    }
    //    //}
    //}

    fn into_sii(&mut self) {
        if let Self::SII(_) = &self {
            return;
        }
        *self = Self::SII(SIIReader::new());

        //match core::mem::take(self) {
        //    Self::Taken => unreachable!(),
        //    Self::Owned(timer) => {
        //        *self = InnerFunction::SII(SIIReader::new(timer));
        //    }
        //    Self::ALStateTransfer(al_transfer) => {
        //        *self = InnerFunction::SII(SIIReader::new(al_transfer.take_timer()));
        //    }
        //    Self::SII(_) => unreachable!(),
        //}
    }

    fn into_al_state_transfer(&mut self) {
        if let Self::ALStateTransfer(_) = &self {
            return;
        }
        *self = Self::ALStateTransfer(ALStateTransfer::new());
        //match core::mem::take(self) {
        //    Self::Taken => unreachable!(),
        //    Self::Owned(timer) => {
        //        *self = InnerFunction::ALStateTransfer(ALStateTransfer::new(timer));
        //    }
        //    Self::ALStateTransfer(_) => unreachable!(),
        //    Self::SII(sii) => {
        //        *self = InnerFunction::ALStateTransfer(ALStateTransfer::new(sii.take_timer()));
        //    }
        //}
    }

    //fn owned_timer(&mut self) -> Option<&mut T> {
    //    if let Self::Owned(timer) = self {
    //        Some(timer)
    //    } else {
    //        None
    //    }
    //}

    fn sii(&mut self) -> Option<&mut SIIReader> {
        if let Self::SII(sii) = self {
            Some(sii)
        } else {
            None
        }
    }

    fn al_state_transfer(&mut self) -> Option<&mut ALStateTransfer> {
        if let Self::ALStateTransfer(al) = self {
            Some(al)
        } else {
            None
        }
    }
}
