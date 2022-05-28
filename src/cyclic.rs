pub mod al_state_transfer;
pub mod dc_initilizer;
pub mod network_initilizer;
pub mod sii_reader;
pub mod slave_initializer;

use crate::arch::*;
use crate::error::*;
use crate::interface::Command;
use crate::interface::*;
use crate::network::*;
use crate::packet::*;
use embedded_hal::timer::*;
use fugit::*;
use heapless::Vec;

///EtherCAT system time is expressed in nanoseconds elapsed since January 1, 2000.
#[derive(Debug, Clone, Copy)]
pub struct EtherCATSystemTime(pub u64);

pub trait Cyclic {
    fn next_command(
        &mut self,
        desc: &mut NetworkDescription,
        sys_time: EtherCATSystemTime,
    ) -> Option<(Command, &[u8])>;
    fn recieve_and_process(
        &mut self,
        command: Command,
        data: &[u8],
        wkc: u16,
        desc: &mut NetworkDescription,
        sys_time: EtherCATSystemTime,
    ) -> bool;
}

#[derive(Debug, Clone, Copy)]
pub struct UnitHandle(u8);

#[derive(Debug)]
pub struct CyclicProcess<'a, D, T, C, const U: usize>
where
    D: Device,
    T: CountDown<Time = MicrosDurationU32>,
    C: Cyclic,
{
    iface: &'a mut EtherCATInterface<'a, D, T>,
    units: Vec<C, U>,
}

impl<'a, D, T, C, const U: usize> CyclicProcess<'a, D, T, C, U>
where
    D: Device,
    T: CountDown<Time = MicrosDurationU32>,
    C: Cyclic,
{
    pub fn new(iface: &'a mut EtherCATInterface<'a, D, T>) -> Self {
        Self {
            iface,
            units: Vec::default(),
        }
    }

    pub fn add_unit(&mut self, unit: C) -> Result<UnitHandle, C> {
        let len = self.units.len() as u8;
        self.units.push(unit).map(|_| UnitHandle(len))
    }

    pub fn unit_mut(&mut self, handle: UnitHandle) -> Option<&mut C> {
        self.units.get_mut(handle.0 as usize)
    }

    pub fn poll<I: Into<MicrosDurationU32> + Clone>(
        &mut self,
        desc: &mut NetworkDescription,
        sys_time: EtherCATSystemTime,
        recv_timeout: I,
    ) -> Result<(), CommonError> {
        loop {
            let is_all_commands_enqueued = self.enqueue_commands(desc, sys_time)?;
            self.process(desc, sys_time, recv_timeout.clone())?;
            if is_all_commands_enqueued {
                break;
            }
        }
        Ok(())
    }

    fn enqueue_commands(
        &mut self,
        desc: &mut NetworkDescription,
        sys_time: EtherCATSystemTime,
    ) -> Result<bool, CommonError> {
        let mut complete = true;
        for (i, unit) in self.units.iter_mut().enumerate() {
            if let Some((command, data)) = unit.next_command(desc, sys_time) {
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

    fn process<I: Into<MicrosDurationU32>>(
        &mut self,
        desc: &mut NetworkDescription,
        sys_time: EtherCATSystemTime,
        timeout: I,
    ) -> Result<(), CommonError> {
        self.iface.poll(timeout)?;
        let pdus = self.iface.consume_command();
        for pdu in pdus {
            let index = pdu.index() as usize;
            if let Some(unit) = self.units.get_mut(index) {
                let wkc = pdu.wkc().unwrap_or_default();
                let command =
                    Command::new(CommandType::new(pdu.command_type()), pdu.adp(), pdu.ado());
                unit.recieve_and_process(command, pdu.data(), wkc, desc, sys_time);
            }
        }
        Ok(())
    }
}
