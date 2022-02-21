use crate::al_state::*;
use crate::arch::*;
use crate::dc::config_dc;
use crate::error::*;
use crate::frame::ethercat::*;
use crate::frame::ethercat_frame::*;
use crate::sdo::*;
use crate::slave_device::*;
use crate::util::*;
use bit_field::BitArray;
use core::marker::PhantomData;
use heapless;
use log::*;

const FRAME_MAX_SIZE_WITHOUT_FCS: usize = 1500;

//設定項目
pub const MAX_SLAVES: usize = 16;

const MAX_RXPDO_BUFFER_SIZE: usize = 10;
pub(crate)const SM0_START_ADDRESS: u16 = 0x1800; //Mailbox(リクエスト)の先頭アドレス0x1000～0x2FFFF
pub(crate)const SM1_START_ADDRESS: u16 = 0x1C00; //Mailbox(レスポンス)の先頭アドレス0x1000～0x2FFFF
pub(crate)const SM2_START_ADDRESS: u16 = 0x1100; //Rxプロセスデータの先頭アドレス0x1000～0x2FFFF
pub(crate)const SM3_START_ADDRESS: u16 = 0x1140; //Txプロセスデータの先頭アドレス0x1000～0x2FFFF
const RXPDO_MAPPING_INDEX: u16 = 0x1702; //PDOのマッピング設定をするオブジェクトディクショナリのインデッ                                         //0x1601～0x1603, 0x1700～0x1703
const TXPDO_MAPPING_INDEX: u16 = 0x1B03; //PDOのマッピング設定をするオブジェクトディクショナリのインデッ                                         //0x1A01～0x1A03, 0x1B00～0x1B03
const LOGICAL_START_ADDRESS: u32 = 0x10000; //LRWの際に指定するアドレス

pub struct EtherCATMaster<R: RawPacketInterface, E: EtherCATSystemTime> {
    ethdev: R,
    packet: EtherCATFrame<[u8; FRAME_MAX_SIZE_WITHOUT_FCS]>,
    slave_count: u16,
    dc_delay_ns_from_ref_time: u64,
    slaves: heapless::Vec<SlaveDevice, MAX_SLAVES>,
    rx_pd0_buffer: [u8; MAX_RXPDO_BUFFER_SIZE],
    al_state: AlState,
    rx_error_count: u8,
    recieve_buffer: [u8; FRAME_MAX_SIZE_WITHOUT_FCS],
    _epoch: PhantomData<E>,
}

