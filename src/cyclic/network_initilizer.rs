use super::slave_initializer;
use crate::cyclic::slave_initializer::SlaveInitilizer;
use crate::cyclic::Cyclic;
use crate::error::CommonError;
use crate::interface::Command;
use crate::network::NetworkDescription;
use crate::packet::ethercat::CommandType;
use nb;

use super::EtherCatSystemTime;
use super::ReceivedData;

#[derive(Debug, Clone)]
pub enum Error {
    Common(CommonError),
    Init(slave_initializer::Error),
    TooManySlaves,
}

impl From<CommonError> for Error {
    fn from(err: CommonError) -> Self {
        Self::Common(err)
    }
}

impl From<slave_initializer::Error> for Error {
    fn from(err: slave_initializer::Error) -> Self {
        Self::Init(err)
    }
}

#[derive(Debug)]
enum State {
    Idle,
    Error(Error),
    CountSlaves,
    StartInitSlaves(u16),
    WaitInitSlaves(u16),
    Complete,
}

#[derive(Debug)]
pub struct NetworkInitilizer {
    initilizer: SlaveInitilizer,
    state: State,
    command: Command,
    buffer: [u8; buffer_size()],
    num_slaves: u16,
}

impl NetworkInitilizer {
    pub fn new() -> Self {
        Self {
            initilizer: SlaveInitilizer::new(),
            state: State::Idle,
            command: Command::default(),
            buffer: [0; buffer_size()],
            num_slaves: 0,
        }
    }

    pub fn start(&mut self) {
        self.buffer.fill(0);
        self.command = Command::default();

        self.state = State::CountSlaves;
    }

    pub fn wait(&mut self) -> nb::Result<(), Error> {
        if let State::Error(err) = &self.state {
            Err(nb::Error::Other(err.clone()))
        } else if let State::Complete = self.state {
            Ok(())
        } else {
            Err(nb::Error::WouldBlock)
        }
    }
}

impl Cyclic for NetworkInitilizer {
    fn next_command(
        &mut self,
        desc: &mut NetworkDescription,
        sys_time: EtherCatSystemTime,
    ) -> Option<(Command, &[u8])> {
        let command_and_data = match &self.state {
            State::Idle => None,
            State::Error(_) => None,
            State::CountSlaves => {
                let command = Command::new(CommandType::BRD, 0, 0);
                self.buffer.fill(0);
                Some((command, &self.buffer[..1]))
            }
            State::StartInitSlaves(count) => {
                self.initilizer.start(*count);
                self.initilizer.next_command(desc, sys_time)
            }
            State::WaitInitSlaves(_) => self.initilizer.next_command(desc, sys_time),
            State::Complete => None,
        };
        if let Some((command, _)) = command_and_data {
            self.command = command;
        }
        command_and_data
    }

    fn recieve_and_process(
        &mut self,
        recv_data: Option<ReceivedData>,
        desc: &mut NetworkDescription,
        sys_time: EtherCatSystemTime,
    ) {
        let wkc = if let Some(ref recv_data) = recv_data {
            let ReceivedData { command, wkc, .. } = recv_data;
            if *command != self.command {
                self.state = State::Error(Error::Common(CommonError::BadPacket));
            }
            *wkc
        } else {
            self.state = State::Error(Error::Common(CommonError::LostCommand));
            return;
        };

        match &mut self.state {
            State::Idle => {}
            State::Error(_) => {}
            State::CountSlaves => {
                self.num_slaves = wkc;
                desc.clear();
                if wkc == 0 {
                    self.state = State::Complete;
                } else {
                    self.state = State::StartInitSlaves(0);
                }
            }
            State::StartInitSlaves(count) => {
                self.initilizer
                    .recieve_and_process(recv_data, desc, sys_time);
                self.state = State::WaitInitSlaves(*count);
            }
            State::WaitInitSlaves(count) => {
                self.initilizer
                    .recieve_and_process(recv_data, desc, sys_time);
                match self.initilizer.wait() {
                    Ok(Some(slave)) => {
                        if desc.push_slave(slave).is_err() {
                            self.state = State::Error(Error::TooManySlaves);
                        } else if *count + 1 < self.num_slaves {
                            self.state = State::StartInitSlaves(*count + 1);
                        } else {
                            self.state = State::Complete;
                        }
                    }
                    Ok(None) => unreachable!(),
                    Err(nb::Error::WouldBlock) => {}
                    Err(nb::Error::Other(err)) => {
                        self.state = State::Error(err.into());
                    }
                }
            }
            State::Complete => {}
        }
    }
}

const fn buffer_size() -> usize {
    1
}
