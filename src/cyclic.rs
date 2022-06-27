pub mod al_state_reader;
pub mod al_state_transfer;
pub mod dc_initilizer;
pub mod mailbox;
pub mod mailbox_reader;
pub mod mailbox_writer;
pub mod network_initilizer;
pub mod ram_access_unit;
pub mod sdo;
pub mod sdo_downloader;
pub mod sdo_uploader;
pub mod sii_reader;
pub mod slave_initializer;
use crate::arch::*;
use crate::error::*;
use crate::interface;
use crate::interface::Command;
use crate::interface::*;
use crate::network::*;
use crate::packet::*;
use core::time::Duration;

///EtherCat system time is expressed in nanoseconds elapsed since January 1, 2000.
#[derive(Debug, Clone, Copy)]
pub struct EtherCatSystemTime(pub u64);

pub trait CyclicProcess {
    fn next_command<'a, 'b, 'c>(
        &mut self,
        //desc: &mut NetworkDescription<'a, 'b, 'c>,
        sys_time: EtherCatSystemTime,
    ) -> Option<(Command, &[u8])>;
    fn recieve_and_process(
        &mut self,
        recv_data: Option<ReceivedData>,
        //desc: &mut NetworkDescription,
        sys_time: EtherCatSystemTime,
    );
}

#[derive(Debug, Clone)]
pub struct ReceivedData<'a> {
    pub command: Command,
    pub data: &'a [u8],
    pub wkc: u16,
}

#[derive(Debug, Clone)]
pub struct UnitHandle(usize);
impl From<UnitHandle> for usize {
    fn from(handle: UnitHandle) -> Self {
        handle.0
    }
}

#[derive(Debug)]
pub enum Unit<C: CyclicProcess> {
    NextFreeUnit(UnitHandle),
    Unit((C, bool)),
}

impl<C: CyclicProcess> Default for Unit<C> {
    fn default() -> Self {
        Self::NextFreeUnit(UnitHandle(0))
    }
}

impl<C: CyclicProcess> From<C> for Unit<C> {
    fn from(unit: C) -> Self {
        Self::Unit((unit, false))
    }
}

#[derive(Debug)]
pub struct CyclicUnits<'packet, 'units, D, C, T>
where
    D: Device,
    C: CyclicProcess,
    T: CountDown,
{
    iface: EtherCatInterface<'packet, D, T>,
    //units: Vec<Unit<C>, 8>,
    units: &'units mut [Unit<C>],
    free_unit: UnitHandle,
}

