use crate::al_state_transfer::*;
use crate::arch::*;
use crate::error::*;
use crate::interface::*;
use crate::master::Cyclic;
use crate::packet::*;
use crate::register::datalink::*;
use crate::sii::*;
use crate::slave_status::*;
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
    TooManySlaves,
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

#[derive(Debug, Clone)]
pub enum ConfiguredAddress {
    StationAlias,
    StationAddress(u16),
}

#[derive(Debug)]
enum InitilizerState {
    Idle,
    Error(InitError),
    Complete,
    SetLoopPort,
    RequestInitState,
    WaitInitState,
    ResetErrorCount,
    SetWatchDogDivider,
    SetDLWatchDog,
    SetSMWatchDog,
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
    GetTxMailboxSize,
    WaitTxMailboxSize,
    SetSM0,
    SetSM1,
    SetStationAddress,
    ClearDC,
}

#[derive(Debug)]
enum InnerFunction<'a, T>
where
    T: CountDown<Time = MicrosDurationU32>,
{
    None,
    This(&'a mut T),
    SII(SIIReader<'a, T>),
    ALTransfer(ALStateTransfer<'a, T>),
}

impl<'a, T> Default for InnerFunction<'a, T>
where
    T: CountDown<Time = MicrosDurationU32>,
{
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug)]
pub struct SlaveInitilizer<'a, T>
where
    T: CountDown<Time = MicrosDurationU32>,
{
    inner: InnerFunction<'a, T>,
    slave_address: SlaveAddress,
    state: InitilizerState,
    command: Command,
    buffer: [u8; buffer_size()],
    //al_transfer: Option<ALStateTransfer<'a, T>>,
    slave_info: Slave,
}

impl<'a, T> Cyclic for SlaveInitilizer<'a, T>
where
    T: CountDown<Time = MicrosDurationU32>,
{
    fn next_transmission_data(&mut self) -> Option<(Command, &[u8])> {
        match self.state {
            InitilizerState::Idle => None,
            InitilizerState::Error(_) => None,
            InitilizerState::Complete => None,
            InitilizerState::SetLoopPort => {
                self.command = Command::new_write(self.slave_address, DLControl::ADDRESS);
                self.buffer.fill(0);
                // ループポートを設定する。
                // ・EtherCAT以外のフレームを削除する。
                // ・ソースMACアドレスを変更して送信する。
                // ・ポートを自動開閉する。
                let mut dl_control = DLControl(self.buffer);
                dl_control.set_forwarding_rule(true);
                dl_control.set_tx_buffer_size(7);
                Some((self.command, &self.buffer[..DLControl::SIZE]))
            }
            InitilizerState::RequestInitState => {
                if let InnerFunction::This(timer) = core::mem::take(&mut self.inner) {
                    self.inner = InnerFunction::ALTransfer(ALStateTransfer::new(timer));
                } else {
                    unreachable!();
                }

                if let InnerFunction::ALTransfer(ref mut al_transfer) = self.inner {
                    let not_busy = al_transfer.start(self.slave_address, AlState::Init);
                    assert!(not_busy);
                    al_transfer.next_transmission_data()
                } else {
                    unreachable!();
                }
            }
            InitilizerState::WaitInitState => {
                if let InnerFunction::ALTransfer(ref mut al_transfer) = self.inner {
                    al_transfer.next_transmission_data()
                } else {
                    unreachable!();
                }
            }
            InitilizerState::ResetErrorCount => {
                self.command = Command::new_write(self.slave_address, RxErrorCounter::ADDRESS);
                self.buffer.fill(0);
                Some((self.command, &self.buffer[..RxErrorCounter::SIZE]))
            }
            InitilizerState::SetWatchDogDivider => {
                self.command = Command::new_write(self.slave_address, WatchDogDivider::ADDRESS);
                self.buffer.fill(0);
                let mut watchdog_div = WatchDogDivider(self.buffer);
                watchdog_div.set_watch_dog_divider(2498); //100us(default)
                Some((self.command, &self.buffer[..WatchDogDivider::SIZE]))
            }
            InitilizerState::SetDLWatchDog => {
                self.command = Command::new_write(self.slave_address, DLUserWatchDog::ADDRESS);
                self.buffer.fill(0);
                // disable dl watch dog
                Some((self.command, &self.buffer[..DLUserWatchDog::SIZE]))
            }
            InitilizerState::SetSMWatchDog => {
                self.command =
                    Command::new_write(self.slave_address, SyncManagerChannelWatchDog::ADDRESS);
                self.buffer.fill(0);
                // disable sm watch dog
                Some((
                    self.command,
                    &self.buffer[..SyncManagerChannelWatchDog::SIZE],
                ))
            }
            InitilizerState::CheckDLStatus => {
                // ポートと、EEPROMのロード状況を確認する。
                self.command = Command::new_read(self.slave_address, DLStatus::ADDRESS);
                self.buffer.fill(0);
                Some((self.command, &self.buffer[..DLStatus::SIZE]))
            }
            InitilizerState::CheckDLInfo => {
                // 各種サポート状況の確認
                self.command = Command::new_read(self.slave_address, DLInformation::ADDRESS);
                self.buffer.fill(0);
                Some((self.command, &self.buffer[..DLInformation::SIZE]))
            }
            InitilizerState::ClearFMMU0 => {
                self.command = Command::new_write(self.slave_address, FMMURegister::ADDRESS0);
                self.buffer.fill(0);
                // disable dl watch dog
                Some((self.command, &self.buffer[..FMMURegister::SIZE]))
            }
            InitilizerState::ClearFMMU1 => {
                self.command = Command::new_write(self.slave_address, FMMURegister::ADDRESS1);
                self.buffer.fill(0);
                // disable dl watch dog
                Some((self.command, &self.buffer[..FMMURegister::SIZE]))
            }
            InitilizerState::ClearSM0 => {
                self.command =
                    Command::new_write(self.slave_address, SyncManagerRegister::ADDRESS0);
                self.buffer.fill(0);
                // disable dl watch dog
                Some((self.command, &self.buffer[..SyncManagerRegister::SIZE]))
            }
            InitilizerState::ClearSM1 => {
                self.command =
                    Command::new_write(self.slave_address, SyncManagerRegister::ADDRESS1);
                self.buffer.fill(0);
                // disable dl watch dog
                Some((self.command, &self.buffer[..SyncManagerRegister::SIZE]))
            }
            InitilizerState::ClearSM2 => {
                self.command =
                    Command::new_write(self.slave_address, SyncManagerRegister::ADDRESS2);
                self.buffer.fill(0);
                // disable dl watch dog
                Some((self.command, &self.buffer[..SyncManagerRegister::SIZE]))
            }
            InitilizerState::ClearSM3 => {
                self.command =
                    Command::new_write(self.slave_address, SyncManagerRegister::ADDRESS3);
                self.buffer.fill(0);
                // disable dl watch dog
                Some((self.command, &self.buffer[..SyncManagerRegister::SIZE]))
            }
            InitilizerState::GetVenderID => {
                if let InnerFunction::ALTransfer(ref mut al_transfer) = self.inner {
                    if let Some(timer) = core::mem::take(&mut al_transfer.timer){
                        self.inner = InnerFunction::SII(SIIReader::new(timer));
                    }
                } else {
                    unreachable!();
                }

                if let InnerFunction::SII(ref mut sii_reader) = self.inner {
                    let not_busy = sii_reader.start(self.slave_address, sii_reg::VenderID::ADDRESS);
                    assert!(not_busy);
                    sii_reader.next_transmission_data()
                } else {
                    unreachable!();
                }
            }
            InitilizerState::WaitVenderID => {
                if let InnerFunction::SII(ref mut sii_reader) = self.inner {
                    sii_reader.next_transmission_data()
                } else {
                    unreachable!();
                }
            }
            InitilizerState::GetProductCode => {
                if let InnerFunction::SII(ref mut sii_reader) = self.inner {
                    let not_busy = sii_reader.start(self.slave_address, sii_reg::ProductCode::ADDRESS);
                    assert!(not_busy);
                    sii_reader.next_transmission_data()
                } else {
                    unreachable!();
                }
            }
            InitilizerState::WaitProductCode => {
                if let InnerFunction::SII(ref mut sii_reader) = self.inner {
                    sii_reader.next_transmission_data()
                } else {
                    unreachable!();
                }
            }
            InitilizerState::GetRevision => {
                if let InnerFunction::SII(ref mut sii_reader) = self.inner {
                    let not_busy = sii_reader.start(self.slave_address, sii_reg::RevisionNumber::ADDRESS);
                    assert!(not_busy);
                    sii_reader.next_transmission_data()
                } else {
                    unreachable!();
                }
            }
            InitilizerState::WaitRevision => {
                if let InnerFunction::SII(ref mut sii_reader) = self.inner {
                    sii_reader.next_transmission_data()
                } else {
                    unreachable!();
                }
            }
        }
    }

    fn recieve_and_process(&mut self, command: Command, data: &[u8], wkc: u16) -> bool {
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
                if let InnerFunction::ALTransfer(ref mut al_transfer) = self.inner {
                    if !al_transfer.recieve_and_process(command, data, wkc) {
                        self.state = InitilizerState::Error(InitError::AlStateTransition(
                            al_transfer.error().unwrap(),
                        ))
                    } else {
                        self.state = InitilizerState::WaitInitState;
                    }
                } else {
                    unreachable!()
                }
            }
            InitilizerState::WaitInitState => {
                if let InnerFunction::ALTransfer(ref mut al_transfer) = self.inner {
                    if !al_transfer.recieve_and_process(command, data, wkc) {
                        self.state = InitilizerState::Error(InitError::AlStateTransition(
                            al_transfer.error().unwrap(),
                        ))
                    } else {
                        match al_transfer.wait_al_state() {
                            Ok(Some(AlState::Init)) => {
                                self.state = InitilizerState::ResetErrorCount
                            }
                            Ok(None) => {}
                            Err(err) => {
                                self.state =
                                    InitilizerState::Error(InitError::AlStateTransition(err));
                            }
                            _ => unreachable!(),
                        }
                        //self.state = InitilizerState::WaitInitState;
                    }
                } else {
                    unreachable!()
                }
            }
            InitilizerState::ResetErrorCount => self.state = InitilizerState::SetWatchDogDivider,
            InitilizerState::SetWatchDogDivider => self.state = InitilizerState::SetDLWatchDog,
            InitilizerState::SetDLWatchDog => self.state = InitilizerState::SetSMWatchDog,
            InitilizerState::SetSMWatchDog => self.state = InitilizerState::CheckDLStatus,
            InitilizerState::CheckDLStatus => {
                let dl_status = DLStatus(data);
                if !dl_status.pdi_operational() {
                    self.state = InitilizerState::Error(InitError::FailedToLoadEEPROM);
                } else {
                    self.slave_info.linked_ports[0] = dl_status.signal_detection_port0();
                    self.slave_info.linked_ports[1] = dl_status.signal_detection_port1();
                    self.slave_info.linked_ports[2] = dl_status.signal_detection_port2();
                    self.slave_info.linked_ports[3] = dl_status.signal_detection_port3();
                    self.state = InitilizerState::CheckDLInfo;
                }
            }
            InitilizerState::CheckDLInfo => {
                let dl_info = DLInformation(data);
                self.slave_info.ports[0] = dl_info.port0_type();
                self.slave_info.ports[1] = dl_info.port1_type();
                self.slave_info.ports[2] = dl_info.port2_type();
                self.slave_info.ports[3] = dl_info.port3_type();

                self.slave_info.support_dc = dl_info.dc_supported();
                self.slave_info.is_dc_range_64bits = dl_info.dc_range();
                self.slave_info.support_fmmu_bit_operation =
                    !dl_info.fmmu_bit_operation_not_supported();
                self.slave_info.support_lrw = !dl_info.not_lrw_supported(); //これが無いと事実上プロセスデータに対応しない。
                self.slave_info.support_rw = !dl_info.not_bafrw_supported(); //これが無いと事実上DCに対応しない。
                self.slave_info.ram_size_kb = dl_info.ram_size();
                //fmmuの確認
                //2個はないと入出力のどちらかしかできないはず。
                let number_of_fmmu = dl_info.number_of_supported_fmmu_entities();
                if number_of_fmmu >= 1 {
                    self.slave_info.fmmu0 = Some(0x0600);
                }
                if number_of_fmmu >= 2 {
                    self.slave_info.fmmu1 = Some(0x0610);
                }
                self.slave_info.number_of_sm = dl_info.number_of_supported_sm_channels();
                self.state = InitilizerState::ClearFMMU0;
            }
            InitilizerState::ClearFMMU0 => self.state = InitilizerState::ClearFMMU1,
            InitilizerState::ClearFMMU1 => self.state = InitilizerState::ClearSM0,
            InitilizerState::ClearSM0 => self.state = InitilizerState::ClearSM1,
            InitilizerState::ClearSM1 => self.state = InitilizerState::ClearSM2,
            InitilizerState::ClearSM2 => self.state = InitilizerState::ClearSM3,
            InitilizerState::ClearSM3 => self.state = InitilizerState::GetVenderID,
            InitilizerState::GetVenderID => {
                if let InnerFunction::SII(ref mut sii_reader) = self.inner {
                    if !sii_reader.recieve_and_process(command, data, wkc) {
                        self.state = InitilizerState::Error(InitError::SII(
                            sii_reader.error().unwrap(),
                        ))
                    } else {
                        self.state = InitilizerState::WaitVenderID;
                    }
                } else {
                    unreachable!()
                }
            }
            InitilizerState::WaitVenderID => {
                if let InnerFunction::SII(ref mut sii_reader) = self.inner {
                    if !sii_reader.recieve_and_process(command, data, wkc) {
                        self.state = InitilizerState::Error(InitError::SII(
                            sii_reader.error().unwrap(),
                        ))
                    } else {
                        match sii_reader.wait_read_data() {
                            Ok(Some((data, _size))) => {
                                self.slave_info.id.vender_id = data.sii_data() as u16;
                                self.state = InitilizerState::GetProductCode
                            }
                            Ok(None) => {}
                            Err(err) => {
                                self.state =
                                    InitilizerState::Error(InitError::SII(err));
                            }
                            _ => unreachable!(),
                        }
                    }
                } else {
                    unreachable!()
                }
            }
            InitilizerState::GetProductCode => {
                if let InnerFunction::SII(ref mut sii_reader) = self.inner {
                    if !sii_reader.recieve_and_process(command, data, wkc) {
                        self.state = InitilizerState::Error(InitError::SII(
                            sii_reader.error().unwrap(),
                        ))
                    } else {
                        self.state = InitilizerState::WaitProductCode;
                    }
                } else {
                    unreachable!()
                }
            }
            InitilizerState::WaitProductCode => {
                if let InnerFunction::SII(ref mut sii_reader) = self.inner {
                    if !sii_reader.recieve_and_process(command, data, wkc) {
                        self.state = InitilizerState::Error(InitError::SII(
                            sii_reader.error().unwrap(),
                        ))
                    } else {
                        match sii_reader.wait_read_data() {
                            Ok(Some((data, _size))) => {
                                self.slave_info.id.product_code = data.sii_data() as u16;
                                self.state = InitilizerState::GetRevision
                            }
                            Ok(None) => {}
                            Err(err) => {
                                self.state =
                                    InitilizerState::Error(InitError::SII(err));
                            }
                        }
                    }
                } else {
                    unreachable!()
                }
            }
            InitilizerState::GetRevision => {
                if let InnerFunction::SII(ref mut sii_reader) = self.inner {
                    if !sii_reader.recieve_and_process(command, data, wkc) {
                        self.state = InitilizerState::Error(InitError::SII(
                            sii_reader.error().unwrap(),
                        ))
                    } else {
                        self.state = InitilizerState::WaitRevision;
                    }
                } else {
                    unreachable!()
                }
            }
            InitilizerState::WaitRevision => {
                if let InnerFunction::SII(ref mut sii_reader) = self.inner {
                    if !sii_reader.recieve_and_process(command, data, wkc) {
                        self.state = InitilizerState::Error(InitError::SII(
                            sii_reader.error().unwrap(),
                        ))
                    } else {
                        match sii_reader.wait_read_data() {
                            Ok(Some((data, _size))) => {
                                self.slave_info.id.revision_number = data.sii_data() as u16;
                                self.state = InitilizerState::GetProtocol
                            }
                            Ok(None) => {}
                            Err(err) => {
                                self.state =
                                    InitilizerState::Error(InitError::SII(err));
                            }
                            _ => unreachable!(),
                        }
                    }
                } else {
                    unreachable!()
                }
            }


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
    size
}
//pub struct SlaveInitilizer<'a, D, T, U>
//where
//    D: Device,
//    T: CountDown<Time = MicrosDurationU32>,
//    U: CountDown<Time = MicrosDurationU32>,
//{
//    iface: &'a mut EtherCATInterface<'a, D, T>,
//    timer: &'a mut U,
//}
//
//impl<'a, D, T, U> SlaveInitilizer<'a, D, T, U>
//where
//    D: Device,
//    T: CountDown<Time = MicrosDurationU32>,
//    U: CountDown<Time = MicrosDurationU32>,
//{
//    pub fn new(iface: &'a mut EtherCATInterface<'a, D, T>, timer: &'a mut U) -> Self {
//        Self { iface, timer }
//    }
//
//    pub fn init_slaves(&mut self, slave_buffer: &mut [Slave]) -> Result<(), InitError> {
//        let num_slaves = self.count_slaves()?;
//        if num_slaves as usize > slave_buffer.len() {
//            return Err(InitError::TooManySlaves);
//        }
//
//        for i in 0..num_slaves {
//            let slave = self.init_slave(i)?;
//            slave_buffer[i as usize] = slave.unwrap();
//        }
//        Ok(())
//    }
//
//    pub fn count_slaves(&mut self) -> Result<u16, InitError> {
//        let mut wkc = 0;
//        loop {
//            self.iface
//                .add_command(u8::MAX, CommandType::BRD, 0, 0, 1, |_| ())?;
//            self.iface.poll(MicrosDurationU32::from_ticks(1000))?;
//            let pdu = self
//                .iface
//                .consume_command()
//                .last()
//                .ok_or(CommonError::PacketDropped)?;
//            let new_wkc = pdu.wkc().ok_or(CommonError::PacketDropped)?;
//            if wkc == new_wkc {
//                wkc = new_wkc;
//                break;
//            } else {
//                wkc = new_wkc;
//            }
//        }
//
//        Ok(wkc)
//    }
//
//    pub fn station_alias(&mut self, slave: &Slave) -> Result<u16, InitError> {
//        let position_address = SlaveAddress::SlaveNumber(slave.position_address);
//        let mut sii = SlaveInformationInterface::new(&mut self.iface);
//        let (station_alias, _size) = sii.read(position_address, sii_reg::StationAlias::ADDRESS)?;
//        Ok(station_alias.sii_data() as u16)
//    }
//
//    pub fn enable_station_alias(
//        &mut self,
//        slave: &mut Slave,
//        enable: bool,
//    ) -> Result<(), InitError> {
//        let position_address = SlaveAddress::SlaveNumber(slave.position_address);
//        let mut dl_control = self.iface.read_dl_control(position_address)?;
//        dl_control.set_enable_alias_address(enable);
//        self.iface
//            .write_dl_control(position_address, Some(dl_control))?;
//        let alias = self.station_alias(slave)?;
//        slave.configured_address = alias;
//        Ok(())
//    }
//
//    pub fn set_station_address(
//        &mut self,
//        slave: &mut Slave,
//        address: u16,
//    ) -> Result<(), InitError> {
//        let position_address = SlaveAddress::SlaveNumber(slave.position_address);
//        let mut fixed_st = self.iface.read_fixed_station_address(position_address)?;
//        fixed_st.set_configured_station_address(address);
//        self.iface
//            .write_fixed_station_address(position_address, Some(fixed_st))?;
//        slave.configured_address = address;
//        Ok(())
//    }
//
//    // TODO：もっと分解する
//    fn init_slave(&mut self, slave_number: u16) -> Result<Option<Slave>, InitError> {
//        let count = self.count_slaves()?;
//        if slave_number >= count {
//            return Ok(None);
//        }
//
//        let mut slave = Slave::default();
//        slave.position_address = slave_number;
//
//        // ループポートを設定する。
//        // ・EtherCAT以外のフレームを削除する。
//        // ・ソースMACアドレスを変更して送信する。
//        // ・ポートを自動開閉する。
//        let mut dl_control = DLControl::new();
//        dl_control.set_forwarding_rule(true);
//        dl_control.set_tx_buffer_size(7);
//        self.iface
//            .write_dl_control(SlaveAddress::SlaveNumber(slave_number), Some(dl_control))?;
//
//        // INIT状態にする
//        // 一応ループポートの設定の後にしている。
//        let mut al_transfer = ALStateTransfer::new(self.iface, self.timer);
//        al_transfer.change_al_state(SlaveAddress::SlaveNumber(slave_number), AlState::Init)?;
//        slave.al_state = AlState::Init;
//
//        // エラーカウンタをリセットする。
//        self.iface
//            .write_rx_error_counter(SlaveAddress::SlaveNumber(slave_number), None)?;
//
//        // Watch dogの基本インクリメント値にデフォルト値を設定する
//        let mut watchdog_div = WatchDogDivider::new();
//        watchdog_div.set_watch_dog_divider(2498); //100us(default)
//        self.iface
//            .write_watch_dog_divider(SlaveAddress::SlaveNumber(slave_number), Some(watchdog_div))?;
//
//        // データリンクWatchdogにデフォルト値を設定する。
//        let mut dl_watchdog = DLUserWatchDog::new();
//        dl_watchdog.set_dls_user_watch_dog(1000); //defalut 100ms
//        self.iface
//            .write_dl_user_watch_dog(SlaveAddress::SlaveNumber(slave_number), Some(dl_watchdog))?;
//
//        // シンクマネージャーWatchdogにデフォルト値を設定する。
//        let mut sm_watchdog = SyncManagerChannelWatchDog::new();
//        sm_watchdog.set_sm_channel_watch_dog(1000); //defalut 100ms
//        self.iface
//            .write_sm_watch_dog(SlaveAddress::SlaveNumber(slave_number), Some(sm_watchdog))?;
//
//        // スレーブでEEPROMが正常にロードされたか確認する。
//        self.timer
//            .start(MillisDurationU32::from_ticks(200).convert());
//        loop {
//            let is_pdi_operational = self
//                .iface
//                .read_dl_status(SlaveAddress::SlaveNumber(slave_number))?
//                .pdi_operational();
//            if is_pdi_operational {
//                break;
//            }
//            match self.timer.wait() {
//                Ok(_) => return Err(InitError::FailedToLoadEEPROM),
//                Err(nb::Error::Other(_)) => return Err(InitError::Common(CommonError::UnspcifiedTimerError)),
//                Err(nb::Error::WouldBlock) => (),
//            }
//        }
//
//        // ステーションアドレスを設定する。
//        self.set_station_address(&mut slave, slave_number)?;
//
//        // dlインフォの入手。各種サポート状況の確認
//        let dl_info = self
//            .iface
//            .read_dl_information(SlaveAddress::SlaveNumber(slave_number))?;
//        slave.support_dc = dl_info.dc_supported();
//        slave.is_dc_range_64bits = dl_info.dc_range();
//        slave.support_fmmu_bit_operation = !dl_info.fmmu_bit_operation_not_supported();
//        slave.support_lrw = !dl_info.not_lrw_supported(); //これが無いと事実上プロセスデータに対応しない。
//        slave.support_rw = !dl_info.not_bafrw_supported(); //これが無いと事実上DCに対応しない。
//        slave.ram_size_kb = dl_info.ram_size();
//        //fmmuの確認
//        //2個はないと入出力のどちらかしかできないはず。
//        let number_of_fmmu = dl_info.number_of_supported_fmmu_entities();
//        if number_of_fmmu >= 1 {
//            slave.fmmu0 = Some(0x0600);
//            // FMMUの設定をクリア
//            self.iface
//                .write_fmmu0(SlaveAddress::SlaveNumber(slave_number), None)?;
//        }
//        if number_of_fmmu >= 2 {
//            slave.fmmu1 = Some(0x0610);
//            //FMMUの設定をクリア
//            self.iface
//                .write_fmmu1(SlaveAddress::SlaveNumber(slave_number), None)?;
//        }
//        slave.number_of_sm = dl_info.number_of_supported_sm_channels(); //後で使う
//
//        // ポートの設定
//        let dl_status = self
//            .iface
//            .read_dl_status(SlaveAddress::SlaveNumber(slave_number))?;
//        if dl_status.signal_detection_port0() {
//            slave.ports[0] = dl_info.port0_type();
//        }
//        if dl_status.signal_detection_port1() {
//            slave.ports[1] = dl_info.port1_type();
//        }
//        if dl_status.signal_detection_port2() {
//            slave.ports[2] = dl_info.port2_type();
//        }
//        if dl_status.signal_detection_port3() {
//            slave.ports[3] = dl_info.port3_type();
//        }
//
//        //ベンダーIDとかの設定
//        let mut sii = SlaveInformationInterface::new(&mut self.iface);
//        let (vender_id, _size) = sii.read(
//            SlaveAddress::SlaveNumber(slave_number),
//            sii_reg::VenderID::ADDRESS,
//        )?;
//        slave.id.vender_id = vender_id.sii_data() as u16;
//        let (product_code, _size) = sii.read(
//            SlaveAddress::SlaveNumber(slave_number),
//            sii_reg::ProductCode::ADDRESS,
//        )?;
//        slave.id.product_code = product_code.sii_data() as u16;
//        let (revision_number, _size) = sii.read(
//            SlaveAddress::SlaveNumber(slave_number),
//            sii_reg::RevisionNumber::ADDRESS,
//        )?;
//        slave.id.revision_number = revision_number.sii_data() as u16;
//
//        //シンクマネージャーのサイズとかオフセット
//        // Sync Managerの設定をクリア
//        if slave.number_of_sm >= 1 {
//            self.iface
//                .write_sm0(SlaveAddress::SlaveNumber(slave_number), None)?;
//        }
//        if slave.number_of_sm >= 2 {
//            self.iface
//                .write_sm1(SlaveAddress::SlaveNumber(slave_number), None)?;
//        }
//        if slave.number_of_sm >= 3 {
//            self.iface
//                .write_sm2(SlaveAddress::SlaveNumber(slave_number), None)?;
//        }
//        if slave.number_of_sm >= 4 {
//            self.iface
//                .write_sm3(SlaveAddress::SlaveNumber(slave_number), None)?;
//        }
//        //まずは、メールボックスを使うプロトコルに対応しているか？
//        let (mailbox_protocol, _size) = sii.read(
//            SlaveAddress::SlaveNumber(slave_number),
//            sii_reg::MailboxProtocol::ADDRESS,
//        )?;
//        slave.has_coe = mailbox_protocol.0[0].get_bit(2);
//        slave.has_foe = mailbox_protocol.0[0].get_bit(3);
//        // COEに対応するならメールボックス用のシンクマネージャーがあるはず・・・
//        if slave.has_coe {
//            assert!(slave.number_of_sm >= 2);
//            let (sm_rx_offset, _size) = sii.read(
//                SlaveAddress::SlaveNumber(slave_number),
//                sii_reg::StandardRxMailboxOffset::ADDRESS,
//            )?;
//            let (sm_rx_size, _size) = sii.read(
//                SlaveAddress::SlaveNumber(slave_number),
//                sii_reg::StandardRxMailboxSize::ADDRESS,
//            )?;
//            slave.sm_mailbox_in = Some(MailboxSyncManager {
//                size: sm_rx_size.sii_data() as u16,
//                start_address: sm_rx_offset.sii_data() as u16,
//            });
//            let (sm_tx_offset, _size) = sii.read(
//                SlaveAddress::SlaveNumber(slave_number),
//                sii_reg::StandardTxMailboxOffset::ADDRESS,
//            )?;
//            let (sm_tx_size, _size) = sii.read(
//                SlaveAddress::SlaveNumber(slave_number),
//                sii_reg::StandardTxMailboxSize::ADDRESS,
//            )?;
//            slave.sm_mailbox_out = Some(MailboxSyncManager {
//                size: sm_tx_size.sii_data() as u16,
//                start_address: sm_tx_offset.sii_data() as u16,
//            });
//        }
//        // FOEに対応するなら、ブートストラップ用のシンクマネージャーがあるはず・・・
//        if slave.has_foe {
//            assert!(slave.number_of_sm >= 2);
//            let (bootstrap_sm_rx_offset, _size) = sii.read(
//                SlaveAddress::SlaveNumber(slave_number),
//                sii_reg::BootstrapRxMailboxOffset::ADDRESS,
//            )?;
//            let (bootstrap_sm_rx_size, _size) = sii.read(
//                SlaveAddress::SlaveNumber(slave_number),
//                sii_reg::BootstrapRxMailboxSize::ADDRESS,
//            )?;
//            slave.bootstrap_sm_mailbox_in = Some(MailboxSyncManager {
//                size: bootstrap_sm_rx_size.sii_data() as u16,
//                start_address: bootstrap_sm_rx_offset.sii_data() as u16,
//            });
//            let (bootstrap_sm_tx_offset, _size) = sii.read(
//                SlaveAddress::SlaveNumber(slave_number),
//                sii_reg::BootstrapTxMailboxOffset::ADDRESS,
//            )?;
//            let (bootstrap_sm_tx_size, _size) = sii.read(
//                SlaveAddress::SlaveNumber(slave_number),
//                sii_reg::BootstrapTxMailboxSize::ADDRESS,
//            )?;
//            slave.bootstrap_sm_mailbox_out = Some(MailboxSyncManager {
//                size: bootstrap_sm_tx_size.sii_data() as u16,
//                start_address: bootstrap_sm_tx_offset.sii_data() as u16,
//            });
//        }
//
//        //プロセスデータ用のスタートアドレスを決める。
//        //ただしプロセスデータに対応しているとは限らない。
//        //NOTE: COEを前提とする。
//        if slave.number_of_sm >= 3 && slave.has_coe {
//            let sm_address0 = slave.sm_mailbox_in.unwrap().start_address;
//            let sm_size0 = slave.sm_mailbox_in.unwrap().size;
//            let sm_address1 = slave.sm_mailbox_out.unwrap().start_address;
//            let sm_size1 = slave.sm_mailbox_out.unwrap().size;
//            let sm_start_address = sm_address0.min(sm_address1);
//            let size1 = if sm_start_address > 0x1000 {
//                sm_start_address - 0x1000
//            } else {
//                0
//            };
//            let sm_end_address = (sm_address0 + sm_size0 - 1).max(sm_address1 + sm_size1 - 1);
//            let end_address = slave.ram_size_kb as u16 * 0x0400 - 1;
//            let size2 = if end_address > sm_end_address {
//                end_address - sm_end_address
//            } else {
//                0
//            };
//            if size1 > size2 {
//                slave.pdo_start_address = Some(0x1000);
//                slave.pdo_ram_size = size1;
//            } else {
//                slave.pdo_start_address = Some(sm_end_address + 1);
//                slave.pdo_ram_size = size2;
//            }
//        } else {
//            slave.pdo_start_address = None;
//        }
//
//        //メールボックス用シンクマネージャーの設定
//        if let Some(sm_in) = slave.sm_mailbox_in {
//            let mut sm = SyncManagerRegister::new();
//            sm.set_physical_start_address(sm_in.start_address);
//            sm.set_length(sm_in.size);
//            sm.set_buffer_type(0b10); //mailbox
//            sm.set_direction(1); //slave read access
//            sm.set_dls_user_event_enable(true);
//            sm.set_watchdog_enable(true);
//            sm.set_channel_enable(true);
//            sm.set_repeat(false);
//            sm.set_dc_event_w_bus_w(false);
//            sm.set_dc_event_w_loc_w(false);
//        }
//        if let Some(sm_out) = slave.sm_mailbox_out {
//            let mut sm = SyncManagerRegister::new();
//            sm.set_physical_start_address(sm_out.start_address);
//            sm.set_length(sm_out.size);
//            sm.set_buffer_type(0b10); //mailbox
//            sm.set_direction(0); //slave write access
//            sm.set_dls_user_event_enable(true);
//            sm.set_watchdog_enable(true);
//            sm.set_channel_enable(true);
//            sm.set_repeat(false);
//            sm.set_dc_event_w_bus_w(false);
//            sm.set_dc_event_w_loc_w(false);
//        }
//
//        //DC周りの初期化
//        if slave.support_dc {
//            self.iface
//                .write_dc_activation(SlaveAddress::SlaveNumber(slave_number), None)?;
//            self.iface
//                .write_sync0_cycle_time(SlaveAddress::SlaveNumber(slave_number), None)?;
//            self.iface
//                .write_sync1_cycle_time(SlaveAddress::SlaveNumber(slave_number), None)?;
//            self.iface
//                .write_cyclic_operation_start_time(SlaveAddress::SlaveNumber(slave_number), None)?;
//            self.iface
//                .write_latch0_negative_edge_value(SlaveAddress::SlaveNumber(slave_number), None)?;
//            self.iface
//                .write_latch0_positive_edge_value(SlaveAddress::SlaveNumber(slave_number), None)?;
//            self.iface
//                .write_latch1_negative_edge_value(SlaveAddress::SlaveNumber(slave_number), None)?;
//            self.iface
//                .write_latch1_positive_edge_value(SlaveAddress::SlaveNumber(slave_number), None)?;
//            self.iface
//                .write_latch_edge(SlaveAddress::SlaveNumber(slave_number), None)?;
//            self.iface
//                .write_latch_event(SlaveAddress::SlaveNumber(slave_number), None)?;
//        }
//
//        Ok(Some(slave))
//    }
//}
