use super::super::interface::*;
use super::super::CommandData;
use super::super::EtherCatSystemTime;
use crate::cyclic_task::Cyclic;
use crate::error::EcError;
use crate::register::AlStatus;
use crate::register::AlStatusCode;
use crate::slave_network::AlState;
use crate::util::const_max;

#[derive(Debug, Clone, PartialEq)]
enum State {
    Error(EcError<()>),
    Idle,
    Read,
    Complete,
}

#[derive(Debug)]
pub struct AlStateReader {
    state: State,
    slave_address: TargetSlave,
    command: Command,
    //buffer: [u8; buffer_size()],
    current_al_state: AlState,
    current_al_status_code: Option<AlStatusCode>,
}

impl AlStateReader {
    pub const fn required_buffer_size() -> usize {
        buffer_size()
    }

    pub fn new() -> Self {
        Self {
            state: State::Idle,
            slave_address: TargetSlave::default(),
            command: Command::default(),
            //buffer: [0; buffer_size()],
            current_al_state: AlState::Init,
            current_al_status_code: None,
        }
    }

    pub fn start(&mut self, slave_address: TargetSlave) {
        self.slave_address = slave_address;
        self.state = State::Read;
        //self.buffer.fill(0);
        self.command = Command::default();
    }

    pub fn wait(&mut self) -> Option<Result<(AlState, Option<AlStatusCode>), EcError<()>>> {
        match &self.state {
            State::Complete => Some(Ok((self.current_al_state, self.current_al_status_code))),
            State::Error(err) => Some(Err(err.clone())),
            _ => None,
        }
    }
}

impl Cyclic for AlStateReader {
    fn is_finished(&self) -> bool {
        self.state == State::Complete
    }
    fn next_command(&mut self, buf: &mut [u8]) -> Option<(Command, usize)> {
        match self.state {
            State::Idle => None,
            State::Error(_) => None,
            State::Read => {
                self.command = Command::new_read(self.slave_address, AlStatus::ADDRESS);
                //self.buffer.fill(0);
                buf[..AlStatus::SIZE].fill(0);
                Some((self.command, AlStatus::SIZE))
            }
            State::Complete => None,
        }
    }

    fn recieve_and_process(&mut self, recv_data: Option<&CommandData>, _: EtherCatSystemTime) {
        let data = if let Some(recv_data) = recv_data {
            let CommandData { command, data, wkc } = recv_data;
            let wkc = *wkc;
            if !(command.c_type == self.command.c_type && command.ado == self.command.ado) {
                self.state = State::Error(EcError::UnexpectedCommand);
            }
            match self.slave_address {
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
            State::Complete => {}
            State::Read => {
                let al_status = AlStatus(data);
                let al_state = AlState::from(al_status.state());
                self.current_al_state = al_state;
                if al_status.change_err() {
                    self.current_al_status_code = Some(al_status.get_al_status_code());
                }
                self.state = State::Complete;
            }
        }
    }
}

const fn buffer_size() -> usize {
    let mut size = 0;
    size = const_max(size, AlStatus::SIZE);
    size
}
