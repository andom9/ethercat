use super::mailbox::Error;
use super::{CyclicProcess, EtherCatSystemTime, ReceivedData};
use crate::packet::ethercat::MailboxHeader;
use crate::slave::{SyncManager};
use crate::{
    error::EcError,
    interface::{Command, SlaveAddress},
    register::datalink::{SyncManagerActivation, SyncManagerStatus},
    util::const_max,
};

const MAILBOX_REQUEST_RETRY_TIMEOUT_DEFAULT_MS: u32 = 100;

#[derive(Debug)]
enum State {
    Error(EcError<Error>),
    Idle,
    Complete,
    CheckMailboxEmpty,
    WaitMailboxEmpty,
    Write,
}

impl Default for State {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug)]
pub struct MailboxWriter<'a> {
    timer_start: EtherCatSystemTime,
    command: Command,
    slave_address: SlaveAddress,
    buffer: [u8; buffer_size()],
    state: State,
    send_buf: &'a mut [u8],
    //data_length: usize,
    activation_buf: SyncManagerActivation<[u8; SyncManagerActivation::SIZE]>,
    timeout_ns: u64,
    wait_full: bool,
    sm_ado_offset: u16,
    sm_size: u16,
    sm_start_address: u16,
}

impl<'a> MailboxWriter<'a> {
    pub fn new(send_buf: &'a mut [u8]) -> Self {
        Self {
            timer_start: EtherCatSystemTime(0),
            command: Command::default(),
            slave_address: SlaveAddress::default(),
            buffer: [0; buffer_size()],
            state: State::Idle,
            send_buf,
            activation_buf: SyncManagerActivation([0; SyncManagerActivation::SIZE]),
            timeout_ns: 0,
            wait_full: true,
            sm_ado_offset: 0,
            sm_size: 0,
            sm_start_address: 0,
        }
    }

    pub fn take_buffer(self) -> &'a mut [u8] {
        self.send_buf
    }

    pub fn mailbox_header(&self) -> MailboxHeader<&[u8]> {
        MailboxHeader(&self.send_buf[..MailboxHeader::SIZE])
    }

    pub fn mailbox_header_mut(&mut self) -> MailboxHeader<&mut [u8]> {
        MailboxHeader(&mut self.send_buf[..MailboxHeader::SIZE])
    }

    pub fn mailbox_data(&self) -> &[u8] {
        &self.send_buf[MailboxHeader::SIZE..]
    }

    pub fn mailbox_data_mut(&mut self) -> &mut [u8] {
        &mut self.send_buf[MailboxHeader::SIZE..]
    }

    pub fn start(&mut self, slave_address: SlaveAddress, rx_sm: SyncManager, wait_empty: bool) {
        self.timer_start = EtherCatSystemTime(0);
        self.command = Command::default();
        self.slave_address = slave_address;
        self.buffer.fill(0);
        self.timeout_ns = MAILBOX_REQUEST_RETRY_TIMEOUT_DEFAULT_MS as u64 * 1000 * 1000;
        self.state = State::CheckMailboxEmpty;
        self.wait_full = wait_empty;

        //if let Some((sm_num, sm)) = slave_info.mailbox_tx_sm() {
        self.sm_ado_offset = rx_sm.number as u16 * 0x08;
        self.sm_size = rx_sm.size;
        self.sm_start_address = rx_sm.start_address;
        //} else {
        //    self.state = State::Error(Error::NoMailbox.into());
        //}
    }

    pub fn wait(&self) -> Option<Result<(), EcError<Error>>> {
        match &self.state {
            State::Complete => Some(Ok(())),
            State::Error(err) => Some(Err(err.clone())),
            _ => None,
        }
    }
}

impl<'a> CyclicProcess for MailboxWriter<'a> {
    fn next_command(
        &mut self,
        //desc: &mut NetworkDescription,
        sys_time: EtherCatSystemTime,
    ) -> Option<(Command, &[u8])> {
        log::info!("send {:?}", self.state);
        match self.state {
            State::Idle => None,
            State::Error(_) => None,
            State::Complete => None,
            State::CheckMailboxEmpty => {
                self.timer_start = sys_time;
                self.command = Command::new_read(
                    self.slave_address,
                    SyncManagerStatus::ADDRESS + self.sm_ado_offset,
                );
                Some((
                    self.command,
                    &self.buffer[..SyncManagerStatus::SIZE + SyncManagerActivation::SIZE],
                ))
            }
            State::WaitMailboxEmpty => {
                //let slave = desc.slave(self.slave_address).unwrap();
                //let (sm_num, _) = slave.info.mailbox_rx_sm().unwrap();
                self.command = Command::new_read(
                    self.slave_address,
                    SyncManagerStatus::ADDRESS + self.sm_ado_offset,
                );
                self.buffer.fill(0);
                Some((
                    self.command,
                    &self.buffer[..SyncManagerStatus::SIZE + SyncManagerActivation::SIZE],
                ))
            }
            State::Write => {
                //let slave = desc.slave(self.slave_address).unwrap();
                //let (_, sm) = slave.info.mailbox_rx_sm().unwrap();
                self.command = Command::new_write(self.slave_address, self.sm_start_address);
                self.buffer.fill(0);
                if self.send_buf.len() < self.sm_size as usize {
                    log::info!("{}", self.sm_size);
                    self.state = State::Error(Error::BufferSmall.into());
                    None
                } else {
                    Some((self.command, &self.send_buf[..self.sm_size as usize]))
                }
            }
        }
    }

    fn recieve_and_process(
        &mut self,
        recv_data: Option<ReceivedData>,
        //_desc: &mut NetworkDescription,
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
                State::CheckMailboxEmpty => {
                    if wkc != 1 {
                        self.state = State::Error(Error::MailboxNotAvailable.into());
                    } else {
                        let status = SyncManagerStatus(data);
                        self.activation_buf
                            .0
                            .copy_from_slice(&data[SyncManagerStatus::SIZE..]);
                        if !status.is_mailbox_full() {
                            self.state = State::Write;
                        } else if self.wait_full {
                            self.state = State::WaitMailboxEmpty;
                        } else {
                            self.state = State::Error(Error::MailboxFull.into());
                        }
                    }
                }
                State::WaitMailboxEmpty => {
                    if wkc != 1 {
                        self.state = State::Error(Error::MailboxNotAvailable.into());
                    } else {
                        let status = SyncManagerStatus(data);
                        self.activation_buf
                            .0
                            .copy_from_slice(&data[SyncManagerStatus::SIZE..]);
                        if !status.is_mailbox_full() {
                            self.state = State::Write;
                        } else {
                            self.state = State::WaitMailboxEmpty;
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
        }
        // check timeout
        if self.timer_start.0 < sys_time.0 && self.timeout_ns < sys_time.0 - self.timer_start.0 {
            self.state =
                State::Error(Error::TimeoutMs((self.timeout_ns / 1000 / 1000) as u32).into());
        }
    }
}

const fn buffer_size() -> usize {
    let mut size = 0;
    size = const_max(size, SyncManagerStatus::SIZE + SyncManagerActivation::SIZE);
    size
}