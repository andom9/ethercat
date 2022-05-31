use super::{Cyclic, EtherCATSystemTime, ReceivedData};
use crate::network::*;
use crate::packet::ethercat::*;
use crate::{
    error::CommonError,
    interface::{Command, SlaveAddress},
    register::datalink::*,
    util::const_max,
};
use nb;

#[derive(Debug, Clone)]
pub enum MailboxReaderError {
    Common(CommonError),
    TimeoutMs(u32),
    NoMailbox,
    MailboxNotAvailable,
    NoSlave,
    EmptyMailbox,
    SmallBuffer,
}

impl From<CommonError> for MailboxReaderError {
    fn from(err: CommonError) -> Self {
        Self::Common(err)
    }
}

#[derive(Debug)]
enum MailboxReaderState {
    Error(MailboxReaderError),
    Idle,
    Complete,
    CheckMailboxFull,
    Read,
    RequestRepeat,
    WaitRepeatAck,
    WaitMailboxFull,
}

impl Default for MailboxReaderState {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug)]
pub struct MailboxReader<'a> {
    timer_start: EtherCATSystemTime,
    command: Command,
    slave_address: SlaveAddress,
    buffer: [u8; buffer_size()],
    mailbox_header: MailboxPDU<[u8; MAILBOX_HEADER_LENGTH]>,
    state: MailboxReaderState,
    read_buf: &'a mut [u8],
    activation_buf: SyncManagerActivation<[u8; SyncManagerActivation::SIZE]>,
    timeout_ns: u64,
    wait_full: bool,
}

impl<'a> MailboxReader<'a> {
    pub fn start(&mut self, slave_address: SlaveAddress, timeout_ms: u32, wait: bool) {
        self.timer_start = EtherCATSystemTime(0);
        self.command = Command::default();
        self.slave_address = slave_address;
        self.buffer.fill(0);
        //self.read_buf = Some(read_buf);
        self.timeout_ns = timeout_ms as u64 * 1000 * 1000;
        self.state = MailboxReaderState::CheckMailboxFull;
        self.wait_full = wait;
    }

    pub fn wait(
        &'a mut self,
    ) -> nb::Result<(MailboxPDU<[u8; MAILBOX_HEADER_LENGTH]>, &'a [u8]), MailboxReaderError> {
        match &self.state {
            MailboxReaderState::Complete => Ok((self.mailbox_header.clone(), &self.read_buf)),
            MailboxReaderState::Error(err) => Err(nb::Error::Other(err.clone())),
            _ => Err(nb::Error::WouldBlock),
        }
    }
}

