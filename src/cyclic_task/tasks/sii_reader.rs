use crate::cyclic_task::CyclicProcess;
use crate::error::EcError;
use super::super::interface::*;
use crate::register::{SiiAccess, SiiAddress, SiiControl, SiiData};
use crate::util::const_max;

use super::super::EtherCatSystemTime;
use super::super::ReceivedData;

const TIMEOUT_MS: u32 = 100;

#[derive(Debug, Clone)]
pub enum SiiTaskError {
    PermittionDenied,
    AddressSizeOver,
    Busy,
    CheckSumError,
    DeviceInfoError,
    CommandError,
    TimeoutMs(u32),
}

impl From<SiiTaskError> for EcError<SiiTaskError> {
    fn from(err: SiiTaskError) -> Self {
        Self::TaskSpecific(err)
    }
}

#[derive(Debug)]
enum State {
    Error(EcError<SiiTaskError>),
    Idle,
    Init,
    SetOwnership,
    CheckOwnership,
    SetAddress,
    SetReadOperation,
    Wait,
    Read,
    Complete,
}

impl Default for State {
    fn default() -> Self {
        Self::Init
    }
}

#[derive(Debug)]
pub struct SiiReader {
    timer_start: EtherCatSystemTime,
    command: Command,
    slave_address: SlaveAddress,
    buffer: [u8; buffer_size()],
    state: State,
    sii_address: u16,
    read_size: usize,
    sii_data: SiiData<[u8; SiiData::SIZE]>,
}

impl SiiReader {
    pub fn new() -> Self {
        Self {
            timer_start: EtherCatSystemTime(0),
            state: State::Idle,
            slave_address: SlaveAddress::SlavePosition(0),
            sii_address: 0,
            command: Command::default(),
            buffer: [0; buffer_size()],
            read_size: 0,
            sii_data: SiiData::new(),
        }
    }

    pub fn start(&mut self, slave_address: SlaveAddress, sii_address: u16) {
        self.slave_address = slave_address;
        self.sii_address = sii_address;
        self.state = State::Init;
        self.buffer.fill(0);
        self.command = Command::default();
    }

    pub fn wait(
        &mut self,
    ) -> Option<Result<(SiiData<[u8; SiiData::SIZE]>, usize), EcError<SiiTaskError>>> {
        match &self.state {
            State::Complete => Some(Ok((self.sii_data.clone(), self.read_size))),
            State::Error(err) => Some(Err(err.clone())),
            _ => None,
        }
    }
}

impl CyclicProcess for SiiReader {
    fn next_command(&mut self, _: EtherCatSystemTime) -> Option<(Command, &[u8])> {
        match self.state {
            State::Idle => None,
            State::Error(_) => None,
            State::Init => {
                self.command = Command::new_read(self.slave_address.into(), SiiControl::ADDRESS);
                self.buffer.fill(0);
                Some((self.command, &self.buffer[..SiiControl::SIZE]))
            }
            State::SetOwnership => {
                self.buffer.fill(0);
                let mut sii_access = SiiAccess(&mut self.buffer);
                sii_access.set_owner(false);
                sii_access.set_reset_access(true);
                self.command = Command::new_write(self.slave_address.into(), SiiAccess::ADDRESS);
                Some((self.command, &self.buffer[..SiiAccess::SIZE]))
            }
            State::CheckOwnership => {
                self.command = Command::new_read(self.slave_address.into(), SiiAccess::ADDRESS);
                self.buffer.fill(0);
                Some((self.command, &self.buffer[..SiiAccess::SIZE]))
            }
            State::SetAddress => {
                self.buffer.fill(0);
                let mut sii_address = SiiAddress(&mut self.buffer);
                sii_address.set_sii_address(self.sii_address as u32);
                self.command = Command::new_write(self.slave_address.into(), SiiAddress::ADDRESS);
                Some((self.command, &self.buffer[..SiiAddress::SIZE]))
            }
            State::SetReadOperation => {
                self.buffer.fill(0);
                let mut sii_address = SiiControl(&mut self.buffer);
                sii_address.set_read_operation(true);
                self.command = Command::new_write(self.slave_address.into(), SiiControl::ADDRESS);
                Some((self.command, &self.buffer[..SiiControl::SIZE]))
            }
            State::Wait => {
                self.command = Command::new_read(self.slave_address.into(), SiiControl::ADDRESS);
                self.buffer.fill(0);
                Some((self.command, &self.buffer[..SiiControl::SIZE]))
            }
            State::Read => {
                self.command = Command::new_read(self.slave_address.into(), SiiData::ADDRESS);
                self.buffer.fill(0);
                Some((self.command, &self.buffer[..SiiData::SIZE]))
            }
            State::Complete => None,
        }
    }

