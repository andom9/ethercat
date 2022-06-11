use super::al_state_transfer;
use super::sii_reader;
use super::{
    al_state_transfer::AlStateTransfer,
    sii_reader::{sii_reg, SIIReader},
};
use super::{EtherCatSystemTime, ReceivedData};
use crate::cyclic::Cyclic;
use crate::error::CommonError;
use crate::interface::{Command, SlaveAddress};
use crate::network::NetworkDescription;
use crate::register::{
    application::{
        CyclicOperationStartTime, DcActivation, Latch0NegativeEdgeValue, Latch0PositiveEdgeValue,
        Latch1NegativeEdgeValue, Latch1PositiveEdgeValue, LatchEdge, LatchEvent, Sync0CycleTime,
        Sync1CycleTime,
    },
    datalink::{
        DLControl, DLInformation, DLStatus, DLUserWatchDog, FMMURegister, FixedStationAddress,
        RxErrorCounter, SyncManagerActivation, SyncManagerChannelWatchDog, SyncManagerControl,
        SyncManagerStatus, WatchDogDivider,
    },
};
use crate::slave::{AlState, MailboxSyncManager, Slave, SlaveInfo, SyncManager};
use crate::util::const_max;
use bit_field::BitField;

#[derive(Debug, Clone)]
pub enum Error {
    Common(CommonError),
    AlStateTransition(al_state_transfer::Error),
    SII(sii_reader::Error),
    FailedToLoadEEPROM,
}

impl From<CommonError> for Error {
    fn from(err: CommonError) -> Self {
        Self::Common(err)
    }
}

impl From<al_state_transfer::Error> for Error {
    fn from(err: al_state_transfer::Error) -> Self {
        Self::AlStateTransition(err)
    }
}

impl From<sii_reader::Error> for Error {
    fn from(err: sii_reader::Error) -> Self {
        Self::SII(err)
    }
}

#[derive(Debug)]
enum State {
    Idle,
    Error(Error),
    SetLoopPort,
    RequestInitState,
    WaitInitState,
    ResetErrorCount,
    SetWatchDogDivider,
    DisableDLWatchDog,
    DisableSMWatchDog,
    CheckDLStatus,
    CheckDLInfo,
    ClearFMMU(u16),
    ClearSM(u16),
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
    SetSM0Control,
    SetSM0Activation,
    SetSM1Control,
    SetSM1Activation,
    SetStationAddress,
    ClearDcActivation,
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
    state: State,
    command: Command,
    buffer: [u8; buffer_size()],
    slave_info: Option<Slave>,
}

impl SlaveInitilizer {
    pub fn new() -> Self {
        Self {
            inner: InnerFunction::This,
            slave_address: SlaveAddress::SlavePosition(0),
            state: State::Idle,
            command: Command::default(),
            buffer: [0; buffer_size()],
            slave_info: None,
        }
    }

    pub fn start(&mut self, slave_position: u16) {
        self.slave_address = SlaveAddress::SlavePosition(slave_position);
        self.state = State::SetLoopPort;
        self.slave_info = Some(Slave::default());
        self.slave_info
            .as_mut()
            .map(|slave| slave.mailbox_count = 1);
    }

    pub fn wait(&mut self) -> nb::Result<Option<Slave>, Error> {
        match &self.state {
            State::Complete => Ok(core::mem::take(&mut self.slave_info)),
            State::Error(err) => Err(nb::Error::Other(err.clone())),
            _ => Err(nb::Error::WouldBlock),
        }
    }
}

