use super::TaskError;
use super::{CyclicTask, EtherCatSystemTime};
use crate::interface::*;

#[derive(Debug, PartialEq)]
enum State {
    Error(TaskError<()>),
    Idle,
    Read,
    Write,
    Complete,
}

#[derive(Debug)]
pub struct AddressAccessTask {
    state: State,
    slave_address: TargetSlave,
    command: Command,
    data_size: usize,
    ado: u16,
}

impl AddressAccessTask {
    pub fn data_size(&self) -> usize {
        self.data_size
    }

    pub fn new() -> Self {
        Self {
            state: State::Idle,
            slave_address: TargetSlave::default(),
            command: Command::default(),
            data_size: 0,
            ado: 0,
        }
    }

    pub fn start_to_read(&mut self, slave_address: TargetSlave, ado: u16, data_size: usize) {
        self.slave_address = slave_address;
        self.state = State::Read;
        self.command = Command::default();
        self.data_size = data_size;
        self.ado = ado;
    }

    pub fn start_to_write(
        &mut self,
        slave_address: TargetSlave,
        ado: u16,
        data: &[u8],
        buf: &mut [u8],
    ) {
        let size = data.len();
        assert!(size <= buf.len());
        buf[..size].iter_mut().zip(data).for_each(|(b, d)| *b = *d);
        self.slave_address = slave_address;
        self.state = State::Write;
        self.command = Command::default();
        self.data_size = size;
        self.ado = ado;
    }

    pub fn wait(&mut self) -> Option<Result<(), TaskError<()>>> {
        match &self.state {
            State::Complete => Some(Ok(())),
            State::Error(err) => Some(Err(err.clone())),
            _ => None,
        }
    }
}

impl CyclicTask for AddressAccessTask {
    fn is_finished(&self) -> bool {
        match self.state {
            State::Complete | State::Error(_) => true,
            _ => false,
        }
    }

    fn next_pdu(&mut self, _buf: &mut [u8]) -> Option<(Command, usize)> {
        match self.state {
            State::Idle => None,
            State::Error(_) => None,
            State::Read => {
                self.command = Command::new_read(self.slave_address, self.ado);
                Some((self.command, self.data_size))
            }
            State::Write => {
                self.command = Command::new_write(self.slave_address, self.ado);
                Some((self.command, self.data_size))
            }
            State::Complete => None,
        }
    }

    fn recieve_and_process(&mut self, recv_data: &Pdu, _sys_time: EtherCatSystemTime) {
        let Pdu { command, wkc, .. } = recv_data;
        if !(command.c_type == self.command.c_type && command.ado == self.command.ado) {
            self.state = State::Error(TaskError::UnexpectedCommand);
        }
        match self.slave_address {
            TargetSlave::Single(_slave_address) => {
                if *wkc != 1 {
                    self.state = State::Error(TaskError::UnexpectedWkc((1, *wkc).into()));
                }
            }
            TargetSlave::All(num_slaves) => {
                if *wkc != num_slaves {
                    self.state = State::Error(TaskError::UnexpectedWkc((num_slaves, *wkc).into()));
                }
            }
        }

        match self.state {
            State::Idle => {}
            State::Error(_) => {}
            State::Read => {
                self.state = State::Complete;
            }
            State::Write => {
                self.data_size = 0;
                self.state = State::Complete;
            }
            State::Complete => {}
        }
    }
}
