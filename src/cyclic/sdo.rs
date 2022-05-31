use super::{Cyclic, EtherCATSystemTime, ReceivedData};
use crate::network::*;
use crate::packet::ethercat::{MailboxPDU, MAILBOX_HEADER_LENGTH};
use crate::{
    error::CommonError,
    interface::{Command, SlaveAddress},
    register::datalink::*,
    util::const_max,
};
use nb;

#[derive(Debug, Clone)]
pub enum SDOError {
    Common(CommonError),
    TimeoutMs(u32),
    NoMailbox,
    MailboxNotAvailable,
    NoSlave,
    MailboxFull,
    SmallBuffer,
}

impl From<CommonError> for SDOError {
    fn from(err: CommonError) -> Self {
        Self::Common(err)
    }
}

#[derive(Debug)]
enum SDOState {
    Error(SDOError),
    Idle,
    Complete,
    CheckMailboxEmpty,
    WaitMailboxEmpty,
    Write,
}

impl Default for SDOState {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug)]
pub struct SDO<'a> {
    timer_start: EtherCATSystemTime,
    command: Command,
    slave_address: SlaveAddress,
    buffer: [u8; buffer_size()],
    //mailbox_header: MailboxPDU<[u8; MAILBOX_HEADER_LENGTH]>,
    state: SDOState,
    send_buf: &'a mut [u8],
    activation_buf: SyncManagerActivation<[u8; SyncManagerActivation::SIZE]>,
    timeout_ns: u64,
    wait_full: bool,
}

impl<'a> SDO<'a> {
    pub fn start(
        &mut self,
        slave_address: SlaveAddress,
        mailbox_header: MailboxPDU<[u8; MAILBOX_HEADER_LENGTH]>,
        data: &'a [u8],
        timeout_ms: u32,
        wait: bool,
    ) {
        //self.mailbox_header = mailbox_header;
        self.timer_start = EtherCATSystemTime(0);
        self.command = Command::default();
        self.slave_address = slave_address;
        self.buffer.fill(0);
        self.send_buf.fill(0);
        self.send_buf[..MAILBOX_HEADER_LENGTH].copy_from_slice(&mailbox_header.0);
        self.send_buf
            .iter_mut()
            .skip(MAILBOX_HEADER_LENGTH)
            .zip(data.iter())
            .for_each(|(buf, data)| *buf = *data);
        self.timeout_ns = timeout_ms as u64 * 1000 * 1000;
        self.state = SDOState::CheckMailboxEmpty;
        self.wait_full = wait;
    }

    pub fn wait(&mut self) -> nb::Result<(), SDOError> {
        match &self.state {
            SDOState::Complete => Ok(()),
            SDOState::Error(err) => Err(nb::Error::Other(err.clone())),
            _ => Err(nb::Error::WouldBlock),
        }
    }
}

impl<'a> Cyclic for SDO<'a> {
    fn next_command(
        &mut self,
        desc: &mut NetworkDescription,
        sys_time: EtherCATSystemTime,
    ) -> Option<(Command, &[u8])> {
        match self.state {
            SDOState::Idle => None,
            SDOState::Error(_) => None,
            SDOState::Complete => None,
            SDOState::CheckMailboxEmpty => {
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
                        self.state = SDOState::Error(SDOError::NoMailbox);
                        None
                    }
                } else {
                    self.state = SDOState::Error(SDOError::NoSlave);
                    None
                }
            }
            SDOState::WaitMailboxEmpty => {
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
            SDOState::Write => {
                let slave = desc.slave(self.slave_address).unwrap();
                let (_, sm) = slave.info.mailbox_rx_sm().unwrap();
                self.command = Command::new_write(self.slave_address, sm.start_address);
                self.buffer.fill(0);
                if self.send_buf.len() < sm.size as usize {
                    self.state = SDOState::Error(SDOError::SmallBuffer);
                    None
                } else {
                    //self.send_buf[..MAILBOX_HEADER_LENGTH].copy_from_slice(&self.mailbox_header.0);
                    Some((self.command, &self.send_buf[..sm.size as usize]))
                }
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
                    SDOState::Error(SDOError::Common(CommonError::BadPacket));
            }
            let wkc = *wkc;
            match self.state {
                SDOState::Idle => {}
                SDOState::Error(_) => {}
                SDOState::Complete => {}
                SDOState::CheckMailboxEmpty => {
                    if wkc != 1 {
                        self.state =
                            SDOState::Error(SDOError::MailboxNotAvailable);
                    } else {
                        let status = SyncManagerStatus(data);
                        self.activation_buf
                            .0
                            .copy_from_slice(&data[SyncManagerStatus::SIZE..]);
                        if !status.is_mailbox_full() {
                            self.state = SDOState::Write;
                        } else {
                            if self.wait_full {
                                self.state = SDOState::WaitMailboxEmpty;
                            } else {
                                self.state =
                                    SDOState::Error(SDOError::MailboxFull);
                            }
                        }
                    }
                }
                SDOState::WaitMailboxEmpty => {
                    if wkc != 1 {
                        self.state =
                            SDOState::Error(SDOError::MailboxNotAvailable);
                    } else {
                        let status = SyncManagerStatus(data);
                        self.activation_buf
                            .0
                            .copy_from_slice(&data[SyncManagerStatus::SIZE..]);
                        if !status.is_mailbox_full() {
                            self.state = SDOState::Write;
                        } else {
                            self.state = SDOState::WaitMailboxEmpty;
                        }
                    }
                }
                SDOState::Write => {
                    // mailbox lost
                    if wkc != 1 {
                        self.state = SDOState::Write;
                    } else {
                        self.state = SDOState::Complete;
                    }
                }
            }
        }
        // check timeout
        if self.timer_start.0 < sys_time.0 && self.timeout_ns < sys_time.0 - self.timer_start.0 {
            self.state = SDOState::Error(SDOError::TimeoutMs(
                (self.timeout_ns / 1000 / 1000) as u32,
            ));
        }
    }
}

const fn buffer_size() -> usize {
    let mut size = 0;
    size = const_max(size, SyncManagerStatus::SIZE + SyncManagerActivation::SIZE);
    size
}