impl Cyclic for SlaveInitilizer {
    fn next_command(
        &mut self,
        desc: &mut NetworkDescription,
        sys_time: EtherCatSystemTime,
    ) -> Option<(Command, &[u8])> {
        let command_and_data = match self.state {
            State::Idle => None,
            State::Error(_) => None,
            State::Complete => None,
            State::SetLoopPort => {
                let command = Command::new_write(self.slave_address, DLControl::ADDRESS);
                self.buffer.fill(0);
                // ループポートを設定する。
                // ・EtherCat以外のフレームを削除する。
                // ・ソースMACアドレスを変更して送信する。
                // ・ポートを自動開閉する。
                let mut dl_control = DLControl(self.buffer);
                dl_control.set_forwarding_rule(true);
                dl_control.set_tx_buffer_size(7);
                dl_control.set_enable_alias_address(false);
                Some((command, &self.buffer[..DLControl::SIZE]))
            }
            State::RequestInitState => {
                self.inner.into_al_state_transfer();
                let al_transfer = self.inner.al_state_transfer().unwrap();
                al_transfer.start(self.slave_address, Some(AlState::Init));
                al_transfer.next_command(desc, sys_time)
            }
            State::WaitInitState => {
                let al_transfer = self.inner.al_state_transfer().unwrap();
                al_transfer.next_command(desc, sys_time)
            }
            State::ResetErrorCount => {
                let command = Command::new_write(self.slave_address, RxErrorCounter::ADDRESS);
                self.buffer.fill(0);
                Some((command, &self.buffer[..RxErrorCounter::SIZE]))
            }
            State::SetWatchDogDivider => {
                let command = Command::new_write(self.slave_address, WatchDogDivider::ADDRESS);
                self.buffer.fill(0);
                let mut watchdog_div = WatchDogDivider(self.buffer);
                watchdog_div.set_watch_dog_divider(2498); //100us(default)
                Some((command, &self.buffer[..WatchDogDivider::SIZE]))
            }
            State::DisableDLWatchDog => {
                let command = Command::new_write(self.slave_address, DLUserWatchDog::ADDRESS);
                self.buffer.fill(0);
                // disable dl watch dog
                Some((command, &self.buffer[..DLUserWatchDog::SIZE]))
            }
            State::DisableSMWatchDog => {
                let command =
                    Command::new_write(self.slave_address, SyncManagerChannelWatchDog::ADDRESS);
                self.buffer.fill(0);
                // disable sm watch dog
                Some((command, &self.buffer[..SyncManagerChannelWatchDog::SIZE]))
            }
            State::CheckDLStatus => {
                // ポートと、EEPROMのロード状況を確認する。
                let command = Command::new_read(self.slave_address, DLStatus::ADDRESS);
                self.buffer.fill(0);
                Some((command, &self.buffer[..DLStatus::SIZE]))
            }
            State::CheckDLInfo => {
                // 各種サポート状況の確認
                let command = Command::new_read(self.slave_address, DLInformation::ADDRESS);
                self.buffer.fill(0);
                Some((command, &self.buffer[..DLInformation::SIZE]))
            }
            State::ClearFMMU(count) => {
                let command =
                    Command::new_write(self.slave_address, FMMURegister::ADDRESS + count * 0x10);
                self.buffer.fill(0);
                // disable dl watch dog
                Some((command, &self.buffer[..FMMURegister::SIZE]))
            }
            State::ClearSM(count) => {
                let command = Command::new_write(
                    self.slave_address,
                    SyncManagerControl::ADDRESS * count * 0x08,
                );
                self.buffer.fill(0);
                // disable dl watch dog
                let length = SyncManagerControl::SIZE
                    + SyncManagerStatus::SIZE
                    + SyncManagerActivation::SIZE;
                Some((command, &self.buffer[..length]))
            }
            State::GetVenderID => {
                self.inner.into_sii();
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.start(self.slave_address, sii_reg::VenderID::ADDRESS);
                sii_reader.next_command(desc, sys_time)
            }
            State::WaitVenderID => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.next_command(desc, sys_time)
            }
            State::GetProductCode => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.start(self.slave_address, sii_reg::ProductCode::ADDRESS);
                sii_reader.next_command(desc, sys_time)
            }
            State::WaitProductCode => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.next_command(desc, sys_time)
            }
            State::GetRevision => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.start(self.slave_address, sii_reg::RevisionNumber::ADDRESS);
                sii_reader.next_command(desc, sys_time)
            }
            State::WaitRevision => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.next_command(desc, sys_time)
            }
            State::GetProtocol => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.start(self.slave_address, sii_reg::MailboxProtocol::ADDRESS);
                sii_reader.next_command(desc, sys_time)
            }
            State::WaitProtocol => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.next_command(desc, sys_time)
            }
            State::GetRxMailboxSize => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.start(self.slave_address, sii_reg::StandardRxMailboxSize::ADDRESS);
                sii_reader.next_command(desc, sys_time)
            }
            State::WaitRxMailboxSize => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.next_command(desc, sys_time)
            }
            State::GetRxMailboxOffset => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.start(
                    self.slave_address,
                    sii_reg::StandardRxMailboxOffset::ADDRESS,
                );
                sii_reader.next_command(desc, sys_time)
            }
            State::WaitRxMailboxOffset => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.next_command(desc, sys_time)
            }
            State::GetTxMailboxSize => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.start(self.slave_address, sii_reg::StandardTxMailboxSize::ADDRESS);
                sii_reader.next_command(desc, sys_time)
            }
            State::WaitTxMailboxSize => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.next_command(desc, sys_time)
            }
            State::GetTxMailboxOffset => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.start(
                    self.slave_address,
                    sii_reg::StandardTxMailboxOffset::ADDRESS,
                );
                sii_reader.next_command(desc, sys_time)
            }
            State::WaitTxMailboxOffset => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.next_command(desc, sys_time)
            }
            State::SetSM0Control => {
                let command = Command::new_write(self.slave_address, SyncManagerControl::ADDRESS);
                self.buffer.fill(0);
                if let Some(SyncManager::MailboxRx(ref sm0_info)) =
                    self.slave_info.as_mut().unwrap().info.sm0
                {
                    let mut sm = SyncManagerControl(self.buffer);
                    sm.set_physical_start_address(sm0_info.start_address);
                    sm.set_length(sm0_info.size);
                    sm.set_buffer_type(0b10); //mailbox
                    sm.set_direction(1); //slave read access
                    sm.set_dls_user_event_enable(true);
                    //sm.set_watchdog_enable(false); //まずはfalse
                }
                Some((command, &self.buffer[..SyncManagerControl::SIZE]))
            }
            State::SetSM0Activation => {
                let command =
                    Command::new_write(self.slave_address, SyncManagerActivation::ADDRESS);
                self.buffer.fill(0);
                if let Some(SyncManager::MailboxRx(ref sm0_info)) =
                    self.slave_info.as_mut().unwrap().info.sm0
                {
                    let mut sm = SyncManagerActivation(self.buffer);
                    sm.set_channel_enable(true);
                    sm.set_repeat(false);
                    //sm.set_dc_event_w_bus_w(false);
                    //sm.set_dc_event_w_loc_w(false);
                }
                Some((command, &self.buffer[..SyncManagerActivation::SIZE]))
            }
            State::SetSM1Control => {
                let command =
                    Command::new_write(self.slave_address, SyncManagerControl::ADDRESS + 0x08);
                self.buffer.fill(0);
                if let Some(SyncManager::MailboxTx(ref sm1_info)) =
                    self.slave_info.as_mut().unwrap().info.sm1
                {
                    let mut sm = SyncManagerControl(self.buffer);
                    sm.set_physical_start_address(sm1_info.start_address);
                    sm.set_length(sm1_info.size);
                    sm.set_buffer_type(0b10); //mailbox
                    sm.set_direction(0); //slave write access
                    sm.set_dls_user_event_enable(true);
                    //sm.set_watchdog_enable(false); //まずはfalse
                }
                Some((command, &self.buffer[..SyncManagerControl::SIZE]))
            }
            State::SetSM1Activation => {
                let command =
                    Command::new_write(self.slave_address, SyncManagerActivation::ADDRESS + 0x08);
                self.buffer.fill(0);
                if let Some(SyncManager::MailboxTx(ref sm1_info)) =
                    self.slave_info.as_mut().unwrap().info.sm1
                {
                    let mut sm = SyncManagerActivation(self.buffer);
                    sm.set_channel_enable(true);
                    sm.set_repeat(false);
                    //sm.set_dc_event_w_bus_w(false);
                    //sm.set_dc_event_w_loc_w(false);
                }
                Some((command, &self.buffer[..SyncManagerActivation::SIZE]))
            }
            State::SetStationAddress => {
                let command = Command::new_write(self.slave_address, FixedStationAddress::ADDRESS);
                self.buffer.fill(0);
                let mut st_addr = FixedStationAddress(self.buffer);
                let addr = match self.slave_address {
                    SlaveAddress::SlavePosition(addr) => addr + 1,
                    SlaveAddress::StationAddress(addr) => addr,
                };
                self.slave_info.as_mut().unwrap().configured_address = addr;
                st_addr.set_configured_station_address(addr);
                Some((command, &self.buffer[..FixedStationAddress::SIZE]))
            }
            State::ClearDcActivation => {
                let command = Command::new_write(self.slave_address, DcActivation::ADDRESS);
                self.buffer.fill(0);
                Some((command, &self.buffer[..DcActivation::SIZE]))
            }
            State::ClearCyclicOperationStartTime => {
                let command =
                    Command::new_write(self.slave_address, CyclicOperationStartTime::ADDRESS);
                self.buffer.fill(0);
                Some((command, &self.buffer[..CyclicOperationStartTime::SIZE]))
            }
            State::ClearSync0CycleTime => {
                let command = Command::new_write(self.slave_address, Sync0CycleTime::ADDRESS);
                self.buffer.fill(0);
                Some((command, &self.buffer[..Sync0CycleTime::SIZE]))
            }
            State::ClearSync1CycleTime => {
                let command = Command::new_write(self.slave_address, Sync1CycleTime::ADDRESS);
                self.buffer.fill(0);
                Some((command, &self.buffer[..Sync1CycleTime::SIZE]))
            }
            State::ClearLatchEdge => {
                let command = Command::new_write(self.slave_address, LatchEdge::ADDRESS);
                self.buffer.fill(0);
                Some((command, &self.buffer[..LatchEdge::SIZE]))
            }
            State::ClearLatchEvent => {
                let command = Command::new_write(self.slave_address, LatchEvent::ADDRESS);
                self.buffer.fill(0);
                Some((command, &self.buffer[..LatchEvent::SIZE]))
            }
            State::ClearLatch0PositiveEdgeValue => {
                let command =
                    Command::new_write(self.slave_address, Latch0PositiveEdgeValue::ADDRESS);
                self.buffer.fill(0);
                Some((command, &self.buffer[..Latch0PositiveEdgeValue::SIZE]))
            }
            State::ClearLatch0NegativeEdgeValue => {
                let command =
                    Command::new_write(self.slave_address, Latch0NegativeEdgeValue::ADDRESS);
                self.buffer.fill(0);
                Some((command, &self.buffer[..Latch0NegativeEdgeValue::SIZE]))
            }
            State::ClearLatch1PositiveEdgeValue => {
                let command =
                    Command::new_write(self.slave_address, Latch1PositiveEdgeValue::ADDRESS);
                self.buffer.fill(0);
                Some((command, &self.buffer[..Latch1PositiveEdgeValue::SIZE]))
            }
            State::ClearLatch1NegativeEdgeValue => {
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
        recv_data: Option<ReceivedData>,
        desc: &mut NetworkDescription,
        sys_time: EtherCatSystemTime,
    ) {
        let data = if let Some(ref recv_data) = recv_data {
            let ReceivedData { command, data, wkc } = recv_data;
            if *command != self.command {
                self.state = State::Error(Error::Common(CommonError::BadPacket));
            }
            if *wkc != 1 {
                self.state = State::Error(Error::Common(CommonError::UnexpectedWKC(*wkc)));
            }
            data
        } else {
            self.state = State::Error(Error::Common(CommonError::LostCommand));
            return;
        };

        match self.state {
            State::Error(_) => {}
            State::Idle => {}
            State::Complete => {}
            State::SetLoopPort => {
                self.state = State::RequestInitState;
            }
            State::RequestInitState => {
                let al_transfer = self.inner.al_state_transfer().unwrap();
                al_transfer.recieve_and_process(recv_data, desc, sys_time);
                self.state = State::WaitInitState;
            }
            State::WaitInitState => {
                let al_transfer = self.inner.al_state_transfer().unwrap();
                al_transfer.recieve_and_process(recv_data, desc, sys_time);
                match al_transfer.wait() {
                    Ok(AlState::Init) => {
                        self.slave_info.as_mut().unwrap().al_state = AlState::Init;
                        self.state = State::ResetErrorCount;
                    }
                    Ok(_) => unreachable!(),
                    Err(nb::Error::WouldBlock) => {}
                    Err(nb::Error::Other(err)) => {
                        self.state = State::Error(Error::AlStateTransition(err));
                    }
                }
            }
            State::ResetErrorCount => self.state = State::SetWatchDogDivider,
            State::SetWatchDogDivider => self.state = State::DisableDLWatchDog,
            State::DisableDLWatchDog => self.state = State::DisableSMWatchDog,
            State::DisableSMWatchDog => self.state = State::CheckDLStatus,
            State::CheckDLStatus => {
                let dl_status = DLStatus(data);
                if !dl_status.pdi_operational() {
                    self.state = State::Error(Error::FailedToLoadEEPROM);
                } else {
                    let slave = self.slave_info.as_mut().unwrap();
                    slave.linked_ports[0] = dl_status.signal_detection_port0();
                    slave.linked_ports[1] = dl_status.signal_detection_port1();
                    slave.linked_ports[2] = dl_status.signal_detection_port2();
                    slave.linked_ports[3] = dl_status.signal_detection_port3();
                    self.state = State::CheckDLInfo;
                }
            }
            State::CheckDLInfo => {
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
                slave.info.support_rw = !dl_info.not_bafrw_supported(); //これが無いと事実上Dcに対応しない。
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
                self.state = State::ClearFMMU(0);
            }
            State::ClearFMMU(count) => {
                if count < 1 {
                    self.state = State::ClearFMMU(count + 1);
                } else {
                    self.state = State::ClearSM(0);
                }
            }
            State::ClearSM(count) => {
                if count < 4 {
                    self.state = State::ClearFMMU(count + 1);
                } else {
                    self.state = State::GetVenderID;
                }
            }
            State::GetVenderID => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.recieve_and_process(recv_data, desc, sys_time);
                self.state = State::WaitVenderID;
            }
            State::WaitVenderID => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.recieve_and_process(recv_data, desc, sys_time);
                match sii_reader.wait() {
                    Ok((data, _size)) => {
                        self.slave_info.as_mut().unwrap().info.id.vender_id =
                            data.sii_data() as u16;
                        self.state = State::GetProductCode
                    }
                    Err(nb::Error::WouldBlock) => {}
                    Err(nb::Error::Other(err)) => {
                        self.state = State::Error(Error::SII(err));
                    }
                }
            }
            State::GetProductCode => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.recieve_and_process(recv_data, desc, sys_time);
                self.state = State::WaitProductCode;
            }
            State::WaitProductCode => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.recieve_and_process(recv_data, desc, sys_time);
                match sii_reader.wait() {
                    Ok((data, _size)) => {
                        self.slave_info.as_mut().unwrap().info.id.product_code =
                            data.sii_data() as u16;
                        self.state = State::GetRevision
                    }
                    Err(nb::Error::WouldBlock) => {}
                    Err(nb::Error::Other(err)) => {
                        self.state = State::Error(Error::SII(err));
                    }
                }
            }
            State::GetRevision => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.recieve_and_process(recv_data, desc, sys_time);
                self.state = State::WaitRevision;
            }
            State::WaitRevision => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.recieve_and_process(recv_data, desc, sys_time);
                match sii_reader.wait() {
                    Ok((data, _size)) => {
                        self.slave_info.as_mut().unwrap().info.id.revision_number =
                            data.sii_data() as u16;
                        self.state = State::GetProtocol
                    }
                    Err(nb::Error::WouldBlock) => {}
                    Err(nb::Error::Other(err)) => {
                        self.state = State::Error(Error::SII(err));
                    }
                }
            }
            State::GetProtocol => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.recieve_and_process(recv_data, desc, sys_time);
                self.state = State::WaitProtocol;
            }
            State::WaitProtocol => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.recieve_and_process(recv_data, desc, sys_time);
                match sii_reader.wait() {
                    Ok((data, _size)) => {
                        self.slave_info.as_mut().unwrap().info.support_coe = data.0[0].get_bit(2);
                        self.state = State::GetRxMailboxSize
                    }
                    Err(nb::Error::WouldBlock) => {}
                    Err(nb::Error::Other(err)) => {
                        self.state = State::Error(Error::SII(err));
                    }
                }
            }
            State::GetRxMailboxSize => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.recieve_and_process(recv_data, desc, sys_time);
                self.state = State::WaitRxMailboxSize;
            }
            State::WaitRxMailboxSize => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.recieve_and_process(recv_data, desc, sys_time);
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
                        self.state = State::GetRxMailboxOffset
                    }
                    Err(nb::Error::WouldBlock) => {}
                    Err(nb::Error::Other(err)) => {
                        self.state = State::Error(Error::SII(err));
                    }
                }
            }
            State::GetRxMailboxOffset => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.recieve_and_process(recv_data, desc, sys_time);
                self.state = State::WaitRxMailboxOffset;
            }
            State::WaitRxMailboxOffset => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.recieve_and_process(recv_data, desc, sys_time);
                match sii_reader.wait() {
                    Ok((data, _size)) => {
                        match self.slave_info.as_mut().unwrap().info.sm0 {
                            Some(SyncManager::MailboxRx(ref mut sm)) => {
                                sm.start_address = data.sii_data() as u16
                            }
                            _ => {}
                        }

                        self.state = State::GetTxMailboxSize
                    }
                    Err(nb::Error::WouldBlock) => {}
                    Err(nb::Error::Other(err)) => {
                        self.state = State::Error(Error::SII(err));
                    }
                }
            }
            State::GetTxMailboxSize => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.recieve_and_process(recv_data, desc, sys_time);
                self.state = State::WaitTxMailboxSize;
            }
            State::WaitTxMailboxSize => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.recieve_and_process(recv_data, desc, sys_time);
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
                        self.state = State::GetTxMailboxOffset
                    }
                    Err(nb::Error::WouldBlock) => {}
                    Err(nb::Error::Other(err)) => {
                        self.state = State::Error(Error::SII(err));
                    }
                }
            }
            State::GetTxMailboxOffset => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.recieve_and_process(recv_data, desc, sys_time);
                self.state = State::WaitTxMailboxOffset;
            }
            State::WaitTxMailboxOffset => {
                let sii_reader = self.inner.sii().unwrap();
                sii_reader.recieve_and_process(recv_data, desc, sys_time);
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

                        self.state = State::SetSM0Control
                    }
                    Err(nb::Error::WouldBlock) => {}
                    Err(nb::Error::Other(err)) => {
                        self.state = State::Error(Error::SII(err));
                    }
                }
            }
            State::SetSM0Control => self.state = State::SetSM0Activation,
            State::SetSM0Activation => self.state = State::SetSM1Control,
            State::SetSM1Control => self.state = State::SetSM1Activation,
            State::SetSM1Activation => self.state = State::SetStationAddress,
            State::SetStationAddress => self.state = State::ClearDcActivation,
            State::ClearDcActivation => self.state = State::ClearCyclicOperationStartTime,
            State::ClearCyclicOperationStartTime => self.state = State::ClearSync0CycleTime,
            State::ClearSync0CycleTime => self.state = State::ClearSync1CycleTime,
            State::ClearSync1CycleTime => self.state = State::ClearLatchEdge,
            State::ClearLatchEdge => self.state = State::ClearLatchEvent,
            State::ClearLatchEvent => self.state = State::ClearLatch0PositiveEdgeValue,
            State::ClearLatch0PositiveEdgeValue => self.state = State::ClearLatch0NegativeEdgeValue,
            State::ClearLatch0NegativeEdgeValue => self.state = State::ClearLatch1PositiveEdgeValue,
            State::ClearLatch1PositiveEdgeValue => self.state = State::ClearLatch1NegativeEdgeValue,
            State::ClearLatch1NegativeEdgeValue => self.state = State::Complete,
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
    size = const_max(
        size,
        SyncManagerControl::SIZE + SyncManagerStatus::SIZE + SyncManagerActivation::SIZE,
    );
    size = const_max(size, FixedStationAddress::SIZE);
    size = const_max(size, DcActivation::SIZE);
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
    AlStateTransfer(AlStateTransfer),
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
    //    //    Self::AlStateTransfer(al_transfer) => {
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
        //    Self::AlStateTransfer(al_transfer) => {
        //        *self = InnerFunction::SII(SIIReader::new(al_transfer.take_timer()));
        //    }
        //    Self::SII(_) => unreachable!(),
        //}
    }

    fn into_al_state_transfer(&mut self) {
        if let Self::AlStateTransfer(_) = &self {
            return;
        }
        *self = Self::AlStateTransfer(AlStateTransfer::new());
        //match core::mem::take(self) {
        //    Self::Taken => unreachable!(),
        //    Self::Owned(timer) => {
        //        *self = InnerFunction::AlStateTransfer(AlStateTransfer::new(timer));
        //    }
        //    Self::AlStateTransfer(_) => unreachable!(),
        //    Self::SII(sii) => {
        //        *self = InnerFunction::AlStateTransfer(AlStateTransfer::new(sii.take_timer()));
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

    fn al_state_transfer(&mut self) -> Option<&mut AlStateTransfer> {
        if let Self::AlStateTransfer(al) = self {
            Some(al)
        } else {
            None
        }
    }
}
