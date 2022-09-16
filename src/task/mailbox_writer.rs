use super::mailbox::MailboxTaskError;
use super::TaskError;
use super::{Cyclic, EtherCatSystemTime};
use crate::frame::MailboxHeader;
use crate::interface::*;
use crate::network::SyncManager;
use crate::register::{SyncManagerActivation, SyncManagerStatus};

const MAILBOX_REQUEST_RETRY_TIMEOUT_DEFAULT_MS: u32 = 100;

#[derive(Debug, Clone, PartialEq)]
enum State {
    Error(TaskError<MailboxTaskError>),
    Idle,
    Complete,
    CheckMailboxEmpty((bool, bool)),
    Write,
}

impl Default for State {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug)]
pub struct MailboxWriter {
    timer_start: EtherCatSystemTime,
    command: Command,
    slave_address: SlaveAddress,
    empty_check_buffer: [u8; SyncManagerStatus::SIZE + SyncManagerActivation::SIZE],
    state: State,
    //send_buf: &'a mut [u8],
    sm_ado_offset: u16,
    sm_size: u16,
    sm_start_address: u16,
}

impl MailboxWriter {
    //pub fn required_buffer_size(&self) -> usize {
    //    (self.sm_size as usize).max(buffer_size())
    //}

    pub fn new() -> Self {
        Self {
            timer_start: EtherCatSystemTime(0),
            command: Command::default(),
            slave_address: SlaveAddress::default(),
            empty_check_buffer: [0; SyncManagerStatus::SIZE + SyncManagerActivation::SIZE],
            state: State::Idle,
            //send_buf,
            sm_ado_offset: 0,
            sm_size: 0,
            sm_start_address: 0,
        }
    }

    //pub fn take_buffer(self) -> &'a mut [u8] {
    //    self.send_buf
    //}

    // pub fn mailbox_header(&self) -> MailboxHeader<&[u8]> {
    //     MailboxHeader(&self.send_buf[..MailboxHeader::SIZE])
    // }

    // pub fn mailbox_header_mut(&mut self) -> MailboxHeader<&mut [u8]> {
    //     MailboxHeader(&mut self.send_buf[..MailboxHeader::SIZE])
    // }

    // pub fn mailbox_data(&self) -> &[u8] {
    //     &self.send_buf[MailboxHeader::SIZE..]
    // }

    // pub fn mailbox_data_mut(&mut self) -> &mut [u8] {
    //     &mut self.send_buf[MailboxHeader::SIZE..]
    // }
    pub fn set_mailbox_data(mb_header: &[u8; MailboxHeader::SIZE], mb_data: &[u8], buf: &mut [u8]) {
        buf[..MailboxHeader::SIZE]
            .iter_mut()
            .zip(mb_header)
            .for_each(|(b, d)| *b = *d);
        buf[MailboxHeader::SIZE..]
            .iter_mut()
            .zip(mb_data)
            .for_each(|(b, d)| *b = *d);
    }

    pub fn start(&mut self, slave_address: SlaveAddress, rx_sm: SyncManager, wait_empty: bool) {
        self.timer_start = EtherCatSystemTime(0);
        self.command = Command::default();
        self.slave_address = slave_address;
        //self.buffer.fill(0);
        self.state = State::CheckMailboxEmpty((true, wait_empty));

        self.sm_ado_offset = rx_sm.number as u16 * 0x08;
        self.sm_size = rx_sm.size;
        self.sm_start_address = rx_sm.start_address;
    }

    pub fn wait(&self) -> Option<Result<(), TaskError<MailboxTaskError>>> {
        match &self.state {
            State::Complete => Some(Ok(())),
            State::Error(err) => Some(Err(err.clone())),
            _ => None,
        }
    }
}

impl Cyclic for MailboxWriter {
    fn is_finished(&self) -> bool {
        match self.state {
            State::Complete | State::Error(_) => true,
            _ => false,
        }
    }

    fn next_command(&mut self, buf: &mut [u8]) -> Option<(Command, usize)> {
        log::info!("send {:?}", self.state);
        match self.state {
            State::Idle => None,
            State::Error(_) => None,
            State::Complete => None,
            State::CheckMailboxEmpty(_) => {
                self.command = Command::new_read(
                    self.slave_address.into(),
                    SyncManagerStatus::ADDRESS + self.sm_ado_offset,
                );
                let length = SyncManagerStatus::SIZE + SyncManagerActivation::SIZE;
                self.empty_check_buffer
                    .iter_mut()
                    .zip(buf.iter())
                    .for_each(|(b, sb)| *b = *sb);
                buf[..length].fill(0);
                Some((self.command, length))
            }
            State::Write => {
                self.command = Command::new_write(self.slave_address.into(), self.sm_start_address);
                //buf[..self.sm_size as usize].fill(0);
                buf.iter_mut()
                    .zip(self.empty_check_buffer.iter())
                    .for_each(|(sb, b)| *sb = *b);
                if buf.len() < self.sm_size as usize {
                    log::info!("{}", self.sm_size);
                    self.state = State::Error(MailboxTaskError::BufferSmall.into());
                    None
                } else {
                    Some((self.command, self.sm_size as usize))
                }
            }
        }
    }

    fn recieve_and_process(&mut self, recv_data: &CommandData, sys_time: EtherCatSystemTime) {
        let CommandData { command, data, wkc } = recv_data;
        if !(command.c_type == self.command.c_type && command.ado == self.command.ado) {
            self.state = State::Error(TaskError::UnexpectedCommand);
        }
        let wkc = *wkc;
        match self.state {
            State::Idle => {}
            State::Error(_) => {}
            State::Complete => {}
            State::CheckMailboxEmpty((is_first, wait_empty)) => {
                if is_first {
                    self.timer_start = sys_time;
                }
                if wkc != 1 {
                    self.state = State::Error(MailboxTaskError::MailboxNotAvailable.into());
                } else {
                    let status = SyncManagerStatus(data);
                    //self.activation_buf
                    //    .0
                    //    .copy_from_slice(&data[SyncManagerStatus::SIZE..]);
                    if !status.is_mailbox_full() {
                        self.state = State::Write;
                    } else if wait_empty {
                        self.state = State::CheckMailboxEmpty((false, wait_empty));
                    } else {
                        self.state = State::Error(MailboxTaskError::MailboxFull.into());
                    }
                }
            }
            State::Write => {
                // mailbox lost
                if wkc != 1 {
                    self.state = State::Write;
                } else {
                    self.state = State::Complete;
                }
            }
        }

        // check timeout
        let timeout_ns = (MAILBOX_REQUEST_RETRY_TIMEOUT_DEFAULT_MS as u64) * 1000 * 1000;
        if self.timer_start.0 < sys_time.0 && timeout_ns < sys_time.0 - self.timer_start.0 {
            self.state = State::Error(TaskError::Timeout);
        }
    }
}

// const fn buffer_size() -> usize {
//     let mut size = 0;
//     size = const_max(size, SyncManagerStatus::SIZE + SyncManagerActivation::SIZE);
//     size
// }
