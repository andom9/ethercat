use super::mailbox::MailboxTaskError;
use super::TaskError;
use super::{CyclicTask, EtherCatSystemTime};
use crate::interface::*;
use crate::register::{SyncManagerActivation, SyncManagerStatus};
use crate::slave::SyncManager;

const MAILBOX_REQUEST_RETRY_TIMEOUT_DEFAULT_MS: u32 = 100;

#[derive(Debug, Clone, PartialEq)]
enum State {
    Error(TaskError<MailboxTaskError>),
    Idle,
    Complete,
    CheckMailboxEmpty((bool, bool)),
    Write,
}

impl Default for State {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug)]
pub struct MailboxWriteTask {
    timer_start: EtherCatSystemTime,
    command: Command,
    slave_address: SlaveAddress,
    empty_check_buffer: [u8; SyncManagerStatus::SIZE + SyncManagerActivation::SIZE],
    state: State,
    sm_ado_offset: u16,
    sm_size: u16,
    sm_start_address: u16,
}

impl MailboxWriteTask {
    pub fn new() -> Self {
        Self {
            timer_start: EtherCatSystemTime(0),
            command: Command::default(),
            slave_address: SlaveAddress::default(),
            empty_check_buffer: [0; SyncManagerStatus::SIZE + SyncManagerActivation::SIZE],
            state: State::Idle,
            sm_ado_offset: 0,
            sm_size: 0,
            sm_start_address: 0,
        }
    }

    pub fn slave_address(&self) -> SlaveAddress {
        self.slave_address
    }

    pub fn start(&mut self, slave_address: SlaveAddress, rx_sm: SyncManager, wait_empty: bool) {
        self.timer_start = EtherCatSystemTime(0);
        self.command = Command::default();
        self.slave_address = slave_address;
        self.state = State::CheckMailboxEmpty((true, wait_empty));

        self.sm_ado_offset = rx_sm.number() as u16 * 0x08;
        self.sm_size = rx_sm.size();
        self.sm_start_address = rx_sm.start_address();
    }

    pub fn wait(&self) -> Option<Result<(), TaskError<MailboxTaskError>>> {
        match &self.state {
            State::Complete => Some(Ok(())),
            State::Error(err) => Some(Err(err.clone())),
            _ => None,
        }
    }
}

impl CyclicTask for MailboxWriteTask {
    fn is_finished(&self) -> bool {
        match self.state {
            State::Idle | State::Complete | State::Error(_) => true,
            _ => false,
        }
    }

    fn next_pdu(&mut self, buf: &mut [u8]) -> Option<(Command, usize)> {
        match self.state {
            State::Idle => None,
            State::Error(_) => None,
            State::Complete => None,
            State::CheckMailboxEmpty(_) => {
                self.command = Command::new_read(
                    self.slave_address.into(),
                    SyncManagerStatus::ADDRESS + self.sm_ado_offset,
                );
                let length = SyncManagerStatus::SIZE + SyncManagerActivation::SIZE;
                self.empty_check_buffer
                    .iter_mut()
                    .zip(buf.iter())
                    .for_each(|(b, sb)| *b = *sb);
                buf[..length].fill(0);
                Some((self.command, length))
            }
            State::Write => {
                self.command = Command::new_write(self.slave_address.into(), self.sm_start_address);
                buf.iter_mut()
                    .zip(self.empty_check_buffer.iter())
                    .for_each(|(sb, b)| *sb = *b);
                if buf.len() < self.sm_size as usize {
                    self.state = State::Error(MailboxTaskError::BufferSmall.into());
                    None
                } else {
                    Some((self.command, self.sm_size as usize))
                }
            }
        }
    }

    fn recieve_and_process(&mut self, recv_data: &Pdu, sys_time: EtherCatSystemTime) {
        let Pdu { command, data, wkc } = recv_data;
        if !(command.c_type == self.command.c_type && command.ado == self.command.ado) {
            self.state = State::Error(TaskError::UnexpectedCommand);
        }
        let wkc = *wkc;
        match self.state {
            State::Idle => {}
            State::Error(_) => {}
            State::Complete => {}
            State::CheckMailboxEmpty((is_first, wait_empty)) => {
                if is_first {
                    self.timer_start = sys_time;
                }
                if wkc != 1 {
                    self.state = State::Error(MailboxTaskError::NoSlaveReaction.into());
                } else {
                    let status = SyncManagerStatus(data);
                    if !status.is_mailbox_full() {
                        self.state = State::Write;
                    } else if wait_empty {
                        self.state = State::CheckMailboxEmpty((false, wait_empty));
                    } else {
                        self.state = State::Error(MailboxTaskError::MailboxAlreadyExisted.into());
                    }
                }
            }
            State::Write => {
                // mailbox lost
                if wkc != 1 {
                    self.state = State::Write;
                } else {
                    self.state = State::Complete;
                }
            }
        }

        // check timeout
        let timeout_ns = (MAILBOX_REQUEST_RETRY_TIMEOUT_DEFAULT_MS as u64) * 1000 * 1000;
        if self.timer_start.0 < sys_time.0 && timeout_ns < sys_time.0 - self.timer_start.0 {
            self.state = State::Error(TaskError::Timeout);
        }
    }
}
