use super::mailbox;
use super::mailbox::MailboxUnit;
use super::sdo::Error;
use super::{CyclicProcess, EtherCatSystemTime, ReceivedData};
use crate::network::NetworkDescription;
use crate::packet::coe::{AbortCode, CoeHeader, CoeServiceType, SdoHeader};
use crate::packet::ethercat::{MailboxHeader, MailboxType};
use crate::{
    error::EcError,
    interface::{Command, SlaveAddress},
};

#[derive(Debug)]
enum State {
    Error(EcError<Error>),
    Idle,
    Complete,
    CheckMailboxEmpty(bool),
    WriteUploadRequest(bool),
    ReadUploadResponse(bool),
}

impl Default for State {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug)]
pub struct SdoUploader<'a> {
    slave_address: SlaveAddress,
    state: State,
    mailbox: MailboxUnit<'a>,
    mailbox_count: u8,
    mb_length: usize,
    //sdo_header:[u8; CoeHeader::SIZE + SdoHeader::SIZE],
}

impl<'a> SdoUploader<'a> {
    pub fn new(mb_buf: &'a mut [u8]) -> Self {
        let mailbox = MailboxUnit::new(mb_buf);

        Self {
            slave_address: SlaveAddress::default(),
            state: State::Idle,
            mailbox,
            mailbox_count: 0,
            mb_length: 0,
            //sdo_header:[0; CoeHeader::SIZE + SdoHeader::SIZE],
        }
    }

    pub fn mailbox(&self) -> &MailboxUnit {
        &self.mailbox
    }

    //pub fn sdo_data_from_slice(slice: &[u8]) -> &[u8] {
    //
    //}

    pub fn sdo_data(&self) -> &[u8] {
        let sdo_header = SdoHeader(&self.mailbox.mailbox_data()[CoeHeader::SIZE..]);

        // expedited
        if sdo_header.transfer_type() {
            let size = match sdo_header.data_set_size() {
                0 => 4,
                1 => 3,
                2 => 2,
                4 => 1,
                _ => 0,
            };
            &self.mailbox.mailbox_data()
                [CoeHeader::SIZE + SdoHeader::SIZE..CoeHeader::SIZE + SdoHeader::SIZE + size]
        // normal
        } else {
            let mut complete_size = [0; 4];
            let buf = &self.mailbox.mailbox_data()
                [CoeHeader::SIZE + SdoHeader::SIZE..CoeHeader::SIZE + SdoHeader::SIZE + 4];

            complete_size.iter_mut().zip(buf).for_each(|(s, b)| *s = *b);
            let size = u32::from_le_bytes(complete_size) as usize;
            log::info!("{:?}", complete_size);
            &self.mailbox.mailbox_data()[SdoHeader::SIZE + 4..SdoHeader::SIZE + size + 4]
        }
    }

    pub fn take_buffer(self) -> &'a mut [u8] {
        self.mailbox.take_buffer()
    }

    pub fn start(&mut self, slave_address: SlaveAddress, index: u16, sub_index: u8) {
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
        log::info!("a");

        self.mailbox
            .mailbox_data_mut()
            .iter_mut()
            .for_each(|b| *b = 0);
        log::info!("a");

        self.mailbox
            .mailbox_header_mut()
            .0
            .iter_mut()
            .for_each(|b| *b = 0);
        log::info!("a");

        self.mailbox
            .mailbox_data_mut()
            .iter_mut()
            .zip(sdo_header)
            .for_each(|(b, d)| *b = d);
        //self.writer.set_data_to_write(|buf| buf.fill(0));
        //self.mailbox.mailbox_data_mut().iter_mut().for_each(|b|*b=0);
        //self.mailbox.mailbox_header_mut().0.iter_mut().for_each(|b|*b=0);

        //self.mailbox.mailbox_data_mut().iter_mut().zip(sdo_header).for_each(|(b, d)| *b = d);

        self.mb_length = 4 + sdo_header.len();
        self.slave_address = slave_address;
        self.state = State::CheckMailboxEmpty(true);
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

impl<'a> CyclicProcess for SdoUploader<'a> {
    fn next_command(
        &mut self,
        desc: &mut NetworkDescription,
        sys_time: EtherCatSystemTime,
    ) -> Option<(Command, &[u8])> {
        log::info!("send {:?}", self.state);

        match self.state {
            State::Idle => None,
            State::Error(_) => None,
            State::Complete => None,
            State::CheckMailboxEmpty(is_first) => {
                if is_first {
                    self.mailbox.start_to_read(self.slave_address, false);
                }
                self.mailbox.next_command(desc, sys_time)
            }
            State::WriteUploadRequest(is_first) => {
                if is_first {
                    if let Some(slave) = desc.slave_mut(self.slave_address) {
                        slave.increment_mb_count();
                        self.mailbox_count = slave.mailbox_count;
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
                        self.mailbox.start_to_write(self.slave_address, true);
                    } else {
                        self.state =
                            State::Error(Error::Mailbox(mailbox::Error::NoSlave.into()).into());
                        return None;
                    }
                }
                self.mailbox.next_command(desc, sys_time)
            }
            State::ReadUploadResponse(is_first) => {
                if is_first {
                    //self.mailbox
                    //    .mailbox_data_mut()
                    //    .iter_mut()
                    //    .for_each(|b| *b = 0);
                    //self.mailbox
                    //    .mailbox_header_mut()
                    //    .0
                    //    .iter_mut()
                    //    .for_each(|b| *b = 0);
                    self.mailbox.start_to_read(self.slave_address, true);
                }
                self.mailbox.next_command(desc, sys_time)
            }
        }
    }

    fn recieve_and_process(
        &mut self,
        recv_data: Option<ReceivedData>,
        desc: &mut NetworkDescription,
        sys_time: EtherCatSystemTime,
    ) {
        log::info!("recv {:?}", self.state);

        match self.state {
            State::Idle => {}
            State::Error(_) => {}
            State::Complete => {}
            State::CheckMailboxEmpty(_) => {
                self.mailbox.recieve_and_process(recv_data, desc, sys_time);
                match self.mailbox.wait() {
                    Some(Ok(_)) => {
                        self.state = State::Error(Error::MailboxAlreadyExisted.into());
                    }
                    Some(Err(EcError::UnitSpecific(mailbox::Error::MailboxEmpty))) => {
                        self.state = State::WriteUploadRequest(true)
                    }
                    None => self.state = State::CheckMailboxEmpty(false),
                    Some(Err(other)) => self.state = State::Error(other.into()),
                }
            }
            State::WriteUploadRequest(_) => {
                self.mailbox.recieve_and_process(recv_data, desc, sys_time);
                match self.mailbox.wait() {
                    Some(Ok(_)) => {
                        self.state = State::ReadUploadResponse(true);
                    }
                    None => self.state = State::WriteUploadRequest(false),
                    Some(Err(other)) => self.state = State::Error(other.into()),
                }
            }
            State::ReadUploadResponse(_) => {
                self.mailbox.recieve_and_process(recv_data, desc, sys_time);
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
                            self.state = State::Error(Error::AbortCode(abort_code).into())
                        } else if sdo_header.command_specifier() != 2 {
                            log::info!("{}", sdo_header.command_specifier());
                            self.state = State::Error(Error::UnexpectedCommandSpecifier.into())
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
