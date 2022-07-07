use super::mailbox::MailboxTaskError;
use super::sdo_downloader::SdoDownloader;
use super::sdo_uploader::SdoUploader;
use super::super::{CyclicProcess, EtherCatSystemTime, ReceivedData};
use crate::frame::AbortCode;
use crate::slave_network::SlaveInfo;
use crate::EcError;
use super::super::interface::*;

#[derive(Debug, Clone)]
pub enum SdoTaskError {
    Mailbox(MailboxTaskError),
    MailboxAlreadyExisted,
    AbortCode(AbortCode),
    UnexpectedCommandSpecifier,
}

impl From<SdoTaskError> for EcError<SdoTaskError> {
    fn from(err: SdoTaskError) -> Self {
        Self::TaskSpecific(err)
    }
}

impl From<EcError<MailboxTaskError>> for EcError<SdoTaskError> {
    fn from(err: EcError<MailboxTaskError>) -> Self {
        match err {
            EcError::TaskSpecific(err) => EcError::TaskSpecific(SdoTaskError::Mailbox(err)),
            EcError::Interface(e) => EcError::Interface(e),
            EcError::LostPacket => EcError::LostPacket,
            EcError::UnexpectedCommand => EcError::UnexpectedCommand,
            EcError::UnexpectedWkc(wkc) => EcError::UnexpectedWkc(wkc),
        }
    }
}

#[derive(Debug)]
enum State {
    Error(EcError<SdoTaskError>),
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
    Reader(SdoUploader<'a>),
    Writer(SdoDownloader<'a>),
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
pub struct SdoTask<'a> {
    state: State,
    inner: Inner<'a>,
}

impl<'a> SdoTask<'a> {
    pub fn new(mb_buf: &'a mut [u8]) -> Self {
        Self {
            state: State::Idle,
            inner: Inner::Reader(SdoUploader::new(mb_buf)),
        }
    }

    pub fn take_buffer(self) -> &'a mut [u8] {
        self.inner.take_buffer()
    }

    pub fn sdo_data(&self) -> &[u8] {
        match &self.inner {
            Inner::Reader(reader) => reader.sdo_data(),
            Inner::Writer(_) => &[],
            Inner::Taked => unreachable!(),
        }
    }

    pub fn start_to_read(&mut self, slave_info: &SlaveInfo, index: u16, sub_index: u8) {
        let inner = core::mem::take(&mut self.inner);
        let buf = inner.take_buffer();
        let mut reader = SdoUploader::new(buf);
        reader.start(slave_info, index, sub_index);
        self.inner = Inner::Reader(reader);
        self.state = State::Processing;
    }

    pub fn start_to_write(
        &mut self,
        slave_info: &SlaveInfo,
        index: u16,
        sub_index: u8,
        data: &[u8],
    ) {
        let inner = core::mem::take(&mut self.inner);
        let buf = inner.take_buffer();
        let mut writer = SdoDownloader::new(buf);
        writer.start(slave_info, index, sub_index, data);
        self.inner = Inner::Writer(writer);
        self.state = State::Processing;
    }

    pub fn wait<'b>(&'b self) -> Option<Result<(), EcError<SdoTaskError>>> {
        match &self.state {
            State::Complete => Some(Ok(())),
            State::Error(err) => Some(Err(err.clone())),
            _ => None,
        }
    }
}

impl<'a> CyclicProcess for SdoTask<'a> {
    fn next_command(&mut self, sys_time: EtherCatSystemTime) -> Option<(Command, &[u8])> {
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
                }
                Inner::Taked => unreachable!(),
            },
        }
    }
}
