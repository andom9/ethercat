use super::mailbox_read::MailboxReadTask;
use super::mailbox_write::MailboxWriteTask;
use super::TaskError;
use super::{CyclicTask, EtherCatSystemTime};
use crate::frame::{MailboxErrorFrame, MailboxFrame};
use crate::interface::*;
use crate::slave::SyncManager;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MailboxTaskError {
    NoSlaveReaction,
    MailboxEmpty,
    MailboxFull,
    BufferSmall,
    ErrorResponse(MailboxErrorFrame<[u8; MailboxErrorFrame::SIZE]>),
}

impl From<MailboxTaskError> for TaskError<MailboxTaskError> {
    fn from(err: MailboxTaskError) -> Self {
        Self::TaskSpecific(err)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum State {
    Error(TaskError<MailboxTaskError>),
    Idle,
    Complete,
    Processing,
}

impl Default for State {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug)]
enum Inner {
    Reader(MailboxReadTask),
    Writer(MailboxWriteTask),
}

#[derive(Debug)]
pub struct MailboxTask {
    state: State,
    inner: Inner,
}

impl MailboxTask {
    pub fn new() -> Self {
        Self {
            state: State::Idle,
            inner: Inner::Reader(MailboxReadTask::new()),
        }
    }

    pub fn set_mailbox_data(
        mb_header: &MailboxFrame<[u8; MailboxFrame::HEADER_SIZE]>,
        mb_data: &[u8],
        buf: &mut [u8],
    ) {
        MailboxWriteTask::set_mailbox_data(mb_header, mb_data, buf);
    }

    pub fn mailbox_data<'a>(buf: &'a [u8]) -> (MailboxFrame<&'a [u8]>, &'a [u8]) {
        MailboxReadTask::mailbox_data(buf)
    }

    pub fn start_to_read(
        &mut self,
        slave_address: SlaveAddress,
        tx_sm: SyncManager,
        wait_full: bool,
    ) {
        let mut reader = MailboxReadTask::new();
        reader.start(slave_address, tx_sm, wait_full);
        self.inner = Inner::Reader(reader);
        self.state = State::Processing;
    }

    pub fn start_to_write(
        &mut self,
        slave_address: SlaveAddress,
        rx_sm: SyncManager,
        wait_empty: bool,
    ) {
        let mut writer = MailboxWriteTask::new();
        writer.start(slave_address, rx_sm, wait_empty);
        self.inner = Inner::Writer(writer);
        self.state = State::Processing;
    }

    pub fn is_write_mode(&self) -> bool {
        if let Inner::Writer(_) = self.inner {
            true
        } else {
            false
        }
    }

    pub fn is_read_mode(&self) -> bool {
        if let Inner::Reader(_) = self.inner {
            true
        } else {
            false
        }
    }

    pub fn slave_address(&self) -> SlaveAddress {
        match &self.inner {
            Inner::Reader(reader) => reader.slave_address(),
            Inner::Writer(writer) => writer.slave_address(),
        }
    }

    pub fn wait<'b>(&'b self) -> Option<Result<(), TaskError<MailboxTaskError>>> {
        match &self.state {
            State::Complete => Some(Ok(())),
            State::Error(err) => Some(Err(err.clone())),
            _ => None,
        }
    }
}

impl CyclicTask for MailboxTask {
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
            State::Processing => match &mut self.inner {
                Inner::Reader(reader) => reader.next_pdu(buf),
                Inner::Writer(writer) => writer.next_pdu(buf),
            },
        }
    }

    fn recieve_and_process(&mut self, recv_data: &Pdu, sys_time: EtherCatSystemTime) {
        match self.state {
            State::Idle => {}
            State::Error(_) => {}
            State::Complete => {}
            State::Processing => match &mut self.inner {
                Inner::Reader(reader) => {
                    reader.recieve_and_process(recv_data, sys_time);
                    match reader.wait() {
                        None => {}
                        Some(Ok(_)) => self.state = State::Complete,
                        Some(Err(err)) => self.state = State::Error(err),
                    }
                }
                Inner::Writer(writer) => {
                    writer.recieve_and_process(recv_data, sys_time);
                    match writer.wait() {
                        None => {}
                        Some(Ok(_)) => self.state = State::Complete,
                        Some(Err(err)) => self.state = State::Error(err),
                    }
                }
            },
        }
    }
}