impl<R, E> EtherCATMaster<R, E>
where
    R: RawPacketInterface,
    E: EtherCATSystemTime,
{
    pub fn new(ethdev: R) -> Self {
        let buffer = [0; FRAME_MAX_SIZE_WITHOUT_FCS];

        let mut packet = EtherCATFrame::new(buffer).expect("too small or too big buffer size");
        packet.init();

        let recieve_buffer = [0; FRAME_MAX_SIZE_WITHOUT_FCS];
        Self {
            ethdev,
            packet,
            slave_count: 0,
            dc_delay_ns_from_ref_time: 0,
            slaves: heapless::Vec::default(),
            rx_pd0_buffer: [0; MAX_RXPDO_BUFFER_SIZE],
            al_state: AlState::Init,
            rx_error_count: 0,
            recieve_buffer,
            _epoch: PhantomData,
        }
    }

    pub fn slaves<'a>(&'a self) -> &'a [SlaveDevice] {
        &self.slaves
    }

    pub fn slave_count(&self) -> u16 {
        self.slave_count
    }

    pub fn slave_mut<'a>(&'a mut self, slave_number: usize) -> Option<&'a mut SlaveDevice> {
        self.slaves.get_mut(slave_number)
    }

    fn scan_slaves(&mut self) -> Result<(), Error> {
        info!("scanning slaves");

        let slave_count = slave_count::<_, _, E>(
            &mut self.ethdev,
            &mut self.packet,
            &mut self.recieve_buffer,
            1000_000_000,
        )?;
        info!("slave count: {}", slave_count);
        if slave_count == 0 {
            return Err(Error::NotFoundSlaves);
        }
        if slave_count as usize > MAX_SLAVES {
            return Err(Error::TooManySlave(slave_count as usize));
        }
        self.slave_count = slave_count;
        self.slaves = (0..slave_count)
            .map(|i| SlaveDevice::new(i as u16))
            .collect();

        self.clear_rx_error_count()?;

        self.set_loop_port_config()?;

        self.check_eeprom_operation()?;
        self.change_al_states(AlState::Init)?;

        self.check_esc_info()?;
        self.check_dl_topology()?;

        Ok(())
    }

    pub fn init_slaves(&mut self) -> Result<(), Error> {
        //
        // Init State
        //
        self.scan_slaves()?;

        self.change_al_states(AlState::Init)?;
        info!("init state");

        //諸々のアドレスクリア
        self.clear_fmmu()?;
        self.clear_sync_manager()?;
        info!("address clear");

        //DCの設定
        self.config_dc()?;
        info!("dc init");

        //PreOperationalに移行するために必要な設定
        self.set_station_address()?;
        self.configure_mailbox_sm()?;

        //
        // Pre Operational State
        //
        self.change_al_states(AlState::PreOperational)?;

        info!("pre operational state");

        Ok(())
    }

    pub fn start_safe_operation(&mut self, cycle_time_ns: u32) -> Result<u64, Error> {
        let sync0_cycle = cycle_time_ns;
        let sync1_cycle = sync0_cycle / 2;
        let count = self.slave_count;
        for num in 0..count {
            self.write_sdo(num, 0x1C12, 0, &[0])?; //一度サブインデックス0をクリアする必要がある。
            self.write_sdo(num, 0x1C12, 0x01, &u16::to_le_bytes(RXPDO_MAPPING_INDEX))?; //SM2 RxPDO アサイン
            self.write_sdo(num, 0x1C12, 0, &[1])?; //SM2 RxPDO エントリー
                                                   //let d = self.read_sdo(num, 0x1C12, 0)?;
                                                   //dbg!(d);
                                                   //let d = self.read_sdo(num, 0x1C12, 1)?;
                                                   //dbg!(d);

            self.write_sdo(num, 0x1C13, 0, &[0])?; //一度サブインデックス0をクリアする必要がある。
            self.write_sdo(num, 0x1C13, 1, &u16::to_le_bytes(TXPDO_MAPPING_INDEX))?; //SM4 TxPDO アサイン
            self.write_sdo(num, 0x1C13, 0, &[1])?; //SM3 TxPDO エントリー
                                                   //let d = self.read_sdo(num, 0x1C13, 0)?;
                                                   //dbg!(d);
                                                   //let d = self.read_sdo(num, 0x1C13, 1)?;
                                                   //dbg!(d);
        }

        self.pd0_mapping()?;

        //PreOptionalの初期化

        //同期モードの設定
        for num in 0..count {
            self.write_sdo(num, 0x1C32, 1, &[0x03])?; //sm2をSYNC1信号同期モードにする
            self.write_sdo(num, 0x1C33, 1, &[0x03])?; //sm3をSYNC1信号同期モードにする
            self.write_sdo(num, 0x1C32, 2, &u32::to_le_bytes(sync0_cycle))?; //サイクルタイム設定
        }

        //SYNC0信号とSYNC1信号のサイクルタイム設定
        let sync0_register = 0x09A0;
        let sync1_register = 0x09A4;
        self.add_bwr(sync0_register, &u32::to_le_bytes(sync0_cycle))?;
        self.add_bwr(sync1_register, &u32::to_le_bytes(sync1_cycle))?;
        self.send_packet_with_error_read()?;
        self.recieve_packet_with_error_check(1000_000_000)?;

        //SYNC信号の開始
        let sync0_pulse_start_time = (E::system_time_from_2000_1_1_as_nanos() + 10_000_000) as u64;
        let sync0_start_time_register = 0x0990;
        self.add_bwr(
            sync0_start_time_register,
            &u64::to_le_bytes(sync0_pulse_start_time),
        )?;
        self.send_packet_with_error_read()?;
        self.recieve_packet_with_error_check(1000_000_000)?;

        //サイクル許可
        let cycle_control_register = 0x0980;
        self.add_bwr(cycle_control_register, &[0, 0x07])?;
        self.send_packet_with_error_read()?;
        self.recieve_packet_with_error_check(1000_000_000)?;
        self.wait_sync_0_starting(2_000_000_000)?; //SYNC信号が始まったか確認

        //PDO SMとFMMUの設定
        self.configure_pd0_sm()?;
        self.configure_fmmu()?;

        //ウォッチドッグ無効
        //NOTE:SMの設定でウォッチドッグがディセーブルの場合、0x0420を0にする必要がある。
        let watch_dog_time_process_register = 0x0420;
        self.add_bwr(watch_dog_time_process_register, &[0, 0])?;
        self.send_packet_with_error_read()?;
        self.recieve_packet_with_error_check(1000_000_000)?;

        //
        // Safe Operational State
        //
        self.change_al_states(AlState::SafeOperational)?;
        info!("safe operational");

        Ok(sync0_pulse_start_time)
    }

    // TODO: 送信と受信に分ける
    pub fn send_pdo(&mut self, timeout_ns: u64) -> Result<u64, Error> {
        let mut bitsum = 0;
        let mut bitsum_tx = 0;
        for slave in &self.slaves {
            let mut bitsum_per_slave = 0;
            let offset = slave.rx_pd0_start_offset();
            let start_bit = slave.rx_pd0_start_bit();
            for pd0_entry in slave.rx_pd0_mapping() {
                let bitsize = pd0_entry.bit_length() as usize;
                let grobal_start_bit = offset * 8 + start_bit + bitsum_per_slave;

                let iter_num = if bitsize % 8 == 0 {
                    bitsize / 8
                } else {
                    bitsize / 8 + 1
                };
                for i in 0..iter_num {
                    let range = i * 8..(bitsize - (iter_num - 1 - i) * 8);
                    let data = pd0_entry.data().get_bits(range.clone());
                    self.rx_pd0_buffer.set_bits(
                        grobal_start_bit + range.start..grobal_start_bit + range.end,
                        data,
                    );
                }

                bitsum += bitsize;
                bitsum_per_slave += bitsize;
            }
            for pd0_entry_tx in slave.tx_pd0_mapping() {
                bitsum_tx += pd0_entry_tx.bit_length() as usize;
            }
        }
        let max_bitsum = bitsum_tx + bitsum;
        let length = if max_bitsum % 8 == 0 {
            max_bitsum / 8
        } else {
            max_bitsum / 8 + 1
        }; //TODO:毎回求める必要がない

        self.packet
            .add_lrw(LOGICAL_START_ADDRESS, &self.rx_pd0_buffer[..length])?;
        self.packet.add_armw(0, 0x0910, &[0; 8])?;
        self.add_brd(0x0130, &[0; 2])?;
        self.send_packet_with_error_read()?;
        self.recieve_packet_with_error_check(timeout_ns)?;
        let res_packet = EtherCATFrame::new(self.recieve_buffer)?;
        let mut predict_sys_time = 0;
        for (i, payload_offset) in res_packet.dlpd0u_payload_offsets().enumerate() {
            //まずプロセスデータを処理
            if i == 0 {
                for slave in self.slaves.iter_mut() {
                    let offset = slave.tx_pd0_start_offset();
                    let start_bit = slave.tx_pd0_start_bit();
                    let grobal_start_bit = offset * 8 + start_bit; // + bitsum;
                    let mut bitsum = 0;
                    for pd0_entry in slave.tx_pd0_mapping_mut() {
                        let bitsize = pd0_entry.bit_length() as usize;
                        let iter_num = if bitsize % 8 == 0 {
                            bitsize / 8
                        } else {
                            bitsize / 8 + 1
                        };
                        for j in 0..iter_num {
                            let range = j * 8..(bitsize - (iter_num - 1 - j) * 8);
                            //let data = pd0_entry.data.get_bits(range.clone());
                            let byte = res_packet.packet()[payload_offset..].get_bits(
                                grobal_start_bit + range.start + bitsum
                                    ..grobal_start_bit + range.end + bitsum,
                            );
                            //dbg!(byte);
                            pd0_entry.data_mut().set_bits(range.clone(), byte);
                            //dbg!(pd0_entry.data);
                        }
                        bitsum += bitsize;
                    }
                }
            }
            //次に基準時間を確認する。
            else if i == 1 {
                //TODO: もっといい書き方がある
                let ref_time = u64::from_le_bytes([
                    res_packet.packet()[payload_offset],
                    res_packet.packet()[payload_offset + 1],
                    res_packet.packet()[payload_offset + 2],
                    res_packet.packet()[payload_offset + 3],
                    res_packet.packet()[payload_offset + 4],
                    res_packet.packet()[payload_offset + 5],
                    res_packet.packet()[payload_offset + 6],
                    res_packet.packet()[payload_offset + 7],
                ]);
                predict_sys_time = ref_time + self.dc_delay_ns_from_ref_time;
            }
            //最後にALStateを確認する。
            else if i == 2 {
                let alstate = AlState::from(u16::from_le_bytes([
                    res_packet.packet()[payload_offset],
                    res_packet.packet()[payload_offset + 1],
                ]));
                if alstate != self.al_state {
                    return Err(Error::UnexpectedAlState(self.al_state, alstate));
                }
            }
        }
        Ok(predict_sys_time)
    }

    pub fn write_sdo(
        &mut self,
        slave_number: u16,
        index: u16,
        sub_index: u8,
        data: &[u8],
    ) -> Result<(), Error> {
        let station_address = self.slaves[slave_number as usize].number();
        let mailbox_count = self.slaves[slave_number as usize].mailbox_count();
        self.slaves[slave_number as usize].increment_mailbox_count();
        write_sdo::<_, _, E>(
            &mut self.ethdev,
            &mut self.packet,
            &mut self.recieve_buffer,
            slave_number,
            station_address,
            data,
            mailbox_count,
            100,
            index,
            sub_index,
        )?;
        Ok(())
    }

    pub fn read_sdo(
        &mut self,
        slave_number: u16,
        index: u16,
        sub_index: u8,
    ) -> Result<heapless::Vec<u8, SDO_MAX_DATA_LENGTH>, Error> {
        let station_address = self.slaves[slave_number as usize].number();
        let mailbox_count = self.slaves[slave_number as usize].mailbox_count();
        self.slaves[slave_number as usize].increment_mailbox_count();
        read_sdo::<_, _, E>(
            &mut self.ethdev,
            &mut self.packet,
            &mut self.recieve_buffer,
            slave_number,
            station_address,
            mailbox_count,
            100,
            index,
            sub_index,
        )
    }

    fn send_packet(&mut self) -> Result<(), Error> {
        send_ec_packet(&mut self.ethdev, &mut self.packet)?;
        init_ec_packet(&mut self.packet);
        Ok(())
    }

    fn receive_packet(&mut self, timeout_ns: u64) -> Result<(), Error> {
        clear_buffer(&mut self.recieve_buffer);
        receive_packet_with_wkc_check::<_, E>(
            &mut self.ethdev,
            &mut self.recieve_buffer,
            self.slave_count,
            timeout_ns,
        )?;
        Ok(())
    }

    fn send_packet_with_error_read(&mut self) -> Result<(), Error> {
        let register = 0x0300;
        self.add_brd(register, &[0])?;
        self.send_packet()
    }

    fn recieve_packet_with_error_check(&mut self, timeout_ns: u64) -> Result<(), Error> {
        let rx_error_count = self.rx_error_count;
        let _ = self.receive_packet(timeout_ns)?;
        let recieve_packet = EtherCATFrame::new(self.recieve_buffer)?;
        if let Some(offset) = recieve_packet.dlpd0u_header_offsets().last() {
            let rxerror =
                EtherCATPDU::new(&self.recieve_buffer[offset..]).ok_or(Error::SmallBuffer)?;
            if rxerror.ado() != 0x0300 {
                return Err(Error::UnexpectedPacket);
            }
            let error = self
                .recieve_buffer
                .get(offset + EtherCATPDU_HEADER_LENGTH)
                .ok_or(Error::SmallBuffer)?;
            if *error != rx_error_count {
                return Err(Error::RxError(*error));
            }
        }

        Ok(())
    }

    // スレーブの基本情報を確認する。
    fn check_esc_info(&mut self) -> Result<(), Error> {
        let esc_info_register = 0x0000;
        self.add_aprd_all_slave(esc_info_register, &[0; 10])?;
        self.send_packet_with_error_read()?;
        self.recieve_packet_with_error_check(1000_000_000)?;
        let res = EtherCATFrame::new(self.recieve_buffer)?;
        for (payload_offset, slave) in res.dlpd0u_payload_offsets().zip(self.slaves.iter_mut()) {
            //slave.num_supported_fmmu = *res
            //    .packet()
            //    .get(payload_offset + 5)
            //    .ok_or(Error::SmallBuffer)?;
            //slave.num_supported_sm = *res
            //    .packet()
            //    .get(payload_offset + 6)
            //    .ok_or(Error::SmallBuffer)?;
            //slave.ram_size = *res
            //    .packet()
            //    .get(payload_offset + 7)
            //    .ok_or(Error::SmallBuffer)?;

            //NOTE: port0とport1がMIIでport3,port4は未使用であること
            let ports = res
                .packet()
                .get(payload_offset + 7)
                .ok_or(Error::SmallBuffer)?;
            if (ports & 0b0000_0011) != 0b11 {
                return Err(Error::UnimplementedFeature);
            }
            if (ports & 0b0000_1100) >> 2 != 0b11 {
                return Err(Error::UnimplementedFeature);
            }
            if (ports & 0b0011_0000) != 0b0 {
                return Err(Error::UnimplementedFeature);
            }
            if (ports & 0b1100_0000) != 0b0 {
                return Err(Error::UnimplementedFeature);
            }
            //NOTE: DCに対応していること
            let has_dc = (res
                .packet()
                .get(payload_offset + 8)
                .ok_or(Error::SmallBuffer)?
                & 0b100)
                != 0;
            if !has_dc {
                return Err(Error::UnimplementedFeature);
            }
            //NOTE: LRWコマンドが使えること
            let support_lrw = (res
                .packet()
                .get(payload_offset + 9)
                .ok_or(Error::SmallBuffer)?
                & 0b10)
                == 0;
            if !support_lrw {
                return Err(Error::UnimplementedFeature);
            }
            //FMMUの可変構成に対応すること
            let fixed_fmmu = (res
                .packet()
                .get(payload_offset + 9)
                .ok_or(Error::SmallBuffer)?
                & 0b1000)
                != 0;
            if fixed_fmmu {
                return Err(Error::UnimplementedFeature);
            }
        }
        Ok(())
    }

    // ネットワークトポロジーを確認する。
    fn check_dl_topology(&mut self) -> Result<(), Error> {
        let slave_count = self.slave_count as usize;
        let dl_status_register = 0x0110;
        self.add_aprd_all_slave(dl_status_register, &[0; 2])?;
        self.send_packet_with_error_read()?;
        self.recieve_packet_with_error_check(1000_000_000)?;
        let res = EtherCATFrame::new(self.recieve_buffer)?;
        for (i, payload_offset) in res.dlpd0u_payload_offsets().enumerate() {
            if i + 1 > slave_count {
                break;
            }
            let is_port0_active = (res
                .packet()
                .get(payload_offset + 1)
                .ok_or(Error::SmallBuffer)?
                & 0b1_0000)
                != 0;
            let is_port1_active = (res
                .packet()
                .get(payload_offset + 1)
                .ok_or(Error::SmallBuffer)?
                & 0b10_0000)
                != 0;
            let is_port2_active = (res
                .packet()
                .get(payload_offset + 1)
                .ok_or(Error::SmallBuffer)?
                & 0b100_0000)
                != 0;
            let is_port3_active = (res
                .packet()
                .get(payload_offset + 1)
                .ok_or(Error::SmallBuffer)?
                & 0b1000_0000)
                != 0;
            //NOTE: ネットワークトポロジーは、port0をin、port1をoutとしたラインであること。
            if (i + 1 != slave_count)
                & is_port0_active
                & is_port1_active
                & !is_port2_active
                & !is_port3_active
            {
                return Err(Error::UnimplementedFeature);
            } else if is_port0_active & !is_port1_active & !is_port2_active & !is_port3_active {
                return Err(Error::UnimplementedFeature);
            }
        }
        Ok(())
    }

    // ステーションアドレスをセットする。
    // 接続順に0からインクリメントするだけ。
    fn set_station_address(&mut self) -> Result<(), Error> {
        let register = 0x0010;
        for i in 0..self.slave_count {
            self.packet.add_apwr(
                get_ap_adp(i),
                register,
                &u16::to_le_bytes(self.slaves[i as usize].number()),
            )?;
        }
        Ok(())
    }

    // 以下のように、ループポートの設定をする。
    // ・EtherCATフレーム以外は破棄する。
    // ・ループ設定はスレーブで自動検出する。
    fn set_loop_port_config(&mut self) -> Result<(), Error> {
        let register = 0x0100;
        //kill non etherCAT frames and auto loop.
        let data = [0x01, 0x00, 0x07, 0x00];
        self.add_bwr(register, &data)?;
        self.send_packet()?;
        self.receive_packet(1000_000_000)?;
        Ok(())
    }

    /// 転送エラーカウントをクリアする。
    pub fn clear_rx_error_count(&mut self) -> Result<(), Error> {
        let register = 0x0300;
        self.add_apwr_all_slave(register, &[0; 8])?;
        self.send_packet()?;
        self.receive_packet(1000_000_000)?;
        Ok(())
    }

    // EEPROMが動作しているか確認する。
    fn check_eeprom_operation(&mut self) -> Result<(), Error> {
        let slave_count = self.slave_count;

        let register = 0x0110;
        self.add_aprd_all_slave(register, &[0])?;
        self.send_packet_with_error_read()?;
        self.recieve_packet_with_error_check(1000_000_000)?;
        let res = EtherCATFrame::new(self.recieve_buffer)?;
        let mut is_operation = true;
        for (i, offset) in res.dlpd0u_payload_offsets().enumerate() {
            if (i + 1) as u16 > slave_count {
                break;
            }
            let data = *res.packet().get(offset).ok_or(Error::SmallBuffer)?;
            is_operation &= (data & 1) != 0;
        }
        if !is_operation {
            Err(Error::EEPROMNotOperation)
        } else {
            Ok(())
        }
    }

    pub fn change_al_states(&mut self, state: AlState) -> Result<(), Error> {
        let slave_numbers: heapless::Vec<_, MAX_SLAVES> = (0..self.slave_count).collect();
        change_al_state::<_, _, E>(
            &mut self.ethdev,
            &mut self.packet,
            &mut self.recieve_buffer,
            &slave_numbers,
            state,
            1000_000_000,
        )?;
        self.al_state = state;
        Ok(())
    }

    fn wait_sync_0_starting(&mut self, timeout_ns: u64) -> Result<(), Error> {
        let start_time = E::system_time_from_2000_1_1_as_nanos();
        while !self.is_sync0_starting()? {
            if E::system_time_from_2000_1_1_as_nanos() - start_time >= timeout_ns {
                return Err(Error::Sync0Timeout(timeout_ns));
            }
        }
        Ok(())
    }

    fn is_sync0_starting(&mut self) -> Result<bool, Error> {
        let slave_count = self.slave_count as usize;
        let sync0_start_time_register = 0x0990;
        self.add_aprd_all_slave(sync0_start_time_register, &[0; 8])?;
        self.send_packet_with_error_read()?;
        self.recieve_packet_with_error_check(1000_000_000)?;
        let recieve_packet = EtherCATFrame::new(self.recieve_buffer)?;
        let mut max_time = 0;
        for (i, payload_offset) in recieve_packet.dlpd0u_payload_offsets().enumerate() {
            if i + 1 > slave_count {
                break;
            }
            let mut data = [0; 8];
            for j in 0..8 {
                data[j] = *self
                    .recieve_buffer
                    .get(payload_offset + j)
                    .ok_or(Error::SmallBuffer)?;
            }
            max_time = max_time.max(u64::from_le_bytes(data));
        }

        let system_time_register = 0x0910;
        self.add_aprd_all_slave(system_time_register, &[0; 8])?;
        self.send_packet_with_error_read()?;
        self.recieve_packet_with_error_check(1000_000_000)?;
        let recieve_packet = EtherCATFrame::new(self.recieve_buffer)?;
        let mut system_min_time = 0;
        for (i, payload_offset) in recieve_packet.dlpd0u_payload_offsets().enumerate() {
            if i + 1 > slave_count {
                break;
            }
            let mut data = [0; 8];
            for j in 0..8 {
                data[j] = *self
                    .recieve_buffer
                    .get(payload_offset + j)
                    .ok_or(Error::SmallBuffer)?;
            }
            system_min_time = system_min_time.max(u64::from_le_bytes(data));
        }

        Ok(system_min_time > max_time)
    }

    fn clear_fmmu(&mut self) -> Result<(), Error> {
        let register = 0x0600;
        self.add_bwr(register, &[0; 128])?;
        self.send_packet_with_error_read()?;
        self.recieve_packet_with_error_check(1000_000_000)?;
        Ok(())
    }

    fn clear_sync_manager(&mut self) -> Result<(), Error> {
        let register = 0x0800;
        self.add_bwr(register, &[0; 64])?;
        self.send_packet_with_error_read()?;
        self.recieve_packet_with_error_check(1000_000_000)?;
        Ok(())
    }

    fn config_dc(&mut self) -> Result<(), Error> {
        config_dc::<_, _, E>(
            &mut self.ethdev,
            &mut self.packet,
            &mut self.recieve_buffer,
            self.slave_count,
            15000,
        )?;
        Ok(())
    }

    fn configure_mailbox_sm(&mut self) -> Result<(), Error> {
        let sm0_register = 0x0800;
        let sm1_register = 0x0808;
        //0x1800から512byte
        let adress = u16::to_le_bytes(SM0_START_ADDRESS);
        self.add_bwr(
            sm0_register,
            &[adress[0], adress[1], 0x00, 0x02, 0x26, 0x00, 0x01, 0x00],
        )?;

        //0x1C00から512byte
        let adress = u16::to_le_bytes(SM1_START_ADDRESS);
        self.add_bwr(
            sm1_register,
            &[adress[0], adress[1], 0x00, 0x02, 0x22, 0x00, 0x01, 0x00],
        )?;
        self.send_packet_with_error_read()?;
        self.recieve_packet_with_error_check(1000_000_000)?;
        //panic!();
        Ok(())
    }

    fn configure_pd0_sm(&mut self) -> Result<(), Error> {
        //TODO: PDOの長さは、2の倍数にする必要があるらしい。
        let sm2_register = 0x0810;
        let sm3_register = 0x0818;
        for (slave_number, slave) in self.slaves.iter().enumerate() {
            let rx_pd0_num = if slave.rx_pd0_bit_size() % 8 == 0 {
                slave.rx_pd0_bit_size() / 8
            } else {
                slave.rx_pd0_bit_size() / 8 + 1
            };
            //dbg!(rx_pd0_num);
            let tx_pd0_num = if slave.tx_pd0_bit_size() % 8 == 0 {
                slave.tx_pd0_bit_size() / 8
            } else {
                slave.tx_pd0_bit_size() / 8 + 1
            };
            //dbg!(tx_pd0_num);

            //0x1100から
            let address = u16::to_le_bytes(SM2_START_ADDRESS);
            self.packet.add_apwr(
                get_ap_adp(slave_number as u16),
                sm2_register,
                &[
                    address[0],
                    address[1],
                    (rx_pd0_num & 0xFF) as u8,
                    ((rx_pd0_num & 0xFF00) >> 8) as u8,
                    0x24,
                    0x00,
                    0x01,
                    0x00,
                ],
            )?;
            //0x1140から
            let address = u16::to_le_bytes(SM3_START_ADDRESS);
            self.packet.add_apwr(
                get_ap_adp(slave_number as u16),
                sm3_register,
                &[
                    address[0],
                    address[1],
                    (tx_pd0_num & 0xFF) as u8,
                    ((tx_pd0_num & 0xFF00) >> 8) as u8,
                    0x20,
                    0x00,
                    0x01,
                    0x00,
                ],
            )?;
        }

        self.send_packet_with_error_read()?;
        self.recieve_packet_with_error_check(1000_000_000)?;
        //panic!();
        Ok(())
    }

    fn configure_fmmu(&mut self) -> Result<(), Error> {
        let fmmu0_register = 0x0600;
        let fmmu1_register = 0x0610;
        let logical_address: u32 = LOGICAL_START_ADDRESS;
        let mut rx_bit_sum = 0;
        let mut tx_bit_sum = 0;
        for (slave_number, slave) in self.slaves.iter_mut().enumerate() {
            //slave.rx_pd0_start_offset_bit = rx_offset_bit;
            //slave.tx_pd0_start_offset_bit = tx_offset_bit;

            let rx_bit_size = slave.rx_pd0_bit_size();
            let rx_offset = rx_bit_sum / 8 + tx_bit_sum / 8;

            let rx_pd0_start_bit = rx_bit_sum % 8;
            let rx_pd0_length = if (rx_bit_size + rx_pd0_start_bit) % 8 != 0 {
                (rx_bit_size + rx_pd0_start_bit) / 8 + 1
            } else {
                (rx_bit_size + rx_pd0_start_bit) / 8
            };
            let rx_pd0_stop_bit = if (rx_bit_sum + rx_bit_size) % 8 == 0 {
                7
            } else {
                (rx_bit_sum + rx_bit_size) % 8 - 1
            };
            let mut fmmu0_data = [0; FMMU_LENGTH];
            let mut fmmu0 = FMMU::new(&mut fmmu0_data).unwrap();
            fmmu0.set_logical_start_address(logical_address + rx_offset as u32);
            fmmu0.set_length(rx_pd0_length);
            fmmu0.set_logical_start_bit(rx_pd0_start_bit as u8);
            fmmu0.set_logical_stop_bit(rx_pd0_stop_bit as u8);
            fmmu0.set_physical_start_address(SM2_START_ADDRESS);
            fmmu0.set_write_access(true);
            fmmu0.set_read_access(false);
            fmmu0.set_active(true);
            slave.set_rx_pd0_start_offset(rx_offset as usize);
            slave.set_rx_pd0_length(rx_pd0_length as usize);
            slave.set_rx_pd0_start_bit(rx_pd0_start_bit as usize);
            slave.set_rx_pd0_stop_bit(rx_pd0_stop_bit as usize);
            rx_bit_sum += rx_bit_size;

            let tx_bit_size = slave.tx_pd0_bit_size();
            let tx_offset = tx_bit_sum / 8 + rx_bit_sum / 8;
            let tx_pd0_start_bit = tx_bit_sum % 8;
            let tx_pd0_length = if (tx_bit_size + tx_pd0_start_bit) % 8 != 0 {
                (tx_bit_size + tx_pd0_start_bit) / 8 + 1
            } else {
                (tx_bit_size + tx_pd0_start_bit) / 8
            };
            let tx_pd0_stop_bit = if (tx_bit_sum + tx_bit_size) % 8 == 0 {
                7
            } else {
                (tx_bit_sum + tx_bit_size) % 8 - 1
            };
            let mut fmmu1_data = [0; FMMU_LENGTH];
            let mut fmmu1 = FMMU::new(&mut fmmu1_data).unwrap();
            fmmu1.set_logical_start_address(logical_address + tx_offset as u32);
            fmmu1.set_length(tx_pd0_length);
            fmmu1.set_logical_start_bit(tx_pd0_start_bit as u8);
            fmmu1.set_logical_stop_bit(tx_pd0_stop_bit as u8);
            fmmu1.set_physical_start_address(SM3_START_ADDRESS);
            fmmu1.set_read_access(true);
            fmmu1.set_write_access(false);
            fmmu1.set_active(true);
            slave.set_tx_pd0_start_offset(tx_offset as usize);
            slave.set_tx_pd0_length(tx_pd0_length as usize);
            slave.set_tx_pd0_start_bit(tx_pd0_start_bit as usize);
            slave.set_tx_pd0_stop_bit(tx_pd0_stop_bit as usize);

            self.packet
                .add_apwr(get_ap_adp(slave_number as u16), fmmu0_register, &fmmu0_data)?;
            self.packet
                .add_apwr(get_ap_adp(slave_number as u16), fmmu1_register, &fmmu1_data)?;
            tx_bit_sum += tx_bit_size;
        }

        self.send_packet_with_error_read()?;
        self.recieve_packet_with_error_check(1000_000_000)?;
        //panic!();

        Ok(())
    }

    fn pd0_mapping(&mut self) -> Result<(), Error> {
        let slave_count = self.slave_count as usize;
        const MAX_ENTRY: usize = if MAX_RXPDO_ENTRY > MAX_TXPDO_ENTRY {
            MAX_RXPDO_ENTRY
        } else {
            MAX_TXPDO_ENTRY
        };
        let mut data_buffer = [0_u32; MAX_ENTRY];

        for i in 0..slave_count {
            let num_rx_entry = self.slaves[i].rx_pd0_mapping().len();
            self.write_sdo(i as u16, RXPDO_MAPPING_INDEX, 0, &[0])?; //一度サブインデックス0をクリアすること
            for (j, entry) in self.slaves[i].rx_pd0_mapping().iter().enumerate() {
                let mut data: u32 = 0;
                data |= (entry.address() as u32) << 16;
                data |= entry.bit_length() as u32;
                data_buffer[j] = data;
            }
            for j in 0..num_rx_entry {
                self.write_sdo(
                    i as u16,
                    RXPDO_MAPPING_INDEX,
                    (j + 1) as u8,
                    &u32::to_le_bytes(data_buffer[j]),
                )?;
            }
            self.write_sdo(i as u16, RXPDO_MAPPING_INDEX, 0, &[num_rx_entry as u8])?;

            let num_tx_entry = self.slaves[i].tx_pd0_mapping().len();
            self.write_sdo(i as u16, TXPDO_MAPPING_INDEX, 0, &[0])?; //一度サブインデックス0をクリアすること
            for (j, entry) in self.slaves[i].tx_pd0_mapping().iter().enumerate() {
                let mut data: u32 = 0;
                data |= (entry.address() as u32) << 16;
                data |= entry.bit_length() as u32;
                data_buffer[j] = data;
            }
            for j in 0..num_tx_entry {
                self.write_sdo(
                    i as u16,
                    TXPDO_MAPPING_INDEX,
                    (j + 1) as u8,
                    &u32::to_le_bytes(data_buffer[j]),
                )?;
            }
            self.write_sdo(i as u16, TXPDO_MAPPING_INDEX, 0, &[num_tx_entry as u8])?;
        }
        Ok(())
    }

    fn add_aprd_all_slave(&mut self, register: u16, data: &[u8]) -> Result<(), Error> {
        self.packet
            .add_aprd_all_slave(register, data, self.slave_count)?;
        Ok(())
    }

    fn add_apwr_all_slave(&mut self, register: u16, data: &[u8]) -> Result<(), Error> {
        self.packet
            .add_apwr_all_slave(register, data, self.slave_count)?;
        Ok(())
    }

    fn add_brd(&mut self, register: u16, data: &[u8]) -> Result<(), Error> {
        self.packet.add_brd(0, register, data)?;
        Ok(())
    }

    fn add_bwr(&mut self, register: u16, data: &[u8]) -> Result<(), Error> {
        self.packet.add_bwr(0, register, data)?;
        Ok(())
    }
}
