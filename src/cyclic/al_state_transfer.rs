use super::al_state_reader::AlStatusCode;
use super::EtherCatSystemTime;
use super::ReceivedData;
use crate::cyclic::CyclicProcess;
use crate::error::EcError;
use crate::interface::{Command, SlaveAddress};
use crate::network::NetworkDescription;
use crate::packet::ethercat::CommandType;
use crate::register::application::{AlControl, AlStatus};
use crate::slave::AlState;
use crate::util::const_max;
use core::convert::TryFrom;

// Timeout. Init -> PreOp or Init -> Boot
const PREOP_TIMEOUT_DEFAULT_MS: u32 = 3000;
// Timeout. SafeOp -> Op or PreOp -> SafeOp
const SAFEOP_OP_TIMEOUT_DEFAULT_MS: u32 = 10000;
// Timeout. Op/SafeOp/PreOp/Boot -> Init or SafeOp -> PreOp
const BACK_TO_INIT_TIMEOUT_DEFAULT_MS: u32 = 5000;
// Timeout. Op -> SafeOp
const BACK_TO_SAFEOP_TIMEOUT_DEFAULT_MS: u32 = 200;

const fn max_timeout_ms() -> u32 {
    let mut max = PREOP_TIMEOUT_DEFAULT_MS;
    if max < SAFEOP_OP_TIMEOUT_DEFAULT_MS {
        max = SAFEOP_OP_TIMEOUT_DEFAULT_MS;
    }
    if max < BACK_TO_INIT_TIMEOUT_DEFAULT_MS {
        max = BACK_TO_INIT_TIMEOUT_DEFAULT_MS;
    }
    if max < BACK_TO_SAFEOP_TIMEOUT_DEFAULT_MS {
        max = BACK_TO_SAFEOP_TIMEOUT_DEFAULT_MS;
    }
    max
}

#[derive(Debug, Clone)]
pub enum Error {
    TimeoutMs(u32),
    AlStatusCode((AlState, AlStatusCode)),
}

impl From<Error> for EcError<Error> {
    fn from(err: Error) -> Self {
        Self::UnitSpecific(err)
    }
}

#[derive(Debug)]
enum State {
    Error(EcError<Error>),
    Idle,
    Read,
    ResetError(AlState),
    Complete,
    Request,
    Poll,
}

#[derive(Debug)]
pub struct AlStateTransfer {
    timer_start: EtherCatSystemTime,
    state: State,
    slave_address: Option<SlaveAddress>,
    target_al: AlState,
    command: Command,
    buffer: [u8; buffer_size()],
    current_al_state: AlState,
    timeout_ms: u32,
}

impl AlStateTransfer {
    pub fn new() -> Self {
        Self {
            timer_start: EtherCatSystemTime(0),
            state: State::Idle,
            slave_address: None,
            target_al: AlState::Init,
            command: Command::default(),
            buffer: [0; buffer_size()],
            current_al_state: AlState::Init,
            timeout_ms: 0,
        }
    }

    pub fn start(&mut self, slave_address: Option<SlaveAddress>, target_al_state: AlState) {
        self.slave_address = slave_address;
        self.target_al = target_al_state;
        self.state = State::Read;
        self.buffer.fill(0);
        self.command = Command::default();
    }

    pub fn wait(&mut self) -> Option<Result<AlState, EcError<Error>>> {
        match &self.state {
            State::Complete => Some(Ok(self.current_al_state)),
            State::Error(err) => Some(Err(err.clone())),
            //State::Idle => Err(EcError::NotStarted.into()),
            _ => None,
        }
    }
}

