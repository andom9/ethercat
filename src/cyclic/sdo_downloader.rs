use super::mailbox_reader;
use super::mailbox_reader::MailboxReader;
use super::mailbox_writer::MailboxWriter;
use super::{CyclicProcess, EtherCatSystemTime, ReceivedData};
use crate::network::NetworkDescription;
use crate::packet::coe::{
    AbortCode, CoeHeader, CoeServiceType, SdoDownloadNormalHeader, SdoHeader,
};
use crate::packet::ethercat::{MailboxHeader, MailboxType};
use crate::{
    error::EcError,
    interface::{Command, SlaveAddress},
};

#[derive(Debug, Clone)]
pub enum Error {
    Mailbox(mailbox_reader::Error),
    MailboxAlreadyExisted,
    AbortCode(AbortCode),
    UnexpectedCommandSpecifier,
}

impl From<Error> for EcError<Error> {
    fn from(err: Error) -> Self {
        Self::UnitSpecific(err)
    }
}

impl From<EcError<mailbox_reader::Error>> for EcError<Error> {
    fn from(err: EcError<mailbox_reader::Error>) -> Self {
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
    CheckMailboxEmpty,
    WriteDownloadRequest(bool),
    ReadDownloadResponse(bool),
}

impl Default for State {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug)]
pub struct SdoDownloader<'a> {
    slave_address: SlaveAddress,
    state: State,
    reader: MailboxReader<'a>,
    writer: MailboxWriter<'a>,
    mailbox_count: u8,
    mb_length: usize,
    //header: [u8; CoeHeader::SIZE + SdoHeader::SIZE + SdoDownloadNormalHeader::SIZE],
}

impl<'a> SdoDownloader<'a> {
    pub fn new(send_buf: &'a mut [u8], recv_buf: &'a mut [u8]) -> Self {
        let reader = MailboxReader::new(recv_buf);
        let writer = MailboxWriter::new(send_buf);

        Self {
            slave_address: SlaveAddress::default(),
            state: State::Idle,
            reader,
            writer,
            mailbox_count: 0,
            mb_length: 0,
            //header: [0; CoeHeader::SIZE + SdoHeader::SIZE + SdoDownloadNormalHeader::SIZE],
        }
    }

    pub fn mailbox_reader(&self) -> &MailboxReader {
        &self.reader
    }

    pub fn take_buffer(self) -> (&'a mut [u8], &'a mut [u8]) {
        let Self { writer, reader, .. } = self;
        (writer.take_buffer(), reader.take_buffer())
    }

    pub fn start(&mut self, slave_address: SlaveAddress, index: u16, sub_index: u8, data: &[u8]) {
        let mut sdo_header = [0; CoeHeader::SIZE + SdoHeader::SIZE + SdoDownloadNormalHeader::SIZE];
        CoeHeader(&mut sdo_header).set_service_type(CoeServiceType::SdoReq as u8);
        let mut sdo = SdoHeader(&mut sdo_header[CoeHeader::SIZE..]);
        sdo.set_complete_access(false);
        sdo.set_data_set_size(0);
        sdo.set_command_specifier(1); // download request
        sdo.set_transfer_type(false); // normal transfer
        sdo.set_size_indicator(true);
        sdo.set_index(index);
        sdo.set_sub_index(sub_index);
        let data_len = data.len();
        SdoDownloadNormalHeader(&mut sdo_header[CoeHeader::SIZE + SdoHeader::SIZE..])
            .set_complete_size(data_len as u32);

        self.writer.set_data_to_write(|buf| buf.fill(0));

        self.writer
            .set_data_to_write(|buf| buf.iter_mut().zip(sdo_header).for_each(|(b, d)| *b = d));

        self.writer.set_data_to_write(|buf| {
            buf.iter_mut()
                .skip(sdo_header.len())
                .zip(data)
                .for_each(|(b, d)| *b = *d)
        });

        self.mb_length = data_len + sdo_header.len();

        self.slave_address = slave_address;
        self.state = State::CheckMailboxEmpty;
    }

