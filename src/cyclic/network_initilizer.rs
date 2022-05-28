use crate::cyclic::Cyclic;
use crate::cyclic::{al_state_transfer::*, sii_reader::*, slave_initializer::*};
use crate::error::*;
use crate::interface::*;
use crate::network::*;
use crate::packet::ethercat::CommandType;
use crate::register::{application::*, datalink::*};
use crate::slave::*;
use crate::util::*;
use bit_field::BitField;
use embedded_hal::timer::*;
use fugit::*;
use heapless::Vec;
use nb;

use super::EtherCATSystemTime;

#[derive(Debug, Clone)]
pub enum NetworkInitError {
    Common(CommonError),
    Init(InitError),
    TooManySlaves,
}

impl From<CommonError> for NetworkInitError {
    fn from(err: CommonError) -> Self {
        Self::Common(err)
    }
}

impl From<InitError> for NetworkInitError {
    fn from(err: InitError) -> Self {
        Self::Init(err)
    }
}

#[derive(Debug)]
enum NetworkInitilizerState {
    Idle,
    Error(NetworkInitError),
    CountSlaves,
    StartInitSlaves(u16),
    WaitInitSlaves(u16),
    Complete,
}

#[derive(Debug)]
pub struct NetworkInitilizer {
    initilizer: SlaveInitilizer,
    state: NetworkInitilizerState,
    command: Command,
    buffer: [u8; buffer_size()],
    num_slaves: u16,
}

impl NetworkInitilizer {
    pub fn new() -> Self {
        Self {
            initilizer: SlaveInitilizer::new(),
            state: NetworkInitilizerState::Idle,
            command: Command::default(),
            buffer: [0; buffer_size()],
            num_slaves: 0,
        }
    }

    pub fn start(&mut self) {
        //if let Some(initilizer) = core::mem::take(&mut self.initilizer) {
        //    self.timer = Some(initilizer.take_timer());
        //}
        self.buffer.fill(0);
        self.command = Command::default();

        self.state = NetworkInitilizerState::CountSlaves;
    }

    //pub fn reset(&mut self){
    //    if let Some(initilizer) = core::mem::take(&mut self.initilizer){
    //        self.timer = Some(initilizer.take_timer());
    //    }
    //    self.state = NetworkInitilizerState::Idle;
    //    self.buffer.fill(0);
    //    self.command = Command::default();
    //    self.network = None;
    //}

    //pub fn error(&self) -> Option<NetworkInitError> {
    //    if let NetworkInitilizerState::Error(err) = &self.state {
    //        Some(err.clone())
    //    } else {
    //        None
    //    }
    //}

    pub fn wait(&mut self) -> nb::Result<(), NetworkInitError> {
        if let NetworkInitilizerState::Error(err) = &self.state {
            Err(nb::Error::Other(err.clone()))
        } else {
            if let NetworkInitilizerState::Complete = self.state {
                Ok(())
                //Ok(core::mem::take(&mut self.network))
            } else {
                Err(nb::Error::WouldBlock)
            }
        }
    }
}

impl Cyclic for NetworkInitilizer {
    fn next_command(
        &mut self,
        desc: &mut NetworkDescription,
        sys_time: EtherCATSystemTime,
    ) -> Option<(Command, &[u8])> {
        let command_and_data = match &self.state {
            NetworkInitilizerState::Idle => None,
            NetworkInitilizerState::Error(_) => None,
            NetworkInitilizerState::CountSlaves => {
                let command = Command::new(CommandType::BRD, 0, 0);
                self.buffer.fill(0);
                Some((command, &self.buffer[..1]))
            }
            NetworkInitilizerState::StartInitSlaves(count) => {
                //let timer = core::mem::take(&mut self.timer).unwrap();
                //self.initilizer = Some(SlaveInitilizer::new(timer));
                //let initilizer = self.initilizer.as_mut().unwrap();
                self.initilizer.start(*count);
                self.initilizer.next_command(desc, sys_time)
            }
            NetworkInitilizerState::WaitInitSlaves(_) => {
                self.initilizer.next_command(desc, sys_time)
            }
            NetworkInitilizerState::Complete => None,
        };
        if let Some((command, _)) = command_and_data {
            self.command = command;
        }
        command_and_data
    }

    fn recieve_and_process(
        &mut self,
        command: Command,
        data: &[u8],
        wkc: u16,
        desc: &mut NetworkDescription,
        sys_time: EtherCATSystemTime,
    ) -> bool {
        if command != self.command {
            self.state =
                NetworkInitilizerState::Error(NetworkInitError::Common(CommonError::PacketDropped));
        }

        match &mut self.state {
            NetworkInitilizerState::Idle => {}
            NetworkInitilizerState::Error(_) => {}
            NetworkInitilizerState::CountSlaves => {
                self.num_slaves = wkc;
                desc.clear();
                if wkc <= 0 {
                    self.state = NetworkInitilizerState::Complete;
                } else {
                    self.state = NetworkInitilizerState::StartInitSlaves(0);
                }
            }
            NetworkInitilizerState::StartInitSlaves(count) => {
                self.initilizer
                    .recieve_and_process(command, data, wkc, desc, sys_time);
                self.state = NetworkInitilizerState::WaitInitSlaves(*count);
            }
            NetworkInitilizerState::WaitInitSlaves(count) => {
                self.initilizer
                    .recieve_and_process(command, data, wkc, desc, sys_time);
                match self.initilizer.wait() {
                    Ok(Some(slave)) => {
                        //if let Some(existed_slave) = desc.slave_mut(*count){
                        //    *existed_slave = slave;
                        //}else{
                        if desc.push_slave(slave).is_err() {
                            self.state =
                                NetworkInitilizerState::Error(NetworkInitError::TooManySlaves);
                        } else {
                            if *count + 1 < self.num_slaves {
                                self.state = NetworkInitilizerState::StartInitSlaves(*count + 1);
                            } else {
                                self.state = NetworkInitilizerState::Complete;
                            }
                        }
                        //}
                    }
                    Ok(None) => unreachable!(),
                    Err(nb::Error::WouldBlock) => {}
                    Err(nb::Error::Other(err)) => {
                        self.state = NetworkInitilizerState::Error(err.into());
                    }
                }
            }
            NetworkInitilizerState::Complete => {}
        }

        if let NetworkInitilizerState::Error(_) = self.state {
            false
        } else {
            true
        }
    }
}

const fn buffer_size() -> usize {
    1
}
