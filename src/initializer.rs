use crate::al_state_transfer::*;
use crate::arch::*;
use crate::error::*;
use crate::interface::*;
use crate::packet::*;
use crate::register::datalink::*;
use crate::slave_status::*;
use crate::util::*;
use crate::sii::*;
use bitfield::*;
use embedded_hal::timer::*;
use fugit::*;
use heapless::Vec;

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

#[derive(Debug, Clone)]
pub enum ConfiguredAddress {
    StationAlias,
    StationAddress(u16),
}

pub struct SlaveInitilizer<'a, D, T, U>
where
    D: Device,
    T: CountDown<Time = MicrosDurationU32>,
    U: CountDown<Time = MicrosDurationU32>,
{
    iface: &'a mut EtherCATInterface<'a, D, T>,
    timer: &'a mut U,
}

impl<'a, D, T, U> SlaveInitilizer<'a, D, T, U>
where
    D: Device,
    T: CountDown<Time = MicrosDurationU32>,
    U: CountDown<Time = MicrosDurationU32>,
{
    pub fn new(iface: &'a mut EtherCATInterface<'a, D, T>, timer: &'a mut U) -> Self {
        Self { iface, timer }
    }

    pub fn count_slaves(&mut self) -> Result<u16, InitError> {
        let mut wkc = 0;
        loop {
            self.iface.add_command(CommandType::BRD, 0, 0, 1, |_| ())?;
            self.iface.poll(MicrosDurationU32::from_ticks(1000))?;
            let pdu = self
                .iface
                .consume_command()
                .last()
                .ok_or(CommonError::PacketDropped)?;
            let new_wkc = pdu.wkc().ok_or(CommonError::PacketDropped)?;
            if wkc == new_wkc {
                wkc = new_wkc;
                break;
            } else {
                wkc = new_wkc;
            }
        }

        Ok(wkc)
    }

    fn init_slave(
        &mut self,
        slave_number: u16,
        set_address: ConfiguredAddress,
    ) -> Result<Option<Slave>, InitError> {
        let count = self.count_slaves()?;
        if slave_number >= count {
            return Ok(None);
        }

        let mut slave = Slave::default();
        slave.position_address = slave_number;

        // ループポートを設定する。
        // ・EtherCAT以外のフレームを削除する。
        // ・ソースMACアドレスを変更して送信する。
        // ・ポートを自動開閉する。
        let mut dl_control = DLControl::new();
        dl_control.set_forwarding_rule(true);
        dl_control.set_tx_buffer_size(7);
        if let ConfiguredAddress::StationAlias = set_address {
            dl_control.set_enable_alias_address(true);
        }
        self.iface
            .write_dl_control(SlaveAddress::SlaveNumber(slave_number), Some(dl_control))?;

        // INIT状態にする
        // 一応ループポートの設定の後にしている。
        let mut al_transfer = ALStateTransfer::new(self.iface, self.timer);
        al_transfer.to_init_state(SlaveAddress::SlaveNumber(slave_number))?;
        slave.al_state = AlState::Init;

        // エラーカウンタをリセットする。
        self.iface
            .write_rx_error_counter(SlaveAddress::SlaveNumber(slave_number), None)?;

        // Watch dogの基本インクリメント値にデフォルト値を設定する
        let mut watchdog_div = WatchDogDivider::new();
        watchdog_div.set_watch_dog_divider(2498); //100us(default)
        self.iface
            .write_watch_dog_divider(SlaveAddress::SlaveNumber(slave_number), Some(watchdog_div))?;

        // データリンクのWatchdogにデフォルト値を設定する。
        let mut dl_watchdog = DLUserWatchDog::new();
        dl_watchdog.set_dls_user_watch_dog(1000); //defalut 100ms
        self.iface
            .write_dl_user_watch_dog(SlaveAddress::SlaveNumber(slave_number), Some(dl_watchdog))?;

        // シンクマネージャーのWatchdogにデフォルト値を設定する。
        let mut sm_watchdog = SyncManagerChannelWatchDog::new();
        sm_watchdog.set_sm_channel_watch_dog(1000); //defalut 100ms
        self.iface
            .write_sm_watch_dog(SlaveAddress::SlaveNumber(slave_number), Some(sm_watchdog))?;

        // FMMUの設定をクリア
        self.iface
            .write_fmmu0(SlaveAddress::SlaveNumber(slave_number), None)?;
        self.iface
            .write_fmmu1(SlaveAddress::SlaveNumber(slave_number), None)?;
        self.iface
            .write_fmmu2(SlaveAddress::SlaveNumber(slave_number), None)?;

        // Sync Managerの設定をクリア
        self.iface
            .write_sm0(SlaveAddress::SlaveNumber(slave_number), None)?;
        self.iface
            .write_sm1(SlaveAddress::SlaveNumber(slave_number), None)?;
        self.iface
            .write_sm2(SlaveAddress::SlaveNumber(slave_number), None)?;
        self.iface
            .write_sm3(SlaveAddress::SlaveNumber(slave_number), None)?;

        // スレーブでEEPROMが正常にロードされたか確認する。
        self.timer
            .start(MillisDurationU32::from_ticks(200).convert());
        loop{
            let is_pdi_operational = self
            .iface
            .read_dl_status(SlaveAddress::SlaveNumber(slave_number))?
            .pdi_operational();
            if is_pdi_operational {
                break;
            }
            match self.timer.wait() {
                Ok(_) => return Err(InitError::FailedToLoadEEPROM),
                Err(nb::Error::Other(_)) => {
                    return Err(InitError::Common(CommonError::TimerError))
                }
                Err(nb::Error::WouldBlock) => (),
            }
        }

        // ステーションアドレスを設定する。
        match set_address{
            ConfiguredAddress::StationAddress(station_address) =>{
                let mut fixed_st = self.iface.read_fixed_station_address(SlaveAddress::SlaveNumber(slave_number))?;
                fixed_st.set_configured_station_address(station_address);
                self.iface.write_fixed_station_address(SlaveAddress::SlaveNumber(slave_number), Some(fixed_st))?;
                slave.configured_address = station_address;
            }
            ConfiguredAddress::StationAlias => {
                let fixed_st = self.iface.read_fixed_station_address(SlaveAddress::SlaveNumber(slave_number))?;
                slave.configured_address = fixed_st.configured_station_alias();
            }
        }

        // dlインフォの入手
        let dl_info = self.iface.read_dl_information(SlaveAddress::SlaveNumber(slave_number))?;
        slave.support_dc = dl_info.dc_supported();
        slave.is_dc_range_64bits = dl_info.dc_range();
        slave.support_fmmu_bit_operation = !dl_info.fmmu_bit_operation_not_supported();
        slave.support_lrw = !dl_info.not_lrw_supported();
        slave.support_rw = !dl_info.not_bafrw_supported();

        // 接続ポートの確認
        let dl_status = self.iface.read_dl_status(SlaveAddress::SlaveNumber(slave_number))?;
        if dl_status.signal_detection_port0(){
            slave.ports[0] = dl_info.port0_type();
        }
        if dl_status.signal_detection_port1(){
            slave.ports[1] = dl_info.port1_type();
        }
        if dl_status.signal_detection_port2(){
            slave.ports[2] = dl_info.port2_type();
        }
        if dl_status.signal_detection_port3(){
            slave.ports[3] = dl_info.port3_type();
        }

        //ベンダーIDとか
        let mut sii = SlaveInformationInterface::new(&mut self.iface);
        let (vender_id, _size) = sii.read(SlaveAddress::SlaveNumber(slave_number), sii_reg::VenderID::ADDRESS)?;
        slave.id.vender_id = vender_id.sii_data() as u16;
        let (product_code, _size) = sii.read(SlaveAddress::SlaveNumber(slave_number), sii_reg::ProductCode::ADDRESS)?;
        slave.id.product_code = product_code.sii_data() as u16;
        let (revision_number, _size) = sii.read(SlaveAddress::SlaveNumber(slave_number), sii_reg::RevisionNumber::ADDRESS)?;
        slave.id.revision_number = revision_number.sii_data() as u16;

        todo!()
    }
}
