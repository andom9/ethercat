use crate::arch::*;
use crate::error::CommonError;
use crate::interface::*;
use crate::packet::*;
use crate::register::{application::*, datalink::*};
use crate::slave_device::*;
use crate::util::*;

#[derive(Debug, Clone)]
pub enum SIIError {
    Conmmon(CommonError),
    PermittionDenied,
    Busy,
    NotReadOperation,
}

impl From<CommonError> for SIIError {
    fn from(err: CommonError) -> Self {
        Self::Conmmon(err)
    }
}

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

    pub fn read(&mut self, address: u16, slave: &Slave) -> Result<u64, SIIError> {
        let station_address = SlaveAddress::StationAddress(slave.station_address);
        let sii_control = self.iface.read_sii_control(station_address)?;
        
        self.iface
        .write_sii_control(station_address, Some(sii_control), |reg| {
            reg.set_read_operation(true);
            reg.set_write_operation(false);
            reg.set_reload_operation(false);
        })?;

        let sii_control = self.iface.read_sii_control(station_address)?;
        if !sii_control.read_operation(){
            return Err(SIIError::NotReadOperation);
        }

        let is_busy = sii_control.busy();
        if is_busy {
            return Err(SIIError::Busy);
        }

        todo!()
    }
}
