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

pub struct Command {
    c_type: CommandType,
    adp: u16,
    ado: u16,
}

#[derive(Debug)]
pub enum CyclicProcessingUnit {
    TEST,
}

impl CyclicProcessingUnit {
    fn data_size(&self) -> usize{
        todo!()
    }

    fn process(&mut self) -> Option<(Command, &[u8])> {
        todo!()
    }

    fn receive(&mut self, command: Command, data: &[u8], wkc: u16) -> bool {
        todo!()
    }
}

#[derive(Debug)]
pub struct EtherCATMaster<'a, D, T>
where
    D: Device,
    T: CountDown<Time = MicrosDurationU32>,
{
    iface: &'a mut EtherCATInterface<'a, D, T>,
    units: &'a mut [CyclicProcessingUnit],
    units_len: usize,
}

impl<'a, D, T> EtherCATMaster<'a, D, T>
where
    D: Device,
    T: CountDown<Time = MicrosDurationU32>,
{
    pub fn process_and_enqueue(&mut self) -> Result<bool, CommonError> {
        let mut complete = true;
        for (i, unit) in self.units.iter_mut().enumerate() {
            if let Some((command, data)) = unit.process() {
                let len = data.len();
                if self.iface.remaing_capacity() < len{
                    complete = false;
                    break;
                }
                let _ = self.iface.add_command(
                    i as u8,
                    command.c_type,
                    command.adp,
                    command.ado,
                    len,
                    |buf| {
                        for (b, d) in buf.iter_mut().zip(data) {
                            *b = *d;
                        }
                    },
                )?;
            }
        }
        Ok(complete)
    }

    pub fn poll<I: Into<MicrosDurationU32>>(&mut self, timeout: I) -> Result<bool, CommonError>{
        let mut is_ok = true;
        self.iface.poll(timeout)?;
        let pdus = self.iface.consume_command();
        for pdu in pdus{
            let index = pdu.index() as usize;
            if let Some(unit) = self.units.get_mut(index){
                let wkc = pdu.wkc().unwrap_or_default();
                let command = Command{
                    c_type: CommandType::new(pdu.command_type()),
                    adp: pdu.adp(),
                    ado: pdu.ado(),
                };
                if !unit.receive(command, pdu.data(), wkc){
                    is_ok = false;
                }
            }
        }
        Ok(is_ok)
    }
}
