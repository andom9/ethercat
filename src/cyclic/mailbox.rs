use super::mailbox_reader::MailboxReader;
use super::mailbox_writer::MailboxWriter;
use super::{CyclicProcess, EtherCatSystemTime, ReceivedData};
use crate::interface::SlaveAddress;
use crate::packet::ethercat::{MailboxErrorResponse, MailboxHeader};
use crate::slave::{self, SlaveInfo, SyncManager};
use crate::{error::EcError, interface::Command};

#[derive(Debug, Clone)]
pub enum Error {
    TimeoutMs(u32),
    //NoMailbox,
    MailboxNotAvailable,
    //NoSlave,
    MailboxEmpty,
    MailboxFull,
    BufferSmall,
    ErrorResponse(MailboxErrorResponse<[u8; MailboxErrorResponse::SIZE]>),
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
    Complete,
    Processing,
}

impl Default for State {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug)]
enum Inner<'a> {
    Reader(MailboxReader<'a>),
    Writer(MailboxWriter<'a>),
    Taked,
}

impl<'a> Inner<'a> {
    fn take_buffer(self) -> &'a mut [u8] {
        match self {
            Inner::Reader(reader) => reader.take_buffer(),
            Inner::Writer(writer) => writer.take_buffer(),
            Inner::Taked => unreachable!(),
        }
    }
}

impl<'a> Default for Inner<'a> {
    fn default() -> Self {
        Inner::Taked
    }
}

#[derive(Debug)]
pub struct MailboxUnit<'a> {
    state: State,
    inner: Inner<'a>,
}

impl<'a> MailboxUnit<'a> {
    pub fn new(mb_buf: &'a mut [u8]) -> Self {
        Self {
            state: State::Idle,
            inner: Inner::Reader(MailboxReader::new(mb_buf)),
        }
    }

    pub fn take_buffer(self) -> &'a mut [u8] {
        self.inner.take_buffer()
    }

    pub fn mailbox_header(&self) -> MailboxHeader<&[u8]> {
        match &self.inner {
            Inner::Reader(reader) => reader.mailbox_header(),
            Inner::Writer(writer) => writer.mailbox_header(),
            Inner::Taked => unreachable!(),
        }
    }

    pub fn mailbox_header_mut(&mut self) -> MailboxHeader<&mut [u8]> {
        match &mut self.inner {
            Inner::Reader(reader) => reader.mailbox_header_mut(),
            Inner::Writer(writer) => writer.mailbox_header_mut(),
            Inner::Taked => unreachable!(),
        }
    }

    pub fn mailbox_data(&self) -> &[u8] {
        match &self.inner {
            Inner::Reader(reader) => reader.mailbox_data(),
            Inner::Writer(writer) => writer.mailbox_data(),
            Inner::Taked => unreachable!(),
        }
    }

    pub fn mailbox_data_mut(&mut self) -> &mut [u8] {
        match &mut self.inner {
            Inner::Reader(reader) => reader.mailbox_data_mut(),
            Inner::Writer(writer) => writer.mailbox_data_mut(),
            Inner::Taked => unreachable!(),
        }
    }

    pub fn start_to_read(
        &mut self,
        slave_address: SlaveAddress,
        tx_sm: SyncManager,
        wait_full: bool,
    ) {
        let inner = core::mem::take(&mut self.inner);
        let buf = inner.take_buffer();
        let mut reader = MailboxReader::new(buf);
        reader.start(slave_address, tx_sm, wait_full);
        self.inner = Inner::Reader(reader);
        self.state = State::Processing;
    }

    pub fn start_to_write(
        &mut self,
        slave_address: SlaveAddress,
        rx_sm: SyncManager,
        wait_full: bool,
    ) {
        let inner = core::mem::take(&mut self.inner);
        let buf = inner.take_buffer();
        let mut writer = MailboxWriter::new(buf);
        writer.start(slave_address, rx_sm, wait_full);
        self.inner = Inner::Writer(writer);
        self.state = State::Processing;
    }

    pub fn wait<'b>(&'b self) -> Option<Result<(), EcError<Error>>> {
        match &self.state {
            State::Complete => Some(Ok(())),
            State::Error(err) => Some(Err(err.clone())),
            _ => None,
        }
    }
}

impl<'a> CyclicProcess for MailboxUnit<'a> {
    fn next_command(
        &mut self,
        //desc: &mut NetworkDescription,
        sys_time: EtherCatSystemTime,
    ) -> Option<(Command, &[u8])> {
        match self.state {
            State::Idle => None,
            State::Error(_) => None,
            State::Complete => None,
            State::Processing => match &mut self.inner {
                Inner::Reader(reader) => reader.next_command(sys_time),
                Inner::Writer(writer) => writer.next_command(sys_time),
                Inner::Taked => unreachable!(),
            },
        }
    }

    fn recieve_and_process(
        &mut self,
        recv_data: Option<ReceivedData>,
        //desc: &mut NetworkDescription,
        sys_time: EtherCatSystemTime,
    ) {
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
                        Some(Err(err)) => self.state = State::Error(err.into()),
                    }
                }
                Inner::Writer(writer) => {
                    writer.recieve_and_process(recv_data, sys_time);
                    match writer.wait() {
                        None => {}
                        Some(Ok(_)) => self.state = State::Complete,
                        Some(Err(err)) => self.state = State::Error(err.into()),
                    }
                }
                Inner::Taked => unreachable!(),
            },
        }
    }
}
