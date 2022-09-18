use super::mailbox::MailboxTask;
use super::sdo::SdoTaskError;
use super::TaskError;
use super::{Cyclic, EtherCatSystemTime};
use super::{MailboxReader, MailboxWriter};
use crate::frame::{AbortCode, CoeHeader, CoeServiceType, SdoHeader};
use crate::frame::{MailboxHeader, MailboxType};
use crate::interface::*;
use crate::network::{Slave, SyncManager};

#[derive(Debug, Clone, PartialEq)]
enum State {
    Error(TaskError<SdoTaskError>),
    Idle,
    Complete,
    WriteUploadRequest,
    ReadUploadResponse(bool),
}

impl Default for State {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug)]
pub struct SdoUploader {
    slave_address: SlaveAddress,
    state: State,
    mailbox: MailboxTask,
    mailbox_count: u8,
    //mb_length: u16,
    tx_sm: SyncManager,
    rx_sm: SyncManager,
}

impl SdoUploader {
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
        }
    }

    pub fn mailbox(&self) -> &MailboxTask {
        &self.mailbox
    }

    pub fn sdo_data<'a>(&self, mb_data: &'a [u8]) -> &'a [u8] {
        let sdo_header = SdoHeader(&mb_data[MailboxHeader::SIZE + CoeHeader::SIZE..]);

        // expedited
        if sdo_header.transfer_type() {
            let size = match sdo_header.data_set_size() {
                0 => 4,
                1 => 3,
                2 => 2,
                3 => 1,
                _ => 0,
            };
            //dbg!(&sdo_header);
            //dbg!(&sdo_header.data_set_size());
            //dbg!(&sdo_header.index());
            //dbg!(&sdo_header.sub_index());


            &mb_data[MailboxHeader::SIZE + CoeHeader::SIZE + SdoHeader::SIZE
                ..MailboxHeader::SIZE + CoeHeader::SIZE + SdoHeader::SIZE + size]
        // normal
        } else {
            let mut complete_size = [0; 4];
            let buf =
                &mb_data[CoeHeader::SIZE + SdoHeader::SIZE..CoeHeader::SIZE + SdoHeader::SIZE + 4];

            complete_size.iter_mut().zip(buf).for_each(|(s, b)| *s = *b);
            let size = u32::from_le_bytes(complete_size) as usize;
            &mb_data[SdoHeader::SIZE + 4..SdoHeader::SIZE + size + 4]
        }
    }

    //pub fn take_buffer(self) -> &'a mut [u8] {
    //    self.mailbox.take_buffer()
    //}

    pub fn start(&mut self, slave: &Slave, index: u16, sub_index: u8, buf: &mut [u8]) {
        buf.fill(0);
        self.slave_address = slave.info().slave_address();
        self.tx_sm = slave.info().mailbox_tx_sm().unwrap_or_default();
        self.rx_sm = slave.info().mailbox_rx_sm().unwrap_or_default();
        self.mailbox_count = slave.increment_mb_count();

        let mut sdo_header = [0; CoeHeader::SIZE + SdoHeader::SIZE];
        CoeHeader(&mut sdo_header).set_service_type(CoeServiceType::SdoReq as u8);
        let mut sdo = SdoHeader(&mut sdo_header[CoeHeader::SIZE..]);
        sdo.set_complete_access(false);
        sdo.set_data_set_size(0);
        sdo.set_command_specifier(2); // upload request
        sdo.set_transfer_type(false);
        sdo.set_size_indicator(false);
        sdo.set_index(index);
        sdo.set_sub_index(sub_index);

        let mb_length = 4 + sdo_header.len() as u16;

        let mut mb_header = MailboxHeader::new();
        mb_header.set_address(0);
        mb_header.set_count(self.mailbox_count);
        mb_header.set_mailbox_type(MailboxType::CoE as u8);
        mb_header.set_length(mb_length);
        mb_header.set_prioriry(0);

        //self.mailbox
        //    .mailbox_header_mut()
        //    .0
        //    .iter_mut()
        //    .zip(mb_header.0)
        //    .for_each(|(b, d)| *b = d);
        MailboxWriter::set_mailbox_data(&mb_header.0, &sdo_header, buf);
        self.mailbox
            .start_to_write(self.slave_address, self.rx_sm, true);

        self.state = State::WriteUploadRequest;
    }

    pub fn wait(&mut self) -> Option<Result<(), TaskError<SdoTaskError>>> {
        match &self.state {
            State::Complete => Some(Ok(())),
            State::Error(err) => Some(Err(err.clone())),
            _ => None,
        }
    }
}

impl Cyclic for SdoUploader {
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
            //State::CheckMailboxEmpty(is_first) => {
            //    if is_first {
            //        self.mailbox
            //            .start_to_read(self.slave_address, self.tx_sm, false);
            //    }
            //    self.mailbox.next_command(sys_time)
            //}
            State::WriteUploadRequest => self.mailbox.next_command(buf),
            State::ReadUploadResponse(is_first) => {
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
            //State::CheckMailboxEmpty(_) => {
            //    self.mailbox.recieve_and_process(recv_data, sys_time);
            //    match self.mailbox.wait() {
            //        Some(Ok(_)) => {
            //            self.state = State::Error(SdoTaskError::MailboxAlreadyExisted.into());
            //        }
            //        Some(Err(TaskError::TaskSpecific(MailboxTaskError::MailboxEmpty))) => {
            //            self.state = State::WriteUploadRequest(true)
            //        }
            //        None => self.state = State::CheckMailboxEmpty(false),
            //        Some(Err(other)) => self.state = State::Error(other.into()),
            //    }
            //}
            State::WriteUploadRequest => {
                self.mailbox.recieve_and_process(recv_data, sys_time);
                match self.mailbox.wait() {
                    Some(Ok(_)) => {
                        self.state = State::ReadUploadResponse(true);
                    }
                    None => self.state = State::WriteUploadRequest,
                    Some(Err(other)) => self.state = State::Error(other.into()),
                }
            }
            State::ReadUploadResponse(_) => {
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
                        } else if sdo_header.command_specifier() != 2 {
                            self.state =
                                State::Error(SdoTaskError::UnexpectedCommandSpecifier.into())
                        } else {
                            self.state = State::Complete;
                        }
                    }
                    None => self.state = State::ReadUploadResponse(false),
                    Some(Err(other)) => self.state = State::Error(other.into()),
                }
            }
        }
    }
}
