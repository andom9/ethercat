    // EEPROMからベンダー情報とプロダクトコードを確認する。
    // TODO: できてない
//    fn check_eeprom_info(&mut self) -> Result<(), EtherCATError> {
//        let slave_count = self.slave_count as usize;
//        //EEPROMのアクセス権をマスターにする
//        let eeprom_owner_register = 0x0500;
//
//        //self.wait_eeprom_busy(100)?;
//        self.add_bwr(eeprom_owner_register, &[0b10])?;
//        self.send_packet_with_error_read()?;
//        self.recieve_packet_with_error_check(1000_000_000)?;
//
//        //EEPROMへアクセス可能かどうか確認
//        //let eeprom_access_state_register = 0x0501;
//        //self.wait_eeprom_busy(100)?;
//        //self.add_brd(eeprom_access_state_register, &[0])?;
//        //self.send_packet_with_error_read()?;
//        //let res = EtherCATPacketUtil::new(self.recieve_packet_with_error_check()?.to_owned())?;
//        //let payload_offset = res
//        //    .dlpdu_payload_offsets()
//        //    .next()
//        //    .ok_or(PacketError::SmallBuffer)?;
//        //let permit_eeprom_access = (res
//        //    .packet()
//        //    .get(payload_offset)
//        //    .ok_or(PacketError::SmallBuffer)?
//        //    & 1)
//        //    == 0;
//        //if !permit_eeprom_access {
//        //    return Err(EtherCATError::CannotAccessEEPROM);
//        //}
//
//        //EEPROMリードコマンドの送信
//        let eeprom_control_register = 0x0503;
//        self.wait_eeprom_busy(100)?;
//
//        self.add_bwr(eeprom_control_register, &[0])?; //まずは0を送信。
//        self.send_packet_with_error_read()?;
//        self.recieve_packet_with_error_check(1000_000_000)?;
//
//        self.wait_eeprom_busy(100)?;
//
//        self.add_bwr(eeprom_control_register, &[1])?;
//        self.send_packet_with_error_read()?;
//        self.recieve_packet_with_error_check(1000_000_000)?;
//
//        //EEPROMのエラー確認
//        self.wait_eeprom_busy(100)?;
//        self.add_aprd_all_slave(eeprom_control_register, &[0])?;
//        self.send_packet_with_error_read()?;
//        let res = EtherCATPacketUtil::new(self.recieve_packet_with_error_check(1000_000_000)?)?;
//        for (i, payload_offset) in res.dlpdu_payload_offsets().enumerate() {
//            if i + 1 > slave_count {
//                break;
//            }
//            let is_eeprom_error = (res
//                .packet()
//                .get(payload_offset)
//                .ok_or(PacketError::SmallBuffer)?
//                & 0b0111_1000)
//                != 0;
//            if is_eeprom_error {
//                return Err(EtherCATError::EEPROMStatusError);
//            }
//        }
//
//        //ベンダーIDのリード
//        self.wait_eeprom_busy(100)?;
//        let eeprom_address_register = 0x0504;
//        self.add_bwr(eeprom_address_register, &[0x08])?;
//        self.send_packet_with_error_read()?;
//        self.recieve_packet_with_error_check(1000_000_000)?;
//        self.wait_eeprom_busy(100)?;
//        let eeprom_data_register = 0x0508;
//        self.add_apwr_all_slave(eeprom_data_register, &[0; 2])?;
//        self.send_packet_with_error_read()?;
//        let res = EtherCATPacketUtil::new(self.recieve_packet_with_error_check(1000_000_000)?)?;
//        for (payload_offset, slave) in res.dlpdu_payload_offsets().zip(self.slaves.iter_mut()) {
//            let low = *res
//                .packet()
//                .get(payload_offset)
//                .ok_or(PacketError::SmallBuffer)?;
//            let high = *res
//                .packet()
//                .get(payload_offset + 1)
//                .ok_or(PacketError::SmallBuffer)?;
//            slave.vender_id = ((high as u16) << 8) | (low as u16);
//        }
//
//        //プロダクトコードのリード
//        self.wait_eeprom_busy(100)?;
//        self.add_bwr(eeprom_address_register, &[0x0A])?;
//        self.send_packet_with_error_read()?;
//        self.recieve_packet_with_error_check(1000_000_000)?;
//        self.wait_eeprom_busy(100)?;
//        self.add_apwr_all_slave(eeprom_data_register, &[0; 2])?;
//        self.send_packet_with_error_read()?;
//        let res = EtherCATPacketUtil::new(self.recieve_packet_with_error_check(1000_000_000)?)?;
//        for (payload_offset, slave) in res.dlpdu_payload_offsets().zip(self.slaves.iter_mut()) {
//            let low = *res
//                .packet()
//                .get(payload_offset)
//                .ok_or(PacketError::SmallBuffer)?;
//            let high = *res
//                .packet()
//                .get(payload_offset + 1)
//                .ok_or(PacketError::SmallBuffer)?;
//            slave.product_code = ((high as u16) << 8) | (low as u16);
//        }
//
//        //EEPROMリードコマンド解除
//        let eeprom_control_register = 0x0503;
//        self.wait_eeprom_busy(100)?;
//        self.add_apwr_all_slave(eeprom_control_register, &[0])?;
//        self.send_packet_with_error_read()?;
//        EtherCATPacketUtil::new(self.recieve_packet_with_error_check(1000_000_000)?)?;
//
//        //EEPROMのアクセス権をスレーブにする
//        let eeprom_owner_register = 0x0500;
//        self.wait_eeprom_busy(100)?;
//        self.add_bwr(eeprom_owner_register, &[0])?;
//        self.send_packet_with_error_read()?;
//        self.recieve_packet_with_error_check(1000_000_000)?;
//        Ok(())
//    }
//
//    fn is_eeprom_busy(&mut self) -> Result<bool, EtherCATError> {
//        let eeprom_status_register = 0x0503;
//        self.add_brd(eeprom_status_register, &[0])?;
//        self.send_packet_with_error_read()?;
//        let res = EtherCATPacketUtil::new(self.recieve_packet_with_error_check(1000_000_000)?)?;
//        let payload_offset = res
//            .dlpdu_payload_offsets()
//            .next()
//            .ok_or(PacketError::SmallBuffer)?;
//        let is_busy = (res
//            .packet()
//            .get(payload_offset)
//            .ok_or(PacketError::SmallBuffer)?
//            & 0b1000_0000)
//            != 0;
//        Ok(is_busy)
//    }
//
//    fn wait_eeprom_busy(&mut self, timeout_mills: u64) -> Result<(), EtherCATError> {
//        let now = std::time::Instant::now();
//        let timeout = std::time::Duration::from_millis(timeout_mills);
//        while self.is_eeprom_busy()? {
//            if now.elapsed() >= timeout {
//                return Err(EtherCATError::EEPROMBusyTimeout);
//            }
//            std::thread::sleep(std::time::Duration::from_millis(10));
//        }
//        Ok(())
//    }
