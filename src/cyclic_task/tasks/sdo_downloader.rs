use super::mailbox::MailboxTask;
use super::mailbox::MailboxTaskError;
use super::sdo::SdoTaskError;
use super::super::{CyclicProcess, EtherCatSystemTime, ReceivedData};

use crate::frame::{AbortCode, CoeHeader, CoeServiceType, SdoDownloadNormalHeader, SdoHeader};
use crate::frame::{MailboxHeader, MailboxType};
use crate::slave_network::{SlaveInfo, SyncManager};
use crate::{
    error::EcError,
};
use super::super::interface::*;

#[derive(Debug)]
enum State {
    Error(EcError<SdoTaskError>),
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
    mailbox: MailboxTask<'a>,
    mailbox_count: u8,
    mb_length: u16,
    tx_sm: SyncManager,
    rx_sm: SyncManager,
}

impl<'a> SdoDownloader<'a> {
    pub fn new(mb_buf: &'a mut [u8]) -> Self {
        let mailbox = MailboxTask::new(mb_buf);

        Self {
            slave_address: SlaveAddress::default(),
            state: State::Idle,
            mailbox,
            mailbox_count: 0,
            mb_length: 0,
            tx_sm: SyncManager::default(),
            rx_sm: SyncManager::default(),
        }
    }

    pub fn mailbox(&self) -> &MailboxTask {
        &self.mailbox
    }

    pub fn take_buffer(self) -> &'a mut [u8] {
        self.mailbox.take_buffer()
    }

    pub fn start(&mut self, slave_info: &SlaveInfo, index: u16, sub_index: u8, data: &[u8]) {
        self.slave_address = slave_info.slave_address();
        self.tx_sm = slave_info.mailbox_tx_sm().unwrap_or_default();
        self.rx_sm = slave_info.mailbox_rx_sm().unwrap_or_default();
        self.mailbox_count = slave_info.increment_mb_count();

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

        //clear
        self.mailbox
            .mailbox_data_mut()
            .iter_mut()
            .for_each(|b| *b = 0);
        self.mailbox
            .mailbox_header_mut()
            .0
            .iter_mut()
            .for_each(|b| *b = 0);

        //set data
        self.mailbox
            .mailbox_data_mut()
            .iter_mut()
            .zip(sdo_header)
            .for_each(|(b, d)| *b = d);
        self.mailbox
            .mailbox_data_mut()
            .iter_mut()
            .skip(sdo_header.len())
            .zip(data)
            .for_each(|(b, d)| *b = *d);

        self.mb_length = data_len + sdo_header.len() as u16;
        self.state = State::CheckMailboxEmpty;
    }

    pub fn wait(&mut self) -> Option<Result<(), EcError<SdoTaskError>>> {
        match &self.state {
            State::Complete => Some(Ok(())),
            State::Error(err) => Some(Err(err.clone())),
            _ => None,
        }
    }
}

impl<'a> CyclicProcess for SdoDownloader<'a> {
    fn next_command(&mut self, sys_time: EtherCatSystemTime) -> Option<(Command, &[u8])> {
        match self.state {
            State::Idle => None,
            State::Error(_) => None,
            State::Complete => None,
            State::CheckMailboxEmpty => {
                self.mailbox
                    .start_to_read(self.slave_address, self.tx_sm, false);
                self.mailbox.next_command(sys_time)
            }
            State::WriteDownloadRequest(is_first) => {
                if is_first {
                    self.mailbox
                        .start_to_write(self.slave_address, self.rx_sm, true);
                    let mut mb_header = MailboxHeader::new();
                    mb_header.set_address(0);
                    mb_header.set_count(self.mailbox_count);
                    mb_header.set_mailbox_type(MailboxType::CoE as u8);
                    mb_header.set_length(self.mb_length as u16);
                    mb_header.set_prioriry(0);
                    self.mailbox
                        .mailbox_header_mut()
                        .0
                        .iter_mut()
                        .zip(mb_header.0)
                        .for_each(|(b, d)| *b = d);
                }
                self.mailbox.next_command(sys_time)
            }
            State::ReadDownloadResponse(is_first) => {
                if is_first {
                    self.mailbox
                        .start_to_read(self.slave_address, self.tx_sm, true);
                }
                self.mailbox.next_command(sys_time)
            }
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
            State::CheckMailboxEmpty => {
                self.mailbox.recieve_and_process(recv_data, sys_time);
                match self.mailbox.wait() {
                    Some(Ok(_)) => {
                        self.state = State::Error(SdoTaskError::MailboxAlreadyExisted.into());
                    }
                    Some(Err(EcError::TaskSpecific(MailboxTaskError::MailboxEmpty))) => {
                        self.state = State::WriteDownloadRequest(true)
                    }
                    None => {}
                    Some(Err(other_err)) => self.state = State::Error(other_err.into()),
                }
            }
            State::WriteDownloadRequest(_) => {
                self.mailbox.recieve_and_process(recv_data, sys_time);
                match self.mailbox.wait() {
                    Some(Ok(_)) => {
                        self.state = State::ReadDownloadResponse(true);
                    }
                    None => self.state = State::WriteDownloadRequest(false),
                    Some(Err(other_err)) => self.state = State::Error(other_err.into()),
                }
            }
            State::ReadDownloadResponse(_) => {
                self.mailbox.recieve_and_process(recv_data, sys_time);
                match self.mailbox.wait() {
                    Some(Ok(_)) => {
                        let sdo_header = SdoHeader(&self.mailbox.mailbox_data()[CoeHeader::SIZE..]);
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
