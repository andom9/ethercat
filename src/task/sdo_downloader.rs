use super::mailbox::MailboxTask;
use super::mailbox_reader::MailboxReader;
use super::mailbox_writer::MailboxWriter;
use super::sdo::SdoTaskError;
use super::TaskError;
use super::{Cyclic, EtherCatSystemTime};
use crate::frame::{AbortCode, CoeHeader, CoeServiceType, SdoDownloadNormalHeader, SdoHeader};
use crate::frame::{MailboxHeader, MailboxType};
use crate::interface::*;
use crate::network::{Slave, SyncManager};

#[derive(Debug, Clone, PartialEq)]
enum State {
    Error(TaskError<SdoTaskError>),
    Idle,
    Complete,
    WriteDownloadRequest,
    ReadDownloadResponse(bool),
}

impl Default for State {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug)]
pub struct SdoDownloader {
    slave_address: SlaveAddress,
    state: State,
    mailbox: MailboxTask,
    mailbox_count: u8,
    //mb_length: u16,
    tx_sm: SyncManager,
    rx_sm: SyncManager,
    //sdo_header: [u8; CoeHeader::SIZE + SdoHeader::SIZE + SdoDownloadNormalHeader::SIZE],
}

impl SdoDownloader {
    pub fn new() -> Self {
        let mailbox = MailboxTask::new();

        Self {
            slave_address: SlaveAddress::default(),
            state: State::Idle,
            mailbox,
            mailbox_count: 0,
            //mb_length: 0,
            tx_sm: SyncManager::default(),
            rx_sm: SyncManager::default(),
            //sdo_header: [0; CoeHeader::SIZE + SdoHeader::SIZE + SdoDownloadNormalHeader::SIZE],
        }
    }

    pub fn mailbox(&self) -> &MailboxTask {
        &self.mailbox
    }

    pub fn start(&mut self, slave: &Slave, index: u16, sub_index: u8, data: &[u8], buf: &mut [u8]) {
        self.slave_address = slave.info().slave_address();
        self.tx_sm = slave.info().mailbox_tx_sm().unwrap_or_default();
        self.rx_sm = slave.info().mailbox_rx_sm().unwrap_or_default();
        self.mailbox_count = slave.increment_mb_count();

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
        let data_len = data.len() as u16;
        SdoDownloadNormalHeader(&mut sdo_header[CoeHeader::SIZE + SdoHeader::SIZE..])
            .set_complete_size(data_len as u32);

        let mut mb_header = MailboxHeader::new();
        mb_header.set_address(0);
        mb_header.set_count(self.mailbox_count);
        mb_header.set_mailbox_type(MailboxType::CoE as u8);
        mb_header.set_length(data_len + sdo_header.len() as u16);
        mb_header.set_prioriry(0);

        MailboxWriter::set_mailbox_data(&mb_header.0, &sdo_header, buf);
        buf.iter_mut()
            .skip(MailboxHeader::SIZE + sdo_header.len())
            .zip(data)
            .for_each(|(b, d)| *b = *d);

        self.mailbox
            .start_to_write(self.slave_address, self.rx_sm, true);

        self.state = State::WriteDownloadRequest;
    }

    pub fn wait(&mut self) -> Option<Result<(), TaskError<SdoTaskError>>> {
        match &self.state {
            State::Complete => Some(Ok(())),
            State::Error(err) => Some(Err(err.clone())),
            _ => None,
        }
    }
}

impl Cyclic for SdoDownloader {
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
            State::WriteDownloadRequest => self.mailbox.next_command(buf),
            State::ReadDownloadResponse(is_first) => {
                if is_first {
                    self.mailbox
                        .start_to_read(self.slave_address, self.tx_sm, true);
                }
                self.mailbox.next_command(buf)
            }
        }
    }

    fn recieve_and_process(&mut self, recv_data: &CommandData, sys_time: EtherCatSystemTime) {
        match self.state {
            State::Idle => {}
            State::Error(_) => {}
            State::Complete => {}
            State::WriteDownloadRequest => {
                self.mailbox.recieve_and_process(recv_data, sys_time);
                match self.mailbox.wait() {
                    Some(Ok(_)) => {
                        self.state = State::ReadDownloadResponse(true);
                    }
                    None => self.state = State::WriteDownloadRequest,
                    Some(Err(other_err)) => self.state = State::Error(other_err.into()),
                }
            }
            State::ReadDownloadResponse(_) => {
                self.mailbox.recieve_and_process(recv_data, sys_time);
                match self.mailbox.wait() {
                    Some(Ok(_)) => {
                        let (_mb_header, mb_data) = MailboxReader::mailbox_data(recv_data.data);
                        let sdo_header = SdoHeader(&mb_data[CoeHeader::SIZE..]);
                        if sdo_header.command_specifier() == 4 {
                            let mut abort_code = [0; 4];
                            for (code, data) in abort_code
                                .iter_mut()
                                .zip(sdo_header.0.iter().skip(SdoHeader::SIZE))
                            {
                                *code = *data;
                            }
                            let abort_code = AbortCode::from(u32::from_le_bytes(abort_code));
                            self.state = State::Error(SdoTaskError::AbortCode(abort_code).into())
                        } else if sdo_header.command_specifier() != 3 {
                            self.state =
                                State::Error(SdoTaskError::UnexpectedCommandSpecifier.into())
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
