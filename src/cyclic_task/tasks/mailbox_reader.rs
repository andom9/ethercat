use super::mailbox::MailboxTaskError;
use super::super::{CyclicProcess, EtherCatSystemTime, ReceivedData};
use crate::frame::{MailboxErrorResponse, MailboxHeader, MailboxType};
use crate::slave_network::SyncManager;
use crate::{
    error::EcError,
    register::{SyncManagerActivation, SyncManagerPdiControl, SyncManagerStatus},
    util::const_max,
};
use super::super::interface::*;


const MAILBOX_RESPONSE_RETRY_TIMEOUT_DEFAULT_MS: u32 = 2000;

#[derive(Debug)]
enum State {
    Error(EcError<MailboxTaskError>),
    Idle,
    Complete,
    CheckMailboxFull((bool, bool)),
    Read,
    RequestRepeat,
    WaitRepeatAck,
}

impl Default for State {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug)]
pub struct MailboxReader<'a> {
    timer_start: EtherCatSystemTime,
    command: Command,
    slave_address: SlaveAddress,
    buffer: [u8; buffer_size()],
    state: State,
    recv_buf: &'a mut [u8],
    activation_buf: SyncManagerActivation<[u8; SyncManagerActivation::SIZE]>,
    sm_ado_offset: u16,
    sm_size: u16,
    sm_start_address: u16,
}

impl<'a> MailboxReader<'a> {
    pub fn new(recv_buf: &'a mut [u8]) -> Self {
        Self {
            timer_start: EtherCatSystemTime(0),
            command: Command::default(),
            slave_address: SlaveAddress::default(),
            buffer: [0; buffer_size()],
            state: State::Idle,
            recv_buf,
            activation_buf: SyncManagerActivation([0; SyncManagerActivation::SIZE]),
            sm_ado_offset: 0,
            sm_size: 0,
            sm_start_address: 0,
        }
    }

    pub fn take_buffer(self) -> &'a mut [u8] {
        self.recv_buf
    }

    pub fn mailbox_header(&self) -> MailboxHeader<&[u8]> {
        MailboxHeader(&self.recv_buf[..MailboxHeader::SIZE])
    }

    pub fn mailbox_header_mut(&mut self) -> MailboxHeader<&mut [u8]> {
        MailboxHeader(&mut self.recv_buf[..MailboxHeader::SIZE])
    }

    pub fn mailbox_data(&self) -> &[u8] {
        &self.recv_buf[MailboxHeader::SIZE..]
    }

    pub fn mailbox_data_mut(&mut self) -> &mut [u8] {
        &mut self.recv_buf[MailboxHeader::SIZE..]
    }

    pub fn start(&mut self, slave_address: SlaveAddress, tx_sm: SyncManager, wait_full: bool) {
        self.timer_start = EtherCatSystemTime(0);
        self.command = Command::default();
        self.slave_address = slave_address;
        self.buffer.fill(0);
        self.state = State::CheckMailboxFull((true, wait_full));

        self.sm_ado_offset = tx_sm.number as u16 * 0x08;
        self.sm_size = tx_sm.size;
        self.sm_start_address = tx_sm.start_address;
    }

    pub fn wait<'b>(&'b self) -> Option<Result<(), EcError<MailboxTaskError>>> {
        match &self.state {
            State::Complete => Some(Ok(())),
            State::Error(err) => Some(Err(err.clone())),
            _ => None,
        }
    }
}