impl<'a> Cyclic for MailboxReader<'a> {
    fn next_command(
        &mut self,
        desc: &mut NetworkDescription,
        sys_time: EtherCATSystemTime,
    ) -> Option<(Command, &[u8])> {
        match self.state {
            MailboxReaderState::Idle => None,
            MailboxReaderState::Error(_) => None,
            MailboxReaderState::Complete => None,
            MailboxReaderState::CheckMailboxFull => {
                self.timer_start = sys_time;
                if let Some(slave) = desc.slave(self.slave_address) {
                    if let Some((sm_num, _)) = slave.info.mailbox_tx_sm() {
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
                        self.state = MailboxReaderState::Error(MailboxReaderError::NoMailbox);
                        None
                    }
                } else {
                    self.state = MailboxReaderState::Error(MailboxReaderError::NoSlave);
                    None
                }
            }
            MailboxReaderState::WaitMailboxFull => {
                let slave = desc.slave(self.slave_address).unwrap();
                let (sm_num, _) = slave.info.mailbox_tx_sm().unwrap();
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
            MailboxReaderState::Read => {
                let slave = desc.slave(self.slave_address).unwrap();
                let (_, sm) = slave.info.mailbox_tx_sm().unwrap();
                self.command = Command::new_read(self.slave_address, sm.start_address);
                if self.read_buf.len() < sm.size as usize {
                    self.state = MailboxReaderState::Error(MailboxReaderError::SmallBuffer);
                    None
                } else {
                    self.read_buf.fill(0);
                    //self.mailbox_header.set_address(slave.configured_address);
                    //self.mailbox_header.set_prioriry(0);
                    //self.mailbox_header.set_length(value);
                    Some((self.command, &self.read_buf[..sm.size as usize]))
                }
            }
            MailboxReaderState::RequestRepeat => {
                let slave = desc.slave(self.slave_address).unwrap();
                let (sm_num, _) = slave.info.mailbox_tx_sm().unwrap();
                self.command = Command::new_write(
                    self.slave_address,
                    SyncManagerActivation::ADDRESS + sm_num * 0x08,
                );
                self.buffer.fill(0);
                self.activation_buf
                    .set_repeat(!self.activation_buf.repeat()); //toggle
                Some((self.command, &self.activation_buf.0))
            }
            MailboxReaderState::WaitRepeatAck => {
                let slave = desc.slave(self.slave_address).unwrap();
                let (sm_num, _) = slave.info.mailbox_tx_sm().unwrap();
                self.command = Command::new_read(
                    self.slave_address,
                    SyncManagerPDIControl::ADDRESS + sm_num * 0x08,
                );
                self.buffer.fill(0);
                Some((self.command, &self.buffer[..SyncManagerPDIControl::SIZE]))
            }
        }
    }

    fn recieve_and_process(
        &mut self,
        recv_data: Option<ReceivedData>,
        _desc: &mut NetworkDescription,
        sys_time: EtherCATSystemTime,
    ) {
        if let Some(ref recv_data) = recv_data {
            let ReceivedData { command, data, wkc } = recv_data;
            if *command != self.command {
                self.state =
                    MailboxReaderState::Error(MailboxReaderError::Common(CommonError::BadPacket));
            }
            let wkc = *wkc;
            match self.state {
                MailboxReaderState::Idle => {}
                MailboxReaderState::Error(_) => {}
                MailboxReaderState::Complete => {}
                MailboxReaderState::CheckMailboxFull => {
                    if wkc != 1 {
                        self.state =
                            MailboxReaderState::Error(MailboxReaderError::MailboxNotAvailable);
                    } else {
                        let status = SyncManagerStatus(data);
                        self.activation_buf
                            .0
                            .copy_from_slice(&data[SyncManagerStatus::SIZE..]);
                        if status.is_mailbox_full() {
                            self.state = MailboxReaderState::Read;
                        } else {
                            if self.wait_full {
                                self.state = MailboxReaderState::WaitMailboxFull;
                            } else {
                                self.state =
                                    MailboxReaderState::Error(MailboxReaderError::EmptyMailbox);
                            }
                        }
                    }
                }
                MailboxReaderState::WaitMailboxFull => {
                    if wkc != 1 {
                        self.state =
                            MailboxReaderState::Error(MailboxReaderError::MailboxNotAvailable);
                    } else {
                        let status = SyncManagerStatus(data);
                        self.activation_buf
                            .0
                            .copy_from_slice(&data[SyncManagerStatus::SIZE..]);
                        if status.is_mailbox_full() {
                            self.state = MailboxReaderState::Read;
                        } else {
                            self.state = MailboxReaderState::WaitMailboxFull;
                        }
                    }
                }
                MailboxReaderState::Read => {
                    // mailbox lost
                    if wkc != 1 {
                        self.state = MailboxReaderState::RequestRepeat;
                    } else {
                        self.mailbox_header
                            .0
                            .copy_from_slice(&data[..MAILBOX_HEADER_LENGTH]);
                        self.read_buf
                            .iter_mut()
                            .skip(MAILBOX_HEADER_LENGTH)
                            .zip(data.iter())
                            .for_each(|(buf, data)| *buf = *data);
                        self.state = MailboxReaderState::Complete;
                    }
                }
                MailboxReaderState::RequestRepeat => {
                    self.state = MailboxReaderState::WaitRepeatAck;
                }
                MailboxReaderState::WaitRepeatAck => {
                    if wkc != 1 {
                        self.state = MailboxReaderState::Error(MailboxReaderError::Common(
                            CommonError::UnexpectedWKC(wkc),
                        ));
                    } else {
                        if SyncManagerPDIControl(data).repeat_ack() == self.activation_buf.repeat()
                        {
                            self.state = MailboxReaderState::WaitMailboxFull;
                        } else {
                            self.state = MailboxReaderState::WaitRepeatAck;
                        }
                    }
                }
            }
        }
        // check timeout
        if self.timer_start.0 < sys_time.0 && self.timeout_ns < sys_time.0 - self.timer_start.0 {
            self.state = MailboxReaderState::Error(MailboxReaderError::TimeoutMs(
                (self.timeout_ns / 1000 / 1000) as u32,
            ));
        }
    }
}

const fn buffer_size() -> usize {
    let mut size = 0;
    size = const_max(size, SyncManagerStatus::SIZE + SyncManagerActivation::SIZE);
    size = const_max(size, SyncManagerPDIControl::SIZE);
    size
}
