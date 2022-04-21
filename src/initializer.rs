use crate::al_state_transfer::*;
use crate::arch::*;
use crate::error::*;
use crate::interface::*;
use crate::packet::*;
use crate::register::datalink::*;
use crate::sii::*;
use crate::slave_status::*;
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

    pub fn init_slaves(&mut self, slave_buffer: &mut [Slave]) -> Result<(), InitError> {
        let num_slaves = self.count_slaves()?;
        if num_slaves as usize > slave_buffer.len() {
            return Err(InitError::TooManySlaves);
        }

        for i in 0..num_slaves {
            let slave = self.init_slave(i)?;
            slave_buffer[i as usize] = slave.unwrap();
        }
        Ok(())
    }

    pub fn count_slaves(&mut self) -> Result<u16, InitError> {
        let mut wkc = 0;
        loop {
            self.iface
                .add_command(u8::MAX, CommandType::BRD, 0, 0, 1, |_| ())?;
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

    pub fn station_alias(&mut self, slave: &Slave) -> Result<u16, InitError> {
        let position_address = SlaveAddress::SlaveNumber(slave.position_address);
        let mut sii = SlaveInformationInterface::new(&mut self.iface);
        let (station_alias, _size) = sii.read(position_address, sii_reg::StationAlias::ADDRESS)?;
        Ok(station_alias.sii_data() as u16)
    }

    pub fn enable_station_alias(
        &mut self,
        slave: &mut Slave,
        enable: bool,
    ) -> Result<(), InitError> {
        let position_address = SlaveAddress::SlaveNumber(slave.position_address);
        let mut dl_control = self.iface.read_dl_control(position_address)?;
        dl_control.set_enable_alias_address(enable);
        self.iface
            .write_dl_control(position_address, Some(dl_control))?;
        let alias = self.station_alias(slave)?;
        slave.configured_address = alias;
        Ok(())
    }

    pub fn set_station_address(
        &mut self,
        slave: &mut Slave,
        address: u16,
    ) -> Result<(), InitError> {
        let position_address = SlaveAddress::SlaveNumber(slave.position_address);
        let mut fixed_st = self.iface.read_fixed_station_address(position_address)?;
        fixed_st.set_configured_station_address(address);
        self.iface
            .write_fixed_station_address(position_address, Some(fixed_st))?;
        slave.configured_address = address;
        Ok(())
    }

    // TODO：もっと分解する
    fn init_slave(&mut self, slave_number: u16) -> Result<Option<Slave>, InitError> {
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
        self.iface
            .write_dl_control(SlaveAddress::SlaveNumber(slave_number), Some(dl_control))?;

        // INIT状態にする
        // 一応ループポートの設定の後にしている。
        let mut al_transfer = ALStateTransfer::new(self.iface, self.timer);
        al_transfer.change_al_state(SlaveAddress::SlaveNumber(slave_number), AlState::Init)?;
        slave.al_state = AlState::Init;

        // エラーカウンタをリセットする。
        self.iface
            .write_rx_error_counter(SlaveAddress::SlaveNumber(slave_number), None)?;

        // Watch dogの基本インクリメント値にデフォルト値を設定する
        let mut watchdog_div = WatchDogDivider::new();
        watchdog_div.set_watch_dog_divider(2498); //100us(default)
        self.iface
            .write_watch_dog_divider(SlaveAddress::SlaveNumber(slave_number), Some(watchdog_div))?;

        // データリンクWatchdogにデフォルト値を設定する。
        let mut dl_watchdog = DLUserWatchDog::new();
        dl_watchdog.set_dls_user_watch_dog(1000); //defalut 100ms
        self.iface
            .write_dl_user_watch_dog(SlaveAddress::SlaveNumber(slave_number), Some(dl_watchdog))?;

        // シンクマネージャーWatchdogにデフォルト値を設定する。
        let mut sm_watchdog = SyncManagerChannelWatchDog::new();
        sm_watchdog.set_sm_channel_watch_dog(1000); //defalut 100ms
        self.iface
            .write_sm_watch_dog(SlaveAddress::SlaveNumber(slave_number), Some(sm_watchdog))?;

        // スレーブでEEPROMが正常にロードされたか確認する。
        self.timer
            .start(MillisDurationU32::from_ticks(200).convert());
        loop {
            let is_pdi_operational = self
                .iface
                .read_dl_status(SlaveAddress::SlaveNumber(slave_number))?
                .pdi_operational();
            if is_pdi_operational {
                break;
            }
            match self.timer.wait() {
                Ok(_) => return Err(InitError::FailedToLoadEEPROM),
                Err(nb::Error::Other(_)) => return Err(InitError::Common(CommonError::UnspcifiedTimerError)),
                Err(nb::Error::WouldBlock) => (),
            }
        }

        // ステーションアドレスを設定する。
        self.set_station_address(&mut slave, slave_number)?;

        // dlインフォの入手。各種サポート状況の確認
        let dl_info = self
            .iface
            .read_dl_information(SlaveAddress::SlaveNumber(slave_number))?;
        slave.support_dc = dl_info.dc_supported();
        slave.is_dc_range_64bits = dl_info.dc_range();
        slave.support_fmmu_bit_operation = !dl_info.fmmu_bit_operation_not_supported();
        slave.support_lrw = !dl_info.not_lrw_supported(); //これが無いと事実上プロセスデータに対応しない。
        slave.support_rw = !dl_info.not_bafrw_supported(); //これが無いと事実上DCに対応しない。
        slave.ram_size_kb = dl_info.ram_size();
        //fmmuの確認
        //2個はないと入出力のどちらかしかできないはず。
        let number_of_fmmu = dl_info.number_of_supported_fmmu_entities();
        if number_of_fmmu >= 1 {
            slave.fmmu0 = Some(0x0600);
            // FMMUの設定をクリア
            self.iface
                .write_fmmu0(SlaveAddress::SlaveNumber(slave_number), None)?;
        }
        if number_of_fmmu >= 2 {
            slave.fmmu1 = Some(0x0610);
            //FMMUの設定をクリア
            self.iface
                .write_fmmu1(SlaveAddress::SlaveNumber(slave_number), None)?;
        }
        slave.number_of_sm = dl_info.number_of_supported_sm_channels(); //後で使う

        // ポートの設定
        let dl_status = self
            .iface
            .read_dl_status(SlaveAddress::SlaveNumber(slave_number))?;
        if dl_status.signal_detection_port0() {
            slave.ports[0] = dl_info.port0_type();
        }
        if dl_status.signal_detection_port1() {
            slave.ports[1] = dl_info.port1_type();
        }
        if dl_status.signal_detection_port2() {
            slave.ports[2] = dl_info.port2_type();
        }
        if dl_status.signal_detection_port3() {
            slave.ports[3] = dl_info.port3_type();
        }

        //ベンダーIDとかの設定
        let mut sii = SlaveInformationInterface::new(&mut self.iface);
        let (vender_id, _size) = sii.read(
            SlaveAddress::SlaveNumber(slave_number),
            sii_reg::VenderID::ADDRESS,
        )?;
        slave.id.vender_id = vender_id.sii_data() as u16;
        let (product_code, _size) = sii.read(
            SlaveAddress::SlaveNumber(slave_number),
            sii_reg::ProductCode::ADDRESS,
        )?;
        slave.id.product_code = product_code.sii_data() as u16;
        let (revision_number, _size) = sii.read(
            SlaveAddress::SlaveNumber(slave_number),
            sii_reg::RevisionNumber::ADDRESS,
        )?;
        slave.id.revision_number = revision_number.sii_data() as u16;

        //シンクマネージャーのサイズとかオフセット
        // Sync Managerの設定をクリア
        if slave.number_of_sm >= 1 {
            self.iface
                .write_sm0(SlaveAddress::SlaveNumber(slave_number), None)?;
        }
        if slave.number_of_sm >= 2 {
            self.iface
                .write_sm1(SlaveAddress::SlaveNumber(slave_number), None)?;
        }
        if slave.number_of_sm >= 3 {
            self.iface
                .write_sm2(SlaveAddress::SlaveNumber(slave_number), None)?;
        }
        if slave.number_of_sm >= 4 {
            self.iface
                .write_sm3(SlaveAddress::SlaveNumber(slave_number), None)?;
        }
        //まずは、メールボックスを使うプロトコルに対応しているか？
        let (mailbox_protocol, _size) = sii.read(
            SlaveAddress::SlaveNumber(slave_number),
            sii_reg::MailboxProtocol::ADDRESS,
        )?;
        slave.has_coe = mailbox_protocol.0[0].get_bit(2);
        slave.has_foe = mailbox_protocol.0[0].get_bit(3);
        // COEに対応するならメールボックス用のシンクマネージャーがあるはず・・・
        if slave.has_coe {
            assert!(slave.number_of_sm >= 2);
            let (sm_rx_offset, _size) = sii.read(
                SlaveAddress::SlaveNumber(slave_number),
                sii_reg::StandardRxMailboxOffset::ADDRESS,
            )?;
            let (sm_rx_size, _size) = sii.read(
                SlaveAddress::SlaveNumber(slave_number),
                sii_reg::StandardRxMailboxSize::ADDRESS,
            )?;
            slave.sm_mailbox_in = Some(MailboxSyncManager {
                size: sm_rx_size.sii_data() as u16,
                start_address: sm_rx_offset.sii_data() as u16,
            });
            let (sm_tx_offset, _size) = sii.read(
                SlaveAddress::SlaveNumber(slave_number),
                sii_reg::StandardTxMailboxOffset::ADDRESS,
            )?;
            let (sm_tx_size, _size) = sii.read(
                SlaveAddress::SlaveNumber(slave_number),
                sii_reg::StandardTxMailboxSize::ADDRESS,
            )?;
            slave.sm_mailbox_out = Some(MailboxSyncManager {
                size: sm_tx_size.sii_data() as u16,
                start_address: sm_tx_offset.sii_data() as u16,
            });
        }
        // FOEに対応するなら、ブートストラップ用のシンクマネージャーがあるはず・・・
        if slave.has_foe {
            assert!(slave.number_of_sm >= 2);
            let (bootstrap_sm_rx_offset, _size) = sii.read(
                SlaveAddress::SlaveNumber(slave_number),
                sii_reg::BootstrapRxMailboxOffset::ADDRESS,
            )?;
            let (bootstrap_sm_rx_size, _size) = sii.read(
                SlaveAddress::SlaveNumber(slave_number),
                sii_reg::BootstrapRxMailboxSize::ADDRESS,
            )?;
            slave.bootstrap_sm_mailbox_in = Some(MailboxSyncManager {
                size: bootstrap_sm_rx_size.sii_data() as u16,
                start_address: bootstrap_sm_rx_offset.sii_data() as u16,
            });
            let (bootstrap_sm_tx_offset, _size) = sii.read(
                SlaveAddress::SlaveNumber(slave_number),
                sii_reg::BootstrapTxMailboxOffset::ADDRESS,
            )?;
            let (bootstrap_sm_tx_size, _size) = sii.read(
                SlaveAddress::SlaveNumber(slave_number),
                sii_reg::BootstrapTxMailboxSize::ADDRESS,
            )?;
            slave.bootstrap_sm_mailbox_out = Some(MailboxSyncManager {
                size: bootstrap_sm_tx_size.sii_data() as u16,
                start_address: bootstrap_sm_tx_offset.sii_data() as u16,
            });
        }

        //プロセスデータ用のスタートアドレスを決める。
        //ただしプロセスデータに対応しているとは限らない。
        //NOTE: COEを前提とする。
        if slave.number_of_sm >= 3 && slave.has_coe {
            let sm_address0 = slave.sm_mailbox_in.unwrap().start_address;
            let sm_size0 = slave.sm_mailbox_in.unwrap().size;
            let sm_address1 = slave.sm_mailbox_out.unwrap().start_address;
            let sm_size1 = slave.sm_mailbox_out.unwrap().size;
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

        //メールボックス用シンクマネージャーの設定
        if let Some(sm_in) = slave.sm_mailbox_in {
            let mut sm = SyncManagerRegister::new();
            sm.set_physical_start_address(sm_in.start_address);
            sm.set_length(sm_in.size);
            sm.set_buffer_type(0b10); //mailbox
            sm.set_direction(1); //slave read access
            sm.set_dls_user_event_enable(true);
            sm.set_watchdog_enable(true);
            sm.set_channel_enable(true);
            sm.set_repeat(false);
            sm.set_dc_event_w_bus_w(false);
            sm.set_dc_event_w_loc_w(false);
        }
        if let Some(sm_out) = slave.sm_mailbox_out {
            let mut sm = SyncManagerRegister::new();
            sm.set_physical_start_address(sm_out.start_address);
            sm.set_length(sm_out.size);
            sm.set_buffer_type(0b10); //mailbox
            sm.set_direction(0); //slave write access
            sm.set_dls_user_event_enable(true);
            sm.set_watchdog_enable(true);
            sm.set_channel_enable(true);
            sm.set_repeat(false);
            sm.set_dc_event_w_bus_w(false);
            sm.set_dc_event_w_loc_w(false);
        }

        //DC周りの初期化
        if slave.support_dc {
            self.iface
                .write_dc_activation(SlaveAddress::SlaveNumber(slave_number), None)?;
            self.iface
                .write_sync0_cycle_time(SlaveAddress::SlaveNumber(slave_number), None)?;
            self.iface
                .write_sync1_cycle_time(SlaveAddress::SlaveNumber(slave_number), None)?;
            self.iface
                .write_cyclic_operation_start_time(SlaveAddress::SlaveNumber(slave_number), None)?;
            self.iface
                .write_latch0_negative_edge_value(SlaveAddress::SlaveNumber(slave_number), None)?;
            self.iface
                .write_latch0_positive_edge_value(SlaveAddress::SlaveNumber(slave_number), None)?;
            self.iface
                .write_latch1_negative_edge_value(SlaveAddress::SlaveNumber(slave_number), None)?;
            self.iface
                .write_latch1_positive_edge_value(SlaveAddress::SlaveNumber(slave_number), None)?;
            self.iface
                .write_latch_edge(SlaveAddress::SlaveNumber(slave_number), None)?;
            self.iface
                .write_latch_event(SlaveAddress::SlaveNumber(slave_number), None)?;
        }

        Ok(Some(slave))
    }
}