impl<'a> CyclicProcess for MailboxReader<'a> {
    fn next_command(&mut self, sys_time: EtherCatSystemTime) -> Option<(Command, &[u8])> {
        log::info!("send {:?}", self.state);
        match self.state {
            State::Idle => None,
            State::Error(_) => None,
            State::Complete => None,
            State::CheckMailboxFull((is_first, _)) => {
                if is_first {
                    self.timer_start = sys_time;
                }
                self.command = Command::new_read(
                    self.slave_address.into(),
                    SyncManagerStatus::ADDRESS + self.sm_ado_offset,
                );
                Some((
                    self.command,
                    &self.buffer[..SyncManagerStatus::SIZE + SyncManagerActivation::SIZE],
                ))
            }
            State::Read => {
                self.command = Command::new_read(self.slave_address.into(), self.sm_start_address);
                if self.recv_buf.len() < self.sm_size as usize {
                    self.state = State::Error(MailboxTaskError::BufferSmall.into());
                    None
                } else {
                    self.recv_buf.fill(0);
                    Some((self.command, &self.recv_buf[..self.sm_size as usize]))
                }
            }
            State::RequestRepeat => {
                self.command = Command::new_write(
                    self.slave_address.into(),
                    SyncManagerActivation::ADDRESS + self.sm_ado_offset,
                );
                self.buffer.fill(0);
                self.activation_buf
                    .set_repeat(!self.activation_buf.repeat()); //toggle
                Some((self.command, &self.activation_buf.0))
            }
            State::WaitRepeatAck => {
                self.command = Command::new_read(
                    self.slave_address.into(),
                    SyncManagerPdiControl::ADDRESS + self.sm_ado_offset,
                );
                self.buffer.fill(0);
                Some((self.command, &self.buffer[..SyncManagerPdiControl::SIZE]))
            }
        }
    }

    fn recieve_and_process(
        &mut self,
        recv_data: Option<ReceivedData>,
        sys_time: EtherCatSystemTime,
    ) {
        if let Some(ref recv_data) = recv_data {
            let ReceivedData { command, data, wkc } = recv_data;
            if !(command.c_type == self.command.c_type && command.ado == self.command.ado) {
                self.state = State::Error(EcError::UnexpectedCommand);
            }
            let wkc = *wkc;
            match self.state {
                State::Idle => {}
                State::Error(_) => {}
                State::Complete => {}
                State::CheckMailboxFull((_, wait_full)) => {
                    if wkc != 1 {
                        self.state = State::Error(MailboxTaskError::MailboxNotAvailable.into());
                    } else {
                        let status = SyncManagerStatus(data);
                        self.activation_buf
                            .0
                            .copy_from_slice(&data[SyncManagerStatus::SIZE..]);
                        if status.is_mailbox_full() {
                            self.state = State::Read;
                        } else if wait_full {
                            self.state = State::CheckMailboxFull((false, wait_full));
                        } else {
                            self.state = State::Error(MailboxTaskError::MailboxEmpty.into());
                        }
                    }
                }
                State::Read => {
                    // mailbox lost
                    if wkc != 1 {
                        self.state = State::RequestRepeat;
                    } else {
                        self.recv_buf
                            .iter_mut()
                            .zip(data.iter())
                            .for_each(|(buf, data)| *buf = *data);
                        let header = MailboxHeader(&data);
                        if header.mailbox_type() == MailboxType::Error as u8 {
                            let mut err = MailboxErrorResponse::new();
                            err.0.copy_from_slice(&self.recv_buf[..4]);
                            self.state = State::Error(MailboxTaskError::ErrorResponse(err).into());
                        } else {
                            self.state = State::Complete;
                        }
                    }
                }
                State::RequestRepeat => {
                    self.state = State::WaitRepeatAck;
                }
                State::WaitRepeatAck => {
                    if wkc != 1 {
                        self.state = State::Error(EcError::UnexpectedWkc(wkc));
                    } else if SyncManagerPdiControl(data).repeat_ack()
                        == self.activation_buf.repeat()
                    {
                        self.state = State::CheckMailboxFull((false, true));
                    } else {
                        self.state = State::WaitRepeatAck;
                    }
                }
            }
        }
        // check timeout
        let timeout_ns = (MAILBOX_RESPONSE_RETRY_TIMEOUT_DEFAULT_MS as u64) * 1000 * 1000;
        if self.timer_start.0 < sys_time.0 && timeout_ns < sys_time.0 - self.timer_start.0 {
            self.state = State::Error(MailboxTaskError::Timeout.into());
        }
    }
}

const fn buffer_size() -> usize {
    let mut size = 0;
    size = const_max(size, SyncManagerStatus::SIZE + SyncManagerActivation::SIZE);
    size = const_max(size, SyncManagerPdiControl::SIZE);
    size
}
