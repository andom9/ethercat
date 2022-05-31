pub mod al_state_transfer;
pub mod dc_initilizer;
pub mod mailbox_reader;
pub mod mailbox_writer;
pub mod network_initilizer;
pub mod sii_reader;
pub mod slave_initializer;
pub mod sdo;

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
        recv_data: Option<ReceivedData>,
        desc: &mut NetworkDescription,
        sys_time: EtherCATSystemTime,
    );
}

#[derive(Debug, Clone)]
pub struct ReceivedData<'a> {
    pub command: Command,
    pub data: &'a [u8],
    pub wkc: u16,
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
    units: Vec<(C, bool), U>,
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
        self.units
            .push((unit, false))
            .map(|_| UnitHandle(len))
            .map_err(|(c, _)| c)
    }

    pub fn unit_mut(&mut self, handle: UnitHandle) -> Option<&mut C> {
        self.units.get_mut(handle.0 as usize).map(|(c, _)| c)
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
        for (i, (unit, sent)) in self.units.iter_mut().enumerate() {
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
                *sent = true;
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
        let mut last_index = 0;
        for pdu in pdus {
            let index = pdu.index() as usize;
            for j in last_index..index {
                if let Some((unit, sent)) = self.units.get_mut(j) {
                    if *sent {
                        unit.recieve_and_process(None, desc, sys_time);
                        *sent = false;
                    }
                }
            }
            if let Some((unit, sent)) = self.units.get_mut(index) {
                let wkc = pdu.wkc().unwrap_or_default();
                let command =
                    Command::new(CommandType::new(pdu.command_type()), pdu.adp(), pdu.ado());
                let recv_data = ReceivedData {
                    command,
                    data: pdu.data(),
                    wkc,
                };
                assert!(*sent);
                unit.recieve_and_process(Some(recv_data), desc, sys_time);
                *sent = false;
            }
            last_index = index + 1;
        }
        for j in last_index..self.units.len() {
            if let Some((unit, sent)) = self.units.get_mut(j) {
                if *sent {
                    unit.recieve_and_process(None, desc, sys_time);
                    *sent = false;
                }
            }
        }
        Ok(())
    }
}
