use super::TaskError;
use super::{CyclicTask, EtherCatSystemTime};
use crate::interface::*;
use crate::register::AlStatusCode;
use crate::register::SiiAccess;
use crate::register::{AlControl, AlStatus};
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlStateTransferTaskError {
    AlStatusCode((AlState, AlStatusCode)),
}

impl From<AlStateTransferTaskError> for TaskError<AlStateTransferTaskError> {
    fn from(err: AlStateTransferTaskError) -> Self {
        Self::TaskSpecific(err)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum State {
    Error(TaskError<AlStateTransferTaskError>),
    Idle,
    Read,
    ResetError(AlState),
    OffAck(AlState),
    ResetSiiOwnership,
    Request,
    Poll,
    Complete,
}

#[derive(Debug)]
pub struct AlStateTransferTask {
    timer_start: EtherCatSystemTime,
    state: State,
    slave_address: TargetSlave,
    target_al: AlState,
    command: Command,
    current_al_state: AlState,
    timeout_ms: u32,
}

impl AlStateTransferTask {
    pub const fn required_buffer_size() -> usize {
        buffer_size()
    }

    pub fn new() -> Self {
        Self {
            timer_start: EtherCatSystemTime(0),
            state: State::Idle,
            slave_address: TargetSlave::default(),
            target_al: AlState::Init,
            command: Command::default(),
            current_al_state: AlState::Init,
            timeout_ms: 0,
        }
    }

    pub fn start(&mut self, slave_address: TargetSlave, target_al_state: AlState) {
        self.slave_address = slave_address;
        self.target_al = target_al_state;
        self.state = State::Read;
        self.command = Command::default();
    }

    pub fn wait(&mut self) -> Option<Result<AlState, TaskError<AlStateTransferTaskError>>> {
        match &self.state {
            State::Complete => Some(Ok(self.current_al_state)),
            State::Error(err) => Some(Err(err.clone())),
            _ => None,
        }
    }
}

impl CyclicTask for AlStateTransferTask {
    fn is_finished(&self) -> bool {
        match self.state {
            State::Complete | State::Error(_) => true,
            _ => false,
        }
    }

    fn next_pdu(&mut self, buf: &mut [u8]) -> Option<(Command, usize)> {
        match self.state {
            State::Idle => None,
            State::Error(_) => None,
            State::Read => {
                self.command = Command::new_read(self.slave_address, AlStatus::ADDRESS);

                buf[..AlStatus::SIZE].fill(0);
                Some((self.command, AlStatus::SIZE))
            }
            State::ResetError(current_al_state) => {
                self.command = Command::new_write(self.slave_address, AlControl::ADDRESS);

                buf[..AlStatus::SIZE].fill(0);
                let mut al_control = AlControl(buf);
                al_control.set_state(current_al_state as u8);
                al_control.set_acknowledge(true);
                Some((self.command, AlControl::SIZE))
            }
            State::OffAck(current_al_state) => {
                self.command = Command::new_write(self.slave_address, AlControl::ADDRESS);

                buf[..AlControl::SIZE].fill(0);
                let mut al_control = AlControl(buf);
                al_control.set_state(current_al_state as u8);
                al_control.set_acknowledge(false);
                Some((self.command, AlControl::SIZE))
            }
            State::ResetSiiOwnership => {
                buf[..SiiAccess::SIZE].fill(0);
                let mut sii_access = SiiAccess(buf);
                sii_access.set_owner(true);
                sii_access.set_reset_access(false);
                self.command = Command::new_write(self.slave_address, SiiAccess::ADDRESS);

                Some((self.command, SiiAccess::SIZE))
            }
            State::Request => {
                buf[..AlControl::SIZE].fill(0);
                let mut al_control = AlControl(buf);
                let target_al = self.target_al;
                al_control.set_state(target_al as u8);
                self.command = Command::new_write(self.slave_address, AlControl::ADDRESS);

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
                Some((self.command, AlControl::SIZE))
            }
            State::Poll => {
                self.command = Command::new_read(self.slave_address, AlStatus::ADDRESS);
                buf[..AlStatus::SIZE].fill(0);
                Some((self.command, AlStatus::SIZE))
            }
            State::Complete => None,
        }
    }

    fn recieve_and_process(&mut self, recv_data: &Pdu, sys_time: EtherCatSystemTime) {
        let data = {
            let Pdu { command, data, wkc } = recv_data;
            let wkc = *wkc;
            if !(command.c_type == self.command.c_type && command.ado == self.command.ado) {
                self.state = State::Error(TaskError::UnexpectedCommand);
            }
            match self.slave_address {
                TargetSlave::Single(_slave_address) => {
                    if wkc != 1 {
                        self.state = State::Error(TaskError::UnexpectedWkc((1, wkc).into()));
                    }
                }
                TargetSlave::All(num_slaves) => {
                    if wkc != num_slaves {
                        self.state =
                            State::Error(TaskError::UnexpectedWkc((num_slaves, wkc).into()));
                    }
                }
            }
            data
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
                    let non_mixed_al_state = match al_state {
                        AlState::InvalidOrMixed => AlState::Init,
                        _ => al_state,
                    };
                    self.state = State::ResetError(non_mixed_al_state);
                } else {
                    self.state = State::ResetSiiOwnership;
                }
            }
            State::ResetError(al_state) => self.state = State::OffAck(al_state),
            State::OffAck(_) => self.state = State::Read,
            State::ResetSiiOwnership => self.state = State::Request,
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
                    self.state = State::Error(
                        AlStateTransferTaskError::AlStatusCode((al_state, al_status_code)).into(),
                    );
                } else if self.timer_start.0 < sys_time.0
                    && self.timeout_ms as u64 * 1000 < sys_time.0 - self.timer_start.0
                {
                    self.state = State::Error(TaskError::Timeout);
                }
            }
        }
    }
}

const fn buffer_size() -> usize {
    let mut size = 0;
    size = const_max(size, AlStatus::SIZE);
    size = const_max(size, AlControl::SIZE);
    size = const_max(size, SiiAccess::SIZE);
    size
}
