use super::mailbox::MailboxTask;
use super::sdo::SdoTaskError;
use super::TaskError;
use super::{CyclicTask, EtherCatSystemTime};
//use super::{MailboxReadTask, MailboxWriteTask};
use crate::frame::{AbortCode, CoeFrame, CoeServiceType, SdoFrame};
use crate::frame::{MailboxFrame, MailboxType};
use crate::interface::*;
use crate::slave::{Slave, SyncManager};

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
pub struct SdoReadTask {
    slave_address: SlaveAddress,
    state: State,
    mailbox: MailboxTask,
    mailbox_count: u8,
    tx_sm: SyncManager,
    rx_sm: SyncManager,
}

impl SdoReadTask {
    pub fn new() -> Self {
        let mailbox = MailboxTask::new();

        Self {
            slave_address: SlaveAddress::default(),
            state: State::Idle,
            mailbox,
            mailbox_count: 0,
            tx_sm: SyncManager::default(),
            rx_sm: SyncManager::default(),
        }
    }

    pub fn mailbox(&self) -> &MailboxTask {
        &self.mailbox
    }

    pub fn sdo_data<'a>(&self, mb_data: &'a [u8]) -> &'a [u8] {
        let sdo_header = SdoFrame(&mb_data[MailboxFrame::HEADER_SIZE + CoeFrame::HEADER_SIZE..]);

        // expedited
        if sdo_header.transfer_type() {
            let size = match sdo_header.data_set_size() {
                0 => 4,
                1 => 3,
                2 => 2,
                3 => 1,
                _ => 0,
            };

            &mb_data[MailboxFrame::HEADER_SIZE + CoeFrame::HEADER_SIZE + SdoFrame::HEADER_SIZE
                ..MailboxFrame::HEADER_SIZE + CoeFrame::HEADER_SIZE + SdoFrame::HEADER_SIZE + size]

        // normal
        } else {
            let mut complete_size = [0; 4];
            let buf = &mb_data[CoeFrame::HEADER_SIZE + SdoFrame::HEADER_SIZE
                ..CoeFrame::HEADER_SIZE + SdoFrame::HEADER_SIZE + 4];

            complete_size.iter_mut().zip(buf).for_each(|(s, b)| *s = *b);
            let size = u32::from_le_bytes(complete_size) as usize;
            &mb_data[SdoFrame::HEADER_SIZE + 4..SdoFrame::HEADER_SIZE + size + 4]
        }
    }

    pub fn start(&mut self, slave: &Slave, index: u16, sub_index: u8, buf: &mut [u8]) {
        buf.fill(0);
        self.slave_address = slave.info().slave_address();
        self.tx_sm = slave.info().mailbox_tx_sm().unwrap_or_default();
        self.rx_sm = slave.info().mailbox_rx_sm().unwrap_or_default();
        self.mailbox_count = slave.increment_mb_count();

        let mut sdo_header = [0; CoeFrame::HEADER_SIZE + SdoFrame::HEADER_SIZE];
        CoeFrame(&mut sdo_header).set_service_type(CoeServiceType::SdoReq as u8);
        let mut sdo = SdoFrame(&mut sdo_header[CoeFrame::HEADER_SIZE..]);
        sdo.set_complete_access(false);
        sdo.set_data_set_size(0);
        sdo.set_command_specifier(2); // upload request
        sdo.set_transfer_type(false);
        sdo.set_size_indicator(false);
        sdo.set_index(index);
        sdo.set_sub_index(sub_index);

        let mb_length = 4 + sdo_header.len() as u16;

        let mut mb_header = MailboxFrame::new();
        mb_header.set_address(0);
        mb_header.set_count(self.mailbox_count);
        mb_header.set_mailbox_type(MailboxType::CoE as u8);
        mb_header.set_length(mb_length);
        mb_header.set_prioriry(0);

        MailboxTask::set_mailbox_data(&mb_header, &sdo_header, buf);
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

impl CyclicTask for SdoReadTask {
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
            State::WriteUploadRequest => self.mailbox.next_pdu(buf),
            State::ReadUploadResponse(is_first) => {
                if is_first {
                    self.mailbox
                        .start_to_read(self.slave_address, self.tx_sm, true);
                }
                self.mailbox.next_pdu(buf)
            }
        }
    }

    fn recieve_and_process(&mut self, recv_data: &Pdu, sys_time: EtherCatSystemTime) {
        match self.state {
            State::Idle => {}
            State::Error(_) => {}
            State::Complete => {}
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
                        let (_mb_header, mb_data) = MailboxTask::mailbox_data(recv_data.data);
                        let sdo_header = SdoFrame(&mb_data[CoeFrame::HEADER_SIZE..]);
                        if sdo_header.command_specifier() == 4 {
                            let mut abort_code = [0; 4];
                            for (code, data) in abort_code
                                .iter_mut()
                                .zip(sdo_header.0.iter().skip(SdoFrame::HEADER_SIZE))
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
