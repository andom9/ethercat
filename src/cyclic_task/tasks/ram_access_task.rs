use super::super::interface::*;
use super::super::EtherCatSystemTime;
use super::super::ReceivedData;
use crate::cyclic_task::CyclicProcess;
use crate::error::EcError;

#[derive(Debug)]
enum State {
    Error(EcError<()>),
    Idle,
    Read,
    Write,
    Complete,
}

#[derive(Debug)]
pub struct RamAccessTask {
    state: State,
    slave_address: TargetSlave,
    command: Command,
    buffer: [u8; 16],
    buf_size: usize,
    ado: u16,
}

impl RamAccessTask {
    pub fn new() -> Self {
        Self {
            state: State::Idle,
            slave_address: TargetSlave::default(),
            command: Command::default(),
            buffer: [0; 16],
            buf_size: 0,
            ado: 0,
        }
    }

    pub fn start_to_read(&mut self, slave_address: TargetSlave, ado: u16, buf_size: usize) {
        assert!(buf_size <= 16);
        self.slave_address = slave_address;
        self.state = State::Read;
        self.buffer.fill(0);
        self.command = Command::default();
        self.buf_size = buf_size;
        self.ado = ado;
    }

    pub fn start_to_write(&mut self, slave_address: TargetSlave, ado: u16, data: &[u8]) {
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

impl CyclicProcess for RamAccessTask {
    fn next_command(&mut self, _: EtherCatSystemTime) -> Option<(Command, &[u8])> {
        match self.state {
            State::Idle => None,
            State::Error(_) => None,
            State::Read => {
                self.command = Command::new_read(self.slave_address, self.ado);
                Some((self.command, &self.buffer[..self.buf_size]))
            }
            State::Write => {
                self.command = Command::new_write(self.slave_address, self.ado);
                Some((self.command, &self.buffer[..self.buf_size]))
            }
            State::Complete => None,
        }
    }

    fn recieve_and_process(&mut self, recv_data: Option<ReceivedData>, _: EtherCatSystemTime) {
        let data = if let Some(recv_data) = recv_data {
            let ReceivedData { command, data, wkc } = recv_data;
            if !(command.c_type == self.command.c_type && command.ado == self.command.ado) {
                self.state = State::Error(EcError::UnexpectedCommand);
            }
            match self.slave_address.into() {
                TargetSlave::Single(_slave_address) => {
                    if wkc != 1 {
                        self.state = State::Error(EcError::UnexpectedWkc(wkc));
                    }
                }
                TargetSlave::All(num_slaves) => {
                    if wkc != num_slaves {
                        self.state = State::Error(EcError::UnexpectedWkc(wkc));
                    }
                }
            }
            data
        } else {
            self.state = State::Error(EcError::LostPacket);
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