impl<'packet, 'units, D, C, T> CyclicUnits<'packet, 'units, D, C, T>
where
    D: Device,
    C: CyclicProcess,
    T: CountDown,
{
    pub fn new(iface: EtherCatInterface<'packet, D, T>, units: &'units mut [Unit<C>]) -> Self {
        units
            .iter_mut()
            .enumerate()
            .for_each(|(i, unit)| *unit = Unit::NextFreeUnit(UnitHandle(i + 1)));
        Self {
            iface,
            //units: Vec::default(),
            units,
            free_unit: UnitHandle(0),
        }
    }

    pub fn take_resources(self) -> (EtherCatInterface<'packet, D, T>, &'units mut [Unit<C>]) {
        (self.iface, self.units)
    }

    pub fn add_unit(&mut self, unit: C) -> Result<UnitHandle, C> {
        let index = self.free_unit.clone();
        if let Some(unit_enum) = self.units.get_mut(index.0) {
            if let Unit::NextFreeUnit(next) = unit_enum {
                self.free_unit = next.clone();
                *unit_enum = Unit::Unit((unit, false));
                Ok(index)
            } else {
                unreachable!()
            }
        } else {
            //self.units
            //    .push(Unit::Unit((unit, false)))
            //    .map_err(|u| u.take())?;
            //self.free_unit = UnitHandle(index.0 + 1);
            //Ok(index)
            Err(unit)
        }
    }

    pub fn remove_unit(&mut self, unit_handle: UnitHandle) -> Option<C> {
        if let Some(unit_enum) = self.units.get_mut(unit_handle.0) {
            match unit_enum {
                Unit::Unit(_) => {
                    let mut next = Unit::NextFreeUnit(self.free_unit.clone());
                    self.free_unit = unit_handle;
                    core::mem::swap(unit_enum, &mut next);
                    if let Unit::Unit((unit, _)) = next {
                        Some(unit)
                    } else {
                        unreachable!()
                    }
                }
                Unit::NextFreeUnit(_) => None,
            }
        } else {
            None
        }
    }

    pub fn get_unit(&mut self, unit_handle: &UnitHandle) -> Option<&mut C> {
        match self.units.get_mut(unit_handle.0) {
            Some(Unit::Unit((ref mut unit, _))) => Some(unit),
            _ => None,
        }
    }

    pub fn poll<I: Into<Duration>>(
        &mut self,
        //desc: &mut NetworkDescription,
        sys_time: EtherCatSystemTime,
        recv_timeout: I,
    ) -> Result<(), interface::Error> {
        let timeout: Duration = recv_timeout.into();
        loop {
            //let is_all_commands_enqueued = self.enqueue_commands(desc, sys_time)?;
            let is_all_commands_enqueued = self.enqueue_commands(sys_time)?;

            //self.process(desc, sys_time, timeout)?;
            self.process(sys_time, timeout)?;

            if is_all_commands_enqueued {
                break;
            }
        }
        Ok(())
    }

    fn enqueue_commands(
        &mut self,
        //desc: &mut NetworkDescription,
        sys_time: EtherCatSystemTime,
    ) -> Result<bool, interface::Error> {
        let mut complete = true;
        for (i, unit_enum) in self.units.iter_mut().enumerate() {
            if let Unit::Unit((unit, sent)) = unit_enum {
                if *sent {
                    continue;
                }
                //if let Some((command, data)) = unit.next_command(desc, sys_time) {
                if let Some((command, data)) = unit.next_command(sys_time) {
                    let len = data.len();
                    if self.iface.remainig_capacity() < len {
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
        }
        Ok(complete)
    }

    fn process<I: Into<Duration>>(
        &mut self,
        //desc: &mut NetworkDescription,
        sys_time: EtherCatSystemTime,
        recv_timeout: I,
    ) -> Result<(), interface::Error> {
        let Self { iface, units, .. } = self;
        match iface.poll(recv_timeout) {
            Ok(_) => {}
            Err(interface::Error::RecieveTimeout) => {}
            Err(err) => return Err(err),
        }
        let pdus = iface.consume_commands();
        let mut last_index = 0;
        for pdu in pdus {
            let index = pdu.index() as usize;
            for j in last_index..index {
                if let Some((unit, sent)) = get_unit_with_sent_flag(units, UnitHandle(j)) {
                    if *sent {
                        //unit.recieve_and_process(None, desc, sys_time);
                        unit.recieve_and_process(None, sys_time);

                        *sent = false;
                    }
                }
            }
            if let Some((unit, sent)) = get_unit_with_sent_flag(units, UnitHandle(index)) {
                let wkc = pdu.wkc().unwrap_or_default();
                let command =
                    Command::new(CommandType::new(pdu.command_type()), pdu.adp(), pdu.ado());
                let recv_data = ReceivedData {
                    command,
                    data: pdu.data(),
                    wkc,
                };
                assert!(*sent);
                //unit.recieve_and_process(Some(recv_data), desc, sys_time);
                unit.recieve_and_process(Some(recv_data), sys_time);

                *sent = false;
            }
            last_index = index + 1;
        }
        for j in last_index..units.len() {
            if let Some((unit, sent)) = get_unit_with_sent_flag(units, UnitHandle(j)) {
                if *sent {
                    //unit.recieve_and_process(None, desc, sys_time);
                    unit.recieve_and_process(None, sys_time);

                    *sent = false;
                }
            }
        }
        Ok(())
    }
}

fn get_unit_with_sent_flag<C: CyclicProcess>(
    units: &mut [Unit<C>],
    unit_handle: UnitHandle,
) -> Option<(&mut C, &mut bool)> {
    match units.get_mut(unit_handle.0) {
        Some(Unit::Unit((ref mut unit, ref mut sent))) => Some((unit, sent)),
        _ => None,
    }
}
