//use crate::al_state_transfer::*;
use crate::al_state_transfer::ALStateTransfer;
use crate::arch::*;
use crate::error::*;
use crate::interface::*;
use crate::packet::*;
use crate::register::datalink::*;
use crate::sii::SIIReader;
use crate::sii::*;
use crate::slave_status::*;
use bit_field::BitField;
use embedded_hal::timer::*;
use fugit::*;

pub trait Cyclic {
    fn next_transmission_data(&mut self) -> Option<(Command, &[u8])>;
    fn recieve_and_process(&mut self, command: Command, data: &[u8], wkc: u16) -> bool;
}

#[derive(Debug)]
pub enum CyclicProcessingUnit<'a, T>
where
    T: CountDown<Time = MicrosDurationU32>,
{
    SIIReader(SIIReader<'a, T>),
    AlState(ALStateTransfer<'a, T>),
}

impl<'a, T> Cyclic for CyclicProcessingUnit<'a, T>
where
    T: CountDown<Time = MicrosDurationU32>,
{
    fn next_transmission_data(&mut self) -> Option<(Command, &[u8])> {
        match self {
            Self::SIIReader(unit) => unit.next_transmission_data(),
            Self::AlState(unit) => unit.next_transmission_data(),
        }
    }

    fn recieve_and_process(&mut self, command: Command, data: &[u8], wkc: u16) -> bool {
        match self {
            Self::SIIReader(unit) => unit.recieve_and_process(command, data, wkc),
            Self::AlState(unit) => unit.recieve_and_process(command, data, wkc),
        }
    }
}

#[derive(Debug)]
pub struct EtherCATMaster<'a, D, T, C>
where
    D: Device,
    T: CountDown<Time = MicrosDurationU32>,
    C: Cyclic,
{
    iface: &'a mut EtherCATInterface<'a, D, T>,
    units: &'a mut [C],
    //units_len: usize,
}

impl<'a, D, T, C> EtherCATMaster<'a, D, T, C>
where
    D: Device,
    T: CountDown<Time = MicrosDurationU32>,
    C: Cyclic,
{
    pub fn enqueue(&mut self) -> Result<bool, CommonError> {
        let mut complete = true;
        for (i, unit) in self.units.iter_mut().enumerate() {
            if let Some((command, data)) = unit.next_transmission_data() {
                let len = data.len();
                if self.iface.remaing_capacity() < len {
                    complete = false;
                    break;
                }
                let _ = self.iface.add_command(i as u8, command, len, |buf| {
                    for (b, d) in buf.iter_mut().zip(data) {
                        *b = *d;
                    }
                })?;
            }
        }
        Ok(complete)
    }

    pub fn send_and_recieve<I: Into<MicrosDurationU32>>(
        &mut self,
        timeout: I,
    ) -> Result<bool, CommonError> {
        let mut is_ok = true;
        self.iface.poll(timeout)?;
        let pdus = self.iface.consume_command();
        for pdu in pdus {
            let index = pdu.index() as usize;
            if let Some(unit) = self.units.get_mut(index) {
                let wkc = pdu.wkc().unwrap_or_default();
                let command =
                    Command::new(CommandType::new(pdu.command_type()), pdu.adp(), pdu.ado());
                if !unit.recieve_and_process(command, pdu.data(), wkc) {
                    is_ok = false;
                }
            }
        }
        Ok(is_ok)
    }
}
