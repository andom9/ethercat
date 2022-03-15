use crate::arch::*;
use crate::error::CommonError;
use crate::interface::*;
use crate::packet::*;
use crate::register::{application::*, datalink::*};
use crate::slave_device::*;
use crate::util::*;
use log::*;

#[derive(Debug, Clone)]
pub enum SIIError {
    Conmmon(CommonError),
    PermittionDenied,
    AddressSizeOver,
    Busy,
    CheckSumError,
    DeviceInfoError,
    CommandError,
}

impl From<CommonError> for SIIError {
    fn from(err: CommonError) -> Self {
        Self::Conmmon(err)
    }
}

// アプリケーションごとにエラーが欲しい。
pub struct SlaveInformationInterface<'a, 'b, D>
where
    D: Device,
{
    iface: &'a mut EtherCATInterface<'b, D>,
}

impl<'a, 'b, D> SlaveInformationInterface<'a, 'b, D>
where
    D: Device,
{
    pub fn new(iface: &'a mut EtherCATInterface<'b, D>) -> Self {
        Self { iface }
    }

    fn get_ownership(&mut self, slave_address: SlaveAddress) -> Result<(), SIIError> {
        let sii_access = self.iface.read_sii_access(slave_address)?;
        if sii_access.owner() || sii_access.pdi_accessed() {
            self.iface
                .write_sii_access(slave_address, Some(sii_access), |reg| {
                    reg.set_owner(false);
                    reg.set_reset_access(true);
                })?;
        } else {
            return Ok(());
        }

        let sii_access = self.iface.read_sii_access(slave_address)?;
        if sii_access.owner() || sii_access.pdi_accessed() {
            Err(SIIError::PermittionDenied)
        } else {
            Ok(())
        }
    }

    // タプルの2番目のデータは読み取ったサイズで4もしくは8となる
    pub fn read(
        &mut self,
        slave_address: SlaveAddress,
        sii_address: u16,
    ) -> Result<(SIIData<[u8; 8]>, usize), SIIError> {
        let sii_control = self.iface.read_sii_control(slave_address)?;
        if sii_control.check_sum_error() {
            return Err(SIIError::CheckSumError);
        }
        if sii_control.device_info_error() {
            return Err(SIIError::DeviceInfoError);
        }

        // アドレスアルゴリズムが0なら、アドレスは1オクテットでなければならない。
        info!("{}", sii_control.address_algorithm());
        if !sii_control.address_algorithm() && sii_address >> 8 != 0 {
            return Err(SIIError::AddressSizeOver);
        }

        let read_size = if sii_control.read_size() { 8 } else { 4 };

        // このあとビジーかどうか確認するので、今ビジーだと困る。
        if sii_control.busy()
            || sii_control.read_operation()
            || sii_control.write_operation()
            || sii_control.reload_operation()
        {
            return Err(SIIError::Busy);
        }

        self.get_ownership(slave_address)?;

        // 読みだしたいアドレスを書く
        self.iface.write_sii_address(slave_address, None, |reg| {
            reg.set_sii_address(sii_address as u32);
        })?;

        // 読み出し開始する
        let test = self
            .iface
            .write_sii_control(slave_address, Some(sii_control), |reg| {
                reg.set_read_operation(true);
            })?;

        // TODO:タイムアウトの追加
        loop {
            let sii_control = self.iface.read_sii_control(slave_address)?;
            if sii_control.command_error() {
                return Err(SIIError::CommandError);
            }
            if !sii_control.busy() && !sii_control.read_operation() {
                break;
            }
        }

        let data = self.iface.read_sii_data(slave_address)?;

        Ok((data, read_size))
    }
}
