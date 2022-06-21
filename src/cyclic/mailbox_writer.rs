use super::mailbox_reader::Error;
use super::{CyclicProcess, EtherCatSystemTime, ReceivedData};
use crate::network::NetworkDescription;
use crate::packet::ethercat::MailboxHeader;
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
        }
    }

    pub fn set_data_to_write<F: FnOnce(&mut [u8])>(&mut self, data_writer: F) {
        data_writer(&mut self.send_buf[MailboxHeader::SIZE..]);
    }

    pub fn data_to_write(&self) -> &[u8] {
        &self.send_buf[MailboxHeader::SIZE..]
    }

    pub fn set_header(&mut self, mailbox_header: MailboxHeader<[u8; MailboxHeader::SIZE]>) {
        self.send_buf[..MailboxHeader::SIZE].copy_from_slice(&mailbox_header.0);
    }

    pub fn header(&self) -> MailboxHeader<[u8; MailboxHeader::SIZE]> {
        let mut header = MailboxHeader::new();
        header
            .0
            .copy_from_slice(&self.send_buf[..MailboxHeader::SIZE]);
        header
    }

    pub fn start(
        &mut self,
        slave_address: SlaveAddress,
        //mailbox_header: MailboxPdu<[u8; MailboxHeader::SIZE]>,
        //data: &[u8],
        //timeout_ms: u32,
        wait_empty: bool,
    ) {
        //self.mailbox_header = mailbox_header;
        self.timer_start = EtherCatSystemTime(0);
        self.command = Command::default();
        self.slave_address = slave_address;
        self.buffer.fill(0);
        self.send_buf.fill(0);
        //self.send_buf[..MailboxHeader::SIZE].copy_from_slice(&mailbox_header.0);
        //self.send_buf
        //    .iter_mut()
        //    .skip(MailboxHeader::SIZE)
        //    .zip(data.iter())
        //    .for_each(|(buf, data)| *buf = *data);
        self.timeout_ns = MAILBOX_REQUEST_RETRY_TIMEOUT_DEFAULT_MS as u64 * 1000 * 1000;
        self.state = State::CheckMailboxEmpty;
        self.wait_full = wait_empty;
    }

    pub fn wait(&self) -> Option<Result<(), EcError<Error>>> {
        match &self.state {
            State::Complete => Some(Ok(())),
            State::Error(err) => Some(Err(err.clone())),
            //State::Idle => Err(EcError::NotStarted.into()),
            _ => None,
        }
    }
}

impl<'a> CyclicProcess for MailboxWriter<'a> {
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
                self.timer_start = sys_time;
                if let Some(slave) = desc.slave(self.slave_address) {
                    if let Some((sm_num, _)) = slave.info.mailbox_rx_sm() {
                        self.command = Command::new_read(
                            self.slave_address,
                            SyncManagerStatus::ADDRESS + sm_num * 0x08,
                        );
                        self.buffer.fill(0);
                        Some((
                            self.command,
                            &self.buffer[..SyncManagerStatus::SIZE + SyncManagerActivation::SIZE],
                        ))
                    } else {
                        self.state = State::Error(Error::NoMailbox.into());
                        None
                    }
                } else {
                    self.state = State::Error(Error::NoSlave.into());
                    None
                }
            }
            State::WaitMailboxEmpty => {
                let slave = desc.slave(self.slave_address).unwrap();
                let (sm_num, _) = slave.info.mailbox_rx_sm().unwrap();
                self.command = Command::new_read(
                    self.slave_address,
                    SyncManagerStatus::ADDRESS + sm_num * 0x08,
                );
                self.buffer.fill(0);
                Some((
                    self.command,
                    &self.buffer[..SyncManagerStatus::SIZE + SyncManagerActivation::SIZE],
                ))
            }
            State::Write => {
                let slave = desc.slave(self.slave_address).unwrap();
                let (_, sm) = slave.info.mailbox_rx_sm().unwrap();
                self.command = Command::new_write(self.slave_address, sm.start_address);
                self.buffer.fill(0);
                if self.send_buf.len() < sm.size as usize {
                    self.state = State::Error(Error::BufferSmall.into());
                    None
                } else {
                    //self.send_buf[..MailboxHeader::SIZE].copy_from_slice(&self.mailbox_header.0);
                    Some((self.command, &self.send_buf[..sm.size as usize]))
                }
            }
        }
    }

    fn recieve_and_process(
        &mut self,
        recv_data: Option<ReceivedData>,
        _desc: &mut NetworkDescription,
        sys_time: EtherCatSystemTime,
    ) {
        if let Some(ref recv_data) = recv_data {
            let ReceivedData { command, data, wkc } = recv_data;
            //if *command != self.command {
            //    self.state = State::Error(EcError::UnexpectedCommand);
            //}
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