    pub fn wait(&mut self) -> Option<Result<(), EcError<Error>>> {
        match &self.state {
            State::Complete => Some(Ok(())),
            State::Error(err) => Some(Err(err.clone().into())),
            //State::Idle => Err(EcError::NotStarted.into()),
            _ => None,
        }
    }
}

impl<'a> CyclicProcess for SdoDownloader<'a> {
    fn next_command(
        &mut self,
        desc: &mut NetworkDescription,
        sys_time: EtherCatSystemTime,
    ) -> Option<(Command, &[u8])> {
        match self.state {
            State::Idle => None,
            State::Error(_) => None,
            State::Complete => None,
            State::CheckMailboxEmpty => {
                self.reader.start(self.slave_address, false);
                self.reader.next_command(desc, sys_time)
            }
            State::WriteDownloadRequest(is_first) => {
                if is_first {
                    if let Some(slave) = desc.slave_mut(self.slave_address) {
                        self.writer.start(self.slave_address, true);
                        slave.increment_mb_count();
                        self.mailbox_count = slave.mailbox_count;
                        let mut mb_header = MailboxHeader::new();
                        mb_header.set_address(0);
                        mb_header.set_count(self.mailbox_count);
                        mb_header.set_mailbox_type(MailboxType::CoE as u8);
                        mb_header.set_length(self.mb_length as u16);
                        mb_header.set_prioriry(0);
                        self.writer.set_header(mb_header);
                    } else {
                        self.state = State::Error(
                            Error::Mailbox(mailbox_reader::Error::NoSlave.into()).into(),
                        );
                        return None;
                    }
                }
                self.writer.next_command(desc, sys_time)
            }
            State::ReadDownloadResponse(is_first) => {
                if is_first {
                    self.reader.start(self.slave_address, true);
                }
                self.reader.next_command(desc, sys_time)
            }
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
            State::CheckMailboxEmpty => {
                self.reader.recieve_and_process(recv_data, desc, sys_time);
                match self.reader.wait() {
                    Some(Ok(_)) => {
                        self.state = State::Error(Error::MailboxAlreadyExisted.into());
                    }
                    Some(Err(EcError::UnitSpecific(mailbox_reader::Error::MailboxEmpty))) => {
                        self.state = State::WriteDownloadRequest(true)
                    }
                    None => {}
                    Some(Err(other_err)) => self.state = State::Error(other_err.into()),
                }
            }
            State::WriteDownloadRequest(_) => {
                self.writer.recieve_and_process(recv_data, desc, sys_time);
                match self.writer.wait() {
                    Some(Ok(_)) => {
                        self.state = State::ReadDownloadResponse(true);
                    }
                    None => self.state = State::WriteDownloadRequest(false),
                    Some(Err(other_err)) => self.state = State::Error(other_err.into()),
                }
            }
            State::ReadDownloadResponse(_) => {
                self.reader.recieve_and_process(recv_data, desc, sys_time);
                match self.reader.wait() {
                    Some(Ok(_)) => {
                        let sdo_header = SdoHeader(
                            &self.reader.buffer()[MailboxHeader::SIZE + CoeHeader::SIZE..],
                        );
                        if sdo_header.command_specifier() == 4 {
                            let mut abort_code = [0; 4];
                            for (code, data) in abort_code
                                .iter_mut()
                                .zip(sdo_header.0.iter().skip(SdoHeader::SIZE))
                            {
                                *code = *data;
                            }
                            let abort_code = AbortCode::from(u32::from_le_bytes(abort_code));
                            self.state = State::Error(Error::AbortCode(abort_code).into())
                        } else if sdo_header.command_specifier() != 3 {
                            self.state = State::Error(Error::UnexpectedCommandSpecifier.into())
                        } else {
                            self.state = State::Complete;
                        }
                    }
                    None => self.state = State::ReadDownloadResponse(false),
                    Some(Err(other_err)) => self.state = State::Error(other_err.into()),
                }
            }
        }
    }
}
