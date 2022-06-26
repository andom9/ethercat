use super::sdo_downloader::SdoDownloader;
use super::sdo_uploader::SdoUploader;
use super::{mailbox, CyclicProcess, EtherCatSystemTime, ReceivedData};
use crate::network::NetworkDescription;
use crate::packet::coe::AbortCode;
use crate::{
    error::EcError,
    interface::{Command, SlaveAddress},
};

#[derive(Debug, Clone)]
pub enum Error {
    Mailbox(mailbox::Error),
    MailboxAlreadyExisted,
    AbortCode(AbortCode),
    UnexpectedCommandSpecifier,
}

impl From<Error> for EcError<Error> {
    fn from(err: Error) -> Self {
        Self::UnitSpecific(err)
    }
}

impl From<EcError<mailbox::Error>> for EcError<Error> {
    fn from(err: EcError<mailbox::Error>) -> Self {
        match err {
            EcError::UnitSpecific(err) => EcError::UnitSpecific(Error::Mailbox(err)),
            EcError::Interface(e) => EcError::Interface(e),
            EcError::LostCommand => EcError::LostCommand,
            EcError::UnexpectedCommand => EcError::UnexpectedCommand,
            EcError::UnexpectedWKC(wkc) => EcError::UnexpectedWKC(wkc),
        }
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
pub struct SdoUnit<'a> {
    state: State,
    inner: Inner<'a>,
}

impl<'a> SdoUnit<'a> {
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

    pub fn start_to_read(&mut self, slave_address: SlaveAddress, index: u16, sub_index: u8) {
        let inner = core::mem::take(&mut self.inner);
        let buf = inner.take_buffer();
        let mut reader = SdoUploader::new(buf);
        reader.start(slave_address, index, sub_index);
        self.inner = Inner::Reader(reader);
        self.state = State::Processing;
    }

    pub fn start_to_write(
        &mut self,
        slave_address: SlaveAddress,
        index: u16,
        sub_index: u8,
        data: &[u8],
    ) {
        let inner = core::mem::take(&mut self.inner);
        let buf = inner.take_buffer();
        let mut writer = SdoDownloader::new(buf);
        writer.start(slave_address, index, sub_index, data);
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

impl<'a> CyclicProcess for SdoUnit<'a> {
    fn next_command(
        &mut self,
        desc: &mut NetworkDescription,
        sys_time: EtherCatSystemTime,
    ) -> Option<(Command, &[u8])> {
        match self.state {
            State::Idle => None,
            State::Error(_) => None,
            State::Complete => None,
            State::Processing => match &mut self.inner {
                Inner::Reader(reader) => reader.next_command(desc, sys_time),
                Inner::Writer(writer) => writer.next_command(desc, sys_time),
                Inner::Taked => unreachable!(),
            },
        }
    }

    fn recieve_and_process(
        &mut self,
        recv_data: Option<ReceivedData>,
        desc: &mut NetworkDescription,
        sys_time: EtherCatSystemTime,
    ) {
        match self.state {
            State::Idle => {}
            State::Error(_) => {}
            State::Complete => {}
            State::Processing => match &mut self.inner {
                Inner::Reader(reader) => {
                    reader.recieve_and_process(recv_data, desc, sys_time);
                    match reader.wait() {
                        None => {}
                        Some(Ok(_)) => self.state = State::Complete,
                        Some(Err(err)) => self.state = State::Error(err.into()),
                    }
                }
                Inner::Writer(writer) => {
                    writer.recieve_and_process(recv_data, desc, sys_time);
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
