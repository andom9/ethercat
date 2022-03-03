use crate::arch::*;
use crate::datalink::*;
use crate::error::*;
use crate::util::*;
use crate::packet::*;
use crate::slave_device::*;
use bitfield::*;
use heapless::Vec;

//メモ：論理マップはバイト列でそのまま持つ。
//　　　イテレーションしながら、読み込むとき、書き込み時はレンジ指定する。

pub struct EtherCATInitializer<'a, D, const S: usize>
where
    D: Device,
{
    iface: EtherCATInterface<'a, D>,
    slaves: Vec<Slave, S>,
}

impl<'a, D, const S: usize> EtherCATInitializer<'a, D, S>
where
    D: Device,
{
    pub fn scan_slaves(&mut self) -> Result<(), Error> {
        let slave_count;
        loop {
            let slave_count1 = self.count_slaves()?;
            let slave_count2 = self.count_slaves()?;
            if slave_count1 == slave_count2 {
                slave_count = slave_count1;
                break;
            }
        }

        for _ in 0..slave_count {
            self.slaves.push(Slave::default()).unwrap();
        }

        for i in 0..slave_count {
            // TODO: Get Vender ID from EEPROM
            // TODO: Get Product code from EEPROM
            // TODO: Get Revision number from EEPROM
            // TODO: Get FMMU info(availability, address)
            // TODO: Get SM Info(availability, size, start_address)
            // TODO: Get CoE availability
            // TODO: Get FoE availability
            // TODO: Get DC availability

            // get al state
            let al_state = self.read_alstate(i)?;
            self.slaves[i as usize].al_state = al_state;

            // get port type
            let physics = self.read_port_physics(i)?;
            self.slaves[i as usize].physics = physics;
        }

        todo!()
    }

    fn count_slaves(&mut self) -> Result<u16, Error> {
        let Self { iface, .. } = self;
        iface.add_command(CommandType::BRD, 0, 0, &[0])?;
        iface.poll()?;
        let pdu = iface.consume_command().next().ok_or(Error::Dropped)?;
        Ok(pdu.wkc().unwrap())
    }

    fn read_alstate(&mut self, slave_no: u16) -> Result<AlState, Error> {
        let reg = 0x0130;
        let Self { iface, .. } = self;
        iface.add_command(CommandType::APRD, get_ap_adp(slave_no), reg, &[0])?;
        iface.poll()?;
        let pdu = iface.consume_command().next().ok_or(Error::Dropped)?;
        check_wkc(&pdu, 1)?;
        
        let slave_state = pdu.data()[0] & 0b0000_1111;
        let al_state = AlState::from(slave_state);
        Ok(al_state)
    }

    fn read_port_physics(&mut self, slave_no: u16) -> Result<[Option<Physics>; 4], Error> {
        let reg = 0x0E00;
        let Self { iface, .. } = self;
        iface.add_command(CommandType::APRD, get_ap_adp(slave_no), reg, &[0])?;
        iface.poll()?;
        let pdu = iface.consume_command().next().ok_or(Error::Dropped)?;
        check_wkc(&pdu, 1)?;

        let mut physics = [None; 4];

        // port 0
        if pdu.data()[0].bit(2) {
            physics[0] = Some(Physics::MII);
        } else {
            physics[0] = Some(Physics::EBUS);
        }

        // port 1
        if pdu.data()[0].bit(3) {
            physics[1] = Some(Physics::MII);
        } else {
            physics[1] = Some(Physics::EBUS);
        }

        // port 2
        if pdu.data()[0].bit(0) {
            if pdu.data()[0].bit(4) {
                physics[2] = Some(Physics::MII);
            } else {
                physics[2] = Some(Physics::EBUS);
            }
        }

        // port 3
        if pdu.data()[0].bit(1) {
            if pdu.data()[0].bit(5) {
                physics[3] = Some(Physics::MII);
            } else {
                physics[3] = Some(Physics::EBUS);
            }
        }

        Ok(physics)
    }
}