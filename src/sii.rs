use crate::arch::*;
use crate::interface::*;
use crate::slave_device::*;
use crate::register::{datalink::*, application::*};
use crate::packet::*;
use crate::util::*;
use crate::error::Error;


// アプリケーションごとにエラーが欲しい。
pub struct EEPROMReader<'a, 'b, D>
where
    D: Device,
{
    iface: &'a mut EtherCATInterface<'b, D>,
}

impl<'a, 'b, D> EEPROMReader<'a, 'b, D>
where
    D: Device,
{
    pub fn new(iface: &'a mut EtherCATInterface<'b, D>) -> Self {
        Self { iface }
    }

    pub fn is_granted(&mut self, slave: &Slave) -> Result<bool, Error>{
        let station_address = slave.station_address;
        let reg = SIIAccess::<&[u8]>::ADDRESS;
        let mut data = [0_u8; SIIAccess::<&[u8]>::SIZE];
        let datagram = SIIAccess(&mut data);
        self.iface.add_command(CommandType::FPRD, station_address, reg, &data)?;
        self.iface.poll()?;
        let pdu = self.iface.consume_command().last().ok_or(Error::Dropped)?;
        check_wkc(&pdu, 1)?;
        let sii_access = SIIAccess(pdu.data());
        Ok(!sii_access.owner() && !sii_access.access_pdi())
    }

    pub fn read32(&mut self, address: u16, slave: &Slave) -> Result<u32, Error>{
        todo!()
    }
}
