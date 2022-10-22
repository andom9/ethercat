use super::mailbox::MailboxTaskError;
use super::TaskError;
use super::{CyclicTask, EtherCatSystemTime};
use crate::frame::{MailboxErrorResponse, MailboxHeader, MailboxType};
use crate::interface::*;
use crate::register::{SyncManagerActivation, SyncManagerPdiControl, SyncManagerStatus};
use crate::slave::SyncManager;

const MAILBOX_RESPONSE_RETRY_TIMEOUT_DEFAULT_MS: u32 = 2000;

#[derive(Debug, Clone, PartialEq)]
enum State {
    Error(TaskError<MailboxTaskError>),
    Idle,
    Complete,
    CheckMailboxFull((bool, bool)),
    Read,
    RequestRepeat,
    WaitRepeatAck,
}

impl Default for State {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug)]
pub struct MailboxReadTask {
    timer_start: EtherCatSystemTime,
    command: Command,
    slave_address: SlaveAddress,
    state: State,
    activation_buf: SyncManagerActivation<[u8; SyncManagerActivation::SIZE]>,
    sm_ado_offset: u16,
    sm_size: u16,
    sm_start_address: u16,
}

impl MailboxReadTask {
    pub fn new() -> Self {
        Self {
            timer_start: EtherCatSystemTime(0),
            command: Command::default(),
            slave_address: SlaveAddress::default(),
            state: State::Idle,
            activation_buf: SyncManagerActivation([0; SyncManagerActivation::SIZE]),
            sm_ado_offset: 0,
            sm_size: 0,
            sm_start_address: 0,
        }
    }

    pub fn mailbox_data<'a>(buf: &'a [u8]) -> (MailboxHeader<&'a [u8]>, &'a [u8]) {
        (
            MailboxHeader(&buf[..MailboxHeader::SIZE]),
            &buf[MailboxHeader::SIZE..],
        )
    }

    pub fn start(&mut self, slave_address: SlaveAddress, tx_sm: SyncManager, wait_full: bool) {
        self.timer_start = EtherCatSystemTime(0);
        self.command = Command::default();
        self.slave_address = slave_address;
        self.state = State::CheckMailboxFull((true, wait_full));

        self.sm_ado_offset = tx_sm.number() as u16 * 0x08;
        self.sm_size = tx_sm.size();
        self.sm_start_address = tx_sm.start_address();
    }

    pub fn wait(&self) -> Option<Result<(), TaskError<MailboxTaskError>>> {
        match &self.state {
            State::Complete => Some(Ok(())),
            State::Error(err) => Some(Err(err.clone())),
            _ => None,
        }
    }
}

impl CyclicTask for MailboxReadTask {
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
            State::Complete => None,
            State::CheckMailboxFull(_) => {
                self.command = Command::new_read(
                    self.slave_address.into(),
                    SyncManagerStatus::ADDRESS + self.sm_ado_offset,
                );
                let length = SyncManagerStatus::SIZE + SyncManagerActivation::SIZE;
                buf[..length].fill(0);
                Some((self.command, length))
            }
            State::Read => {
                self.command = Command::new_read(self.slave_address.into(), self.sm_start_address);
                if buf.len() < self.sm_size as usize {
                    self.state = State::Error(MailboxTaskError::BufferSmall.into());
                    None
                } else {
                    buf[..self.sm_size as usize].fill(0);
                    Some((self.command, self.sm_size as usize))
                }
            }
            State::RequestRepeat => {
                self.command = Command::new_write(
                    self.slave_address.into(),
                    SyncManagerActivation::ADDRESS + self.sm_ado_offset,
                );
                self.activation_buf
                    .set_repeat(!self.activation_buf.repeat()); //toggle
                buf[..self.activation_buf.0.len()].copy_from_slice(&self.activation_buf.0);
                Some((self.command, self.activation_buf.0.len()))
            }
            State::WaitRepeatAck => {
                self.command = Command::new_read(
                    self.slave_address.into(),
                    SyncManagerPdiControl::ADDRESS + self.sm_ado_offset,
                );
                buf[..SyncManagerPdiControl::SIZE].fill(0);
                Some((self.command, SyncManagerPdiControl::SIZE))
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
            State::CheckMailboxFull((is_first, wait_full)) => {
                if is_first {
                    self.timer_start = sys_time;
                }
                if wkc != 1 {
                    self.state = State::Error(MailboxTaskError::NoSlaveReaction.into());
                } else {
                    let status = SyncManagerStatus(data);
                    self.activation_buf
                        .0
                        .copy_from_slice(&data[SyncManagerStatus::SIZE..]);
                    if status.is_mailbox_full() {
                        self.state = State::Read;
                    } else if wait_full {
                        self.state = State::CheckMailboxFull((false, wait_full));
                    } else {
                        self.state = State::Error(MailboxTaskError::MailboxEmpty.into());
                    }
                }
            }
            State::Read => {
                // mailbox lost
                if wkc != 1 {
                    self.state = State::RequestRepeat;
                } else {
                    let header = MailboxHeader(&data);
                    if header.mailbox_type() == MailboxType::Error as u8 {
                        let mut err = MailboxErrorResponse::new();
                        err.0.copy_from_slice(&data[..4]);
                        self.state = State::Error(MailboxTaskError::ErrorResponse(err).into());
                    } else {
                        self.state = State::Complete;
                    }
                }
            }
            State::RequestRepeat => {
                self.state = State::WaitRepeatAck;
            }
            State::WaitRepeatAck => {
                if wkc != 1 {
                    self.state = State::Error(TaskError::UnexpectedWkc((1, wkc).into()));
                } else if SyncManagerPdiControl(data).repeat_ack() == self.activation_buf.repeat() {
                    self.state = State::CheckMailboxFull((false, true));
                } else {
                    self.state = State::WaitRepeatAck;
                }
            }
        }

        // check timeout
        let timeout_ns = (MAILBOX_RESPONSE_RETRY_TIMEOUT_DEFAULT_MS as u64) * 1000 * 1000;
        if self.timer_start.0 < sys_time.0 && timeout_ns < sys_time.0 - self.timer_start.0 {
            self.state = State::Error(TaskError::Timeout);
        }
    }
}