    fn recieve_and_process(
        &mut self,
        recv_data: Option<ReceivedData>,
        sys_time: EtherCatSystemTime,
    ) {
        let data = if let Some(recv_data) = recv_data {
            let ReceivedData { command, data, wkc } = recv_data;
            if !(command.c_type == self.command.c_type && command.ado == self.command.ado) {
                self.state = State::Error(EcError::UnexpectedCommand);
            }
            if wkc != 1 {
                self.state = State::Error(EcError::UnexpectedWkc(wkc));
            }
            data
        } else {
            self.state = State::Error(EcError::LostPacket);
            return;
        };

        match self.state {
            State::Idle => {}
            State::Error(_) => {}
            State::Init => {
                let sii_control = SiiControl(data);
                if sii_control.check_sum_error() {
                    self.state = State::Error(SiiTaskError::CheckSumError.into());
                }
                if sii_control.device_info_error() {
                    self.state = State::Error(SiiTaskError::DeviceInfoError.into());
                }

                // アドレスアルゴリズムが0なら、アドレスは1オクテットでなければならない。
                if !sii_control.address_algorithm() && self.sii_address >> 8 != 0 {
                    self.state = State::Error(SiiTaskError::AddressSizeOver.into());
                }

                if sii_control.busy()
                    || sii_control.read_operation()
                    || sii_control.write_operation()
                    || sii_control.reload_operation()
                {
                    self.state = State::Error(SiiTaskError::Busy.into());
                }

                self.read_size = if sii_control.read_size() { 8 } else { 4 };

                self.state = State::SetOwnership;
            }
            State::SetOwnership => {
                self.state = State::CheckOwnership;
            }
            State::CheckOwnership => {
                let sii_access = SiiAccess(data);
                if sii_access.owner() || sii_access.pdi_accessed() {
                    self.state = State::Error(SiiTaskError::PermittionDenied.into());
                }
                self.state = State::SetAddress;
            }
            State::SetAddress => {
                self.state = State::SetReadOperation;
            }
            State::SetReadOperation => {
                self.state = State::Wait;
                self.timer_start = sys_time;
            }
            State::Wait => {
                let sii_control = SiiControl(data);
                if sii_control.command_error() {
                    self.state = State::Error(SiiTaskError::CommandError.into());
                } else if !sii_control.busy() && !sii_control.read_operation() {
                    self.state = State::Read;
                } else if self.timer_start.0 < sys_time.0
                    && TIMEOUT_MS as u64 * 1000 < sys_time.0 - self.timer_start.0
                {
                    self.state = State::Error(SiiTaskError::TimeoutMs(TIMEOUT_MS).into())
                }
            }
            State::Read => {
                self.sii_data
                    .0
                    .iter_mut()
                    .zip(data)
                    .for_each(|(b, d)| *b = *d);
                self.state = State::Complete;
            }
            State::Complete => {}
        }
    }
}

const fn buffer_size() -> usize {
    let mut size = 0;
    size = const_max(size, SiiAccess::SIZE);
    size = const_max(size, SiiControl::SIZE);
    size = const_max(size, SiiAddress::SIZE);
    size = const_max(size, SiiData::SIZE);
    size
}
