use super::EtherCatSystemTime;
use super::ReceivedData;
use crate::cyclic::Cyclic;
use crate::error::CommonError;
use crate::interface::{Command, SlaveAddress};
use crate::network::NetworkDescription;
use crate::register::application::{AlControl, AlStatus};
use crate::slave::AlState;
use crate::util::const_max;
use nb;

// Timeout. Init -> PreOp or Init -> Boot
const PREOP_TIMEOUT_DEFAULT_MS: u32 = 3000;
// Timeout. SafeOp -> Op or PreOp -> SafeOp
const SAFEOP_OP_TIMEOUT_DEFAULT_MS: u32 = 10000;
// Timeout. Op/SafeOp/PreOp/Boot -> Init or SafeOp -> PreOp
const BACK_TO_INIT_TIMEOUT_DEFAULT_MS: u32 = 5000;
// Timeout. Op -> SafeOp
const BACK_TO_SAFEOP_TIMEOUT_DEFAULT_MS: u32 = 200;

#[derive(Debug, Clone)]
pub enum Error {
    Common(CommonError),
    TimeoutMs(u32),
    AlStatusCode(AlStatusCode),
    InvalidAlState,
}

impl From<CommonError> for Error {
    fn from(err: CommonError) -> Self {
        Self::Common(err)
    }
}

#[derive(Debug)]
enum State {
    Error(Error),
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
    slave_address: SlaveAddress,
    target_al: Option<AlState>,
    command: Command,
    buffer: [u8; buffer_size()],
    current_al_state: Option<AlState>,
    timeout_ms: u32,
}

impl AlStateTransfer {
    pub fn new() -> Self {
        Self {
            timer_start: EtherCatSystemTime(0),
            state: State::Idle,
            slave_address: SlaveAddress::SlavePosition(0),
            target_al: None,
            command: Command::default(),
            buffer: [0; buffer_size()],
            current_al_state: None,
            timeout_ms: 0,
        }
    }

    pub fn start(&mut self, slave_address: SlaveAddress, target_al_state: Option<AlState>) {
        self.slave_address = slave_address;
        self.target_al = target_al_state;
        self.state = State::Read;
        self.buffer.fill(0);
        self.command = Command::default();
    }

    pub fn wait(&mut self) -> nb::Result<AlState, Error> {
        match &self.state {
            State::Complete => Ok(self.current_al_state.unwrap()),
            State::Error(err) => Err(nb::Error::Other(err.clone())),
            _ => Err(nb::Error::WouldBlock),
        }
    }
}

impl Cyclic for AlStateTransfer {
    fn next_command(
        &mut self,
        _: &mut NetworkDescription,
        _: EtherCatSystemTime,
    ) -> Option<(Command, &[u8])> {
        match self.state {
            State::Idle => None,
            State::Error(_) => None,
            State::Read => {
                self.command = Command::new_read(self.slave_address, AlStatus::ADDRESS);
                self.buffer.fill(0);
                Some((self.command, &self.buffer[..AlStatus::SIZE]))
            }
            State::ResetError(current_al_state) => {
                self.command = Command::new_write(self.slave_address, AlControl::ADDRESS);
                self.buffer.fill(0);
                let mut al_control = AlControl(self.buffer);
                al_control.set_state(current_al_state as u8);
                al_control.set_acknowledge(true);
                Some((self.command, &self.buffer[..AlControl::SIZE]))
            }
            State::Request => {
                self.buffer.fill(0);
                let mut al_control = AlControl(self.buffer);
                let target_al = self.target_al.unwrap();
                al_control.set_state(target_al as u8);
                self.command = Command::new_write(self.slave_address, AlControl::ADDRESS);
                self.timeout_ms = match (self.current_al_state.unwrap(), target_al) {
                    (AlState::PreOperational, AlState::SafeOperational)
                    | (_, AlState::Operational) => SAFEOP_OP_TIMEOUT_DEFAULT_MS,
                    (_, AlState::PreOperational) | (_, AlState::Bootstrap) => {
                        PREOP_TIMEOUT_DEFAULT_MS
                    }
                    (_, AlState::Init) => BACK_TO_INIT_TIMEOUT_DEFAULT_MS,
                    (_, AlState::SafeOperational) => BACK_TO_SAFEOP_TIMEOUT_DEFAULT_MS,
                    (_, AlState::Invalid) => unreachable!(),
                };
                Some((self.command, &self.buffer[..AlControl::SIZE]))
            }
            State::Poll => {
                self.command = Command::new_read(self.slave_address, AlStatus::ADDRESS);
                self.buffer.fill(0);
                Some((self.command, &self.buffer[..AlStatus::SIZE]))
            }
            State::Complete => None,
        }
    }

    fn recieve_and_process(
        &mut self,
        recv_data: Option<ReceivedData>,
        _: &mut NetworkDescription,
        sys_time: EtherCatSystemTime,
    ) {
        let data = if let Some(recv_data) = recv_data {
            let ReceivedData { command, data, wkc } = recv_data;
            if command != self.command {
                self.state = State::Error(Error::Common(CommonError::BadPacket));
            }
            if wkc != 1 {
                self.state = State::Error(Error::Common(CommonError::UnexpectedWKC(wkc)));
            }
            data
        } else {
            self.state = State::Error(Error::Common(CommonError::LostCommand));
            return;
        };

        match self.state {
            State::Idle => {}
            State::Error(_) => {}
            State::Complete => {}
            State::Read => {
                let al_status = AlStatus(data);
                let al_state = AlState::from(al_status.state());
                self.current_al_state = Some(al_state);
                if let Some(target_al) = self.target_al {
                    if target_al == al_state {
                        self.state = State::Complete;
                    } else if al_state == AlState::Invalid {
                        self.state = State::Error(Error::InvalidAlState);
                    } else if al_status.change_err() {
                        self.state = State::ResetError(al_state);
                    } else {
                        self.state = State::Request;
                    }
                } else {
                    self.state = State::Complete;
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
                self.current_al_state = Some(al_state);
                if self.target_al.unwrap() == al_state {
                    self.state = State::Complete;
                } else if al_state == AlState::Invalid {
                    self.state = State::Error(Error::InvalidAlState);
                } else if al_status.change_err() {
                    self.state = State::ResetError(al_state);
                } else if self.timer_start.0 < sys_time.0
                    && self.timeout_ms as u64 * 1000 < sys_time.0 - self.timer_start.0
                {
                    self.state = State::Error(Error::TimeoutMs(self.timeout_ms));
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

//TODO
#[derive(Debug, Clone)]
pub enum AlStatusCode {
    NoError = 0,
    InvalidInputConfig = 0x001E,
}
