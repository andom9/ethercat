use super::super::interface::*;
use super::super::{CommandData, Cyclic, EtherCatSystemTime};
use super::mailbox_reader::MailboxReader;
use super::mailbox_writer::MailboxWriter;
use crate::frame::{MailboxErrorResponse, MailboxHeader};
use crate::slave_network::SyncManager;
use crate::EcError;

#[derive(Debug, Clone, PartialEq)]
pub enum MailboxTaskError {
    Timeout,
    MailboxNotAvailable,
    MailboxEmpty,
    MailboxFull,
    BufferSmall,
    ErrorResponse(MailboxErrorResponse<[u8; MailboxErrorResponse::SIZE]>),
}

impl From<MailboxTaskError> for EcError<MailboxTaskError> {
    fn from(err: MailboxTaskError) -> Self {
        Self::TaskSpecific(err)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum State {
    Error(EcError<MailboxTaskError>),
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
    Reader(MailboxReader),
    Writer(MailboxWriter),
    //Taked,
}

//impl<'a> Inner<'a> {
//    fn take_buffer(self) -> &'a mut [u8] {
//        match self {
//            Inner::Reader(reader) => reader.take_buffer(),
//            Inner::Writer(writer) => writer.take_buffer(),
//            Inner::Taked => unreachable!(),
//        }
//    }
//}

//impl<'a> Default for Inner<'a> {
//    fn default() -> Self {
//        Inner::Taked
//    }
//}

#[derive(Debug)]
pub struct MailboxTask {
    state: State,
    inner: Inner,
}

impl MailboxTask {
    pub fn new() -> Self {
        Self {
            state: State::Idle,
            inner: Inner::Reader(MailboxReader::new()),
        }
    }

    //pub fn take_buffer(self) -> &'a mut [u8] {
    //    self.inner.take_buffer()
    //}

    //pub fn mailbox_header(&self) -> MailboxHeader<&[u8]> {
    //    match &self.inner {
    //        Inner::Reader(reader) => reader.mailbox_header(),
    //        Inner::Writer(writer) => writer.mailbox_header(),
    //        //Inner::Taked => unreachable!(),
    //    }
    //}

    //pub fn mailbox_header_mut(&mut self) -> MailboxHeader<&mut [u8]> {
    //    match &mut self.inner {
    //        Inner::Reader(reader) => reader.mailbox_header_mut(),
    //        Inner::Writer(writer) => writer.mailbox_header_mut(),
    //        //Inner::Taked => unreachable!(),
    //    }
    //}

    //pub fn mailbox_data(&self) -> &[u8] {
    //    match &self.inner {
    //        Inner::Reader(reader) => reader.mailbox_data(),
    //        Inner::Writer(writer) => writer.mailbox_data(),
    //        //Inner::Taked => unreachable!(),
    //    }
    //}

    //pub fn mailbox_data_mut(&mut self) -> &mut [u8] {
    //    match &mut self.inner {
    //        Inner::Reader(reader) => reader.mailbox_data_mut(),
    //        Inner::Writer(writer) => writer.mailbox_data_mut(),
    //        //Inner::Taked => unreachable!(),
    //    }
    //}

    pub fn set_mailbox_data(mb_header: &[u8; MailboxHeader::SIZE], mb_data: &[u8], buf: &mut [u8]) {
        MailboxWriter::set_mailbox_data(mb_header, mb_data, buf);
    }

    pub fn mailbox_data<'a>(buf: &'a [u8]) -> (MailboxHeader<&'a [u8]>, &'a [u8]) {
        MailboxReader::mailbox_data(buf)
    }

    pub fn start_to_read(
        &mut self,
        slave_address: SlaveAddress,
        tx_sm: SyncManager,
        wait_full: bool,
    ) {
        //let inner = core::mem::take(&mut self.inner);
        //let buf = inner.take_buffer();
        let mut reader = MailboxReader::new();
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
        //let inner = core::mem::take(&mut self.inner);
        //let buf = inner.take_buffer();
        let mut writer = MailboxWriter::new();
        writer.start(slave_address, rx_sm, wait_full);
        self.inner = Inner::Writer(writer);
        self.state = State::Processing;
    }

    pub fn wait<'b>(&'b self) -> Option<Result<(), EcError<MailboxTaskError>>> {
        match &self.state {
            State::Complete => Some(Ok(())),
            State::Error(err) => Some(Err(err.clone())),
            _ => None,
        }
    }
}

impl Cyclic for MailboxTask {
    fn is_finished(&self) -> bool {
        match self.state {
            State::Complete | State::Error(_) => true,
            _ => false,
        }
    }

    fn next_command(&mut self, buf: &mut [u8]) -> Option<(Command, usize)> {
        match self.state {
            State::Idle => None,
            State::Error(_) => None,
            State::Complete => None,
            State::Processing => match &mut self.inner {
                Inner::Reader(reader) => reader.next_command(buf),
                Inner::Writer(writer) => writer.next_command(buf),
                //Inner::Taked => unreachable!(),
            },
        }
    }

    fn recieve_and_process(
        &mut self,
        recv_data: Option<&CommandData>,
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
                } //Inner::Taked => unreachable!(),
            },
        }
    }
}
