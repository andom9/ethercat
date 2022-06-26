use super::EtherCatSystemTime;
use super::ReceivedData;
use crate::cyclic::CyclicProcess;
use crate::error::EcError;
use crate::interface::{Command, SlaveAddress};
use crate::network::NetworkDescription;
use crate::packet::ethercat::CommandType;

#[derive(Debug)]
enum State {
    Error(EcError<()>),
    Idle,
    Read,
    Write,
    Complete,
}

#[derive(Debug)]
pub struct RamAccessUnit {
    state: State,
    slave_address: Option<SlaveAddress>,
    command: Command,
    buffer: [u8; 16],
    buf_size: usize,
    ado: u16,
}

impl RamAccessUnit {
    pub fn new() -> Self {
        Self {
            state: State::Idle,
            slave_address: None,
            command: Command::default(),
            buffer: [0; 16],
            buf_size: 0,
            ado: 0,
        }
    }

    pub fn start_to_read(
        &mut self,
        slave_address: Option<SlaveAddress>,
        ado: u16,
        buf_size: usize,
    ) {
        assert!(buf_size <= 16);
        self.slave_address = slave_address;
        self.state = State::Read;
        self.buffer.fill(0);
        self.command = Command::default();
        self.buf_size = buf_size;
        self.ado = ado;
    }

    pub fn start_to_write(&mut self, slave_address: Option<SlaveAddress>, ado: u16, data: &[u8]) {
        let size = data.len();
        assert!(size <= 16);
        self.slave_address = slave_address;
        self.state = State::Write;
        self.buffer.iter_mut().zip(data).for_each(|(b, d)| *b = *d);
        self.command = Command::default();
        self.buf_size = size;
        self.ado = ado;
    }

    pub fn wait(&mut self) -> Option<Result<&[u8], EcError<()>>> {
        match &self.state {
            State::Complete => Some(Ok(&self.buffer[..self.buf_size])),
            State::Error(err) => Some(Err(err.clone())),
            _ => None,
        }
    }
}

impl CyclicProcess for RamAccessUnit {
    fn next_command(
        &mut self,
        _: &mut NetworkDescription,
        _: EtherCatSystemTime,
    ) -> Option<(Command, &[u8])> {
        match self.state {
            State::Idle => None,
            State::Error(_) => None,
            State::Read => {
                if let Some(slave_address) = self.slave_address {
                    self.command = Command::new_read(slave_address, self.ado);
                } else {
                    self.command = Command::new(CommandType::BRD, 0, self.ado);
                }
                Some((self.command, &self.buffer[..self.buf_size]))
            }
            State::Write => {
                if let Some(slave_address) = self.slave_address {
                    self.command = Command::new_write(slave_address, self.ado);
                } else {
                    self.command = Command::new(CommandType::BWR, 0, self.ado);
                }
                Some((self.command, &self.buffer[..self.buf_size]))
            }
            State::Complete => None,
        }
    }

    fn recieve_and_process(
        &mut self,
        recv_data: Option<ReceivedData>,
        desc: &mut NetworkDescription,
        _: EtherCatSystemTime,
    ) {
        let data = if let Some(recv_data) = recv_data {
            let ReceivedData { command, data, wkc } = recv_data;
            if !(command.c_type == self.command.c_type && command.ado == self.command.ado) {
                self.state = State::Error(EcError::UnexpectedCommand);
            }
            if self.slave_address.is_some() && wkc != 1 {
                self.state = State::Error(EcError::UnexpectedWKC(wkc));
            } else if self.slave_address.is_none() && wkc != desc.len() as u16 {
                self.state = State::Error(EcError::UnexpectedWKC(wkc));
            }
            data
        } else {
            self.state = State::Error(EcError::LostCommand);
            return;
        };

        match self.state {
            State::Idle => {}
            State::Error(_) => {}
            State::Read => {
                self.buffer.iter_mut().zip(data).for_each(|(b, d)| *b = *d);
                self.state = State::Complete;
            }
            State::Write => {
                self.buf_size = 0;
                self.state = State::Complete;
            }
            State::Complete => {}
        }
    }
}