impl CyclicProcess for AlStateTransfer {
    fn next_command(
        &mut self,
        _: &mut NetworkDescription,
        _: EtherCatSystemTime,
    ) -> Option<(Command, &[u8])> {
        match self.state {
            State::Idle => None,
            State::Error(_) => None,
            State::Read => {
                if let Some(slave_address) = self.slave_address {
                    self.command = Command::new_read(slave_address, AlStatus::ADDRESS);
                } else {
                    self.command = Command::new(CommandType::BRD, 0, AlStatus::ADDRESS);
                }
                self.buffer.fill(0);
                Some((self.command, &self.buffer[..AlStatus::SIZE]))
            }
            State::ResetError(current_al_state) => {
                if let Some(slave_address) = self.slave_address {
                    self.command = Command::new_write(slave_address, AlControl::ADDRESS);
                } else {
                    self.command = Command::new(CommandType::BWR, 0, AlControl::ADDRESS);
                }
                self.buffer.fill(0);
                let mut al_control = AlControl(&mut self.buffer);
                al_control.set_state(current_al_state as u8);
                al_control.set_acknowledge(true);
                Some((self.command, &self.buffer[..AlControl::SIZE]))
            }
            State::Request => {
                self.buffer.fill(0);
                let mut al_control = AlControl(&mut self.buffer);
                let target_al = self.target_al;
                al_control.set_state(target_al as u8);
                if let Some(slave_address) = self.slave_address {
                    self.command = Command::new_write(slave_address, AlControl::ADDRESS);
                } else {
                    self.command = Command::new(CommandType::BWR, 0, AlControl::ADDRESS);
                }
                self.timeout_ms = match (self.current_al_state, target_al) {
                    (AlState::PreOperational, AlState::SafeOperational)
                    | (_, AlState::Operational) => SAFEOP_OP_TIMEOUT_DEFAULT_MS,
                    (_, AlState::PreOperational) | (_, AlState::Bootstrap) => {
                        PREOP_TIMEOUT_DEFAULT_MS
                    }
                    (_, AlState::Init) => BACK_TO_INIT_TIMEOUT_DEFAULT_MS,
                    (_, AlState::SafeOperational) => BACK_TO_SAFEOP_TIMEOUT_DEFAULT_MS,
                    (_, AlState::InvalidOrMixed) => max_timeout_ms(),
                };
                Some((self.command, &self.buffer[..AlControl::SIZE]))
            }
            State::Poll => {
                if let Some(slave_address) = self.slave_address {
                    self.command = Command::new_read(slave_address, AlStatus::ADDRESS);
                } else {
                    self.command = Command::new(CommandType::BRD, 0, AlStatus::ADDRESS);
                }
                self.buffer.fill(0);
                Some((self.command, &self.buffer[..AlStatus::SIZE]))
            }
            State::Complete => None,
        }
    }

    fn recieve_and_process(
        &mut self,
        recv_data: Option<ReceivedData>,
        desc: &mut NetworkDescription,
        sys_time: EtherCatSystemTime,
    ) {
        //log::info!("{:?}",self.state);
        let data = if let Some(recv_data) = recv_data {
            let ReceivedData { command, data, wkc } = recv_data;
            //if command != self.command {
            //    self.state = State::Error(EcError::UnexpectedCommand);
            //}
            if self.slave_address.is_some() && wkc != 1 {
                self.state = State::Error(EcError::UnexpectedWKC(wkc));
            } else if self.slave_address.is_none() && wkc != desc.len() as u16 {
                self.state = State::Error(EcError::UnexpectedWKC(wkc));
            }
            data
        } else {
            self.state = State::Error(EcError::LostCommand);
            return;
        };

        match self.state {
            State::Idle => {}
            State::Error(_) => {}
            State::Complete => {}
            State::Read => {
                let al_status = AlStatus(data);
                let al_state = AlState::from(al_status.state());
                self.current_al_state = al_state;
                if al_state == self.target_al {
                    self.state = State::Complete;
                } else if al_status.change_err() {
                    self.state = State::ResetError(al_state);
                } else {
                    self.state = State::Request;
                }
            }
            State::ResetError(_) => self.state = State::Read,
            State::Request => {
                self.timer_start = sys_time;
                self.state = State::Poll;
            }
            State::Poll => {
                let al_status = AlStatus(data);
                let al_state = AlState::from(al_status.state());
                self.current_al_state = al_state;
                if self.target_al == al_state {
                    self.state = State::Complete;
                } else if al_status.change_err() {
                    let al_status_code =
                        AlStatusCode::try_from(al_status.al_status_code()).unwrap();
                    self.state =
                        State::Error(Error::AlStatusCode((al_state, al_status_code)).into());
                } else if self.timer_start.0 < sys_time.0
                    && self.timeout_ms as u64 * 1000 < sys_time.0 - self.timer_start.0
                {
                    self.state = State::Error(Error::TimeoutMs(self.timeout_ms).into());
                }
            }
        }
    }
}

const fn buffer_size() -> usize {
    let mut size = 0;
    size = const_max(size, AlStatus::SIZE);
    size = const_max(size, AlControl::SIZE);
    size
}
