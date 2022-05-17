use crate::cyclic::Cyclic;
use crate::cyclic::{al_state_transfer::*, sii::*, slave_initializer::*};
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
pub struct NetworkInitilizer<'a, T, const N: usize>
where
    T: CountDown<Time = MicrosDurationU32>,
{
    timer: Option<&'a mut T>,
    initilizer: Option<SlaveInitilizer<'a, T>>,
    state: NetworkInitilizerState,
    command: Command,
    buffer: [u8; buffer_size()],
    network: Option<EtherCATNetwork<N>>,
}

impl<'a, T, const N: usize> NetworkInitilizer<'a, T, N>
where
    T: CountDown<Time = MicrosDurationU32>,
{
    pub fn new(timer: &'a mut T) -> Self {
        Self {
            timer: Some(timer),
            initilizer: None,
            state: NetworkInitilizerState::Idle,
            command: Command::default(),
            buffer: [0; buffer_size()],
            network: None,
        }
    }

    pub fn start(&mut self) {
        if let Some(initilizer) = core::mem::take(&mut self.initilizer) {
            self.timer = Some(initilizer.take_timer());
        }
        self.buffer.fill(0);
        self.command = Command::default();

        self.state = NetworkInitilizerState::CountSlaves;
        self.network = Some(EtherCATNetwork::new());
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

    pub fn wait(&mut self) -> nb::Result<Option<EtherCATNetwork<N>>, NetworkInitError> {
        if let NetworkInitilizerState::Error(err) = &self.state {
            Err(nb::Error::Other(err.clone()))
        } else {
            if let NetworkInitilizerState::Complete = self.state {
                Ok(core::mem::take(&mut self.network))
            } else {
                Err(nb::Error::WouldBlock)
            }
        }
    }
}

impl<'a, T, const N: usize> Cyclic for NetworkInitilizer<'a, T, N>
where
    T: CountDown<Time = MicrosDurationU32>,
{
    fn next_command(&mut self) -> Option<(Command, &[u8])> {
        let command_and_data = match &self.state {
            NetworkInitilizerState::Idle => None,
            NetworkInitilizerState::Error(_) => None,
            NetworkInitilizerState::CountSlaves => {
                let command = Command::new(CommandType::BRD, 0, 0);
                self.buffer.fill(0);
                Some((command, &self.buffer[..1]))
            }
            NetworkInitilizerState::StartInitSlaves(count) => {
                let timer = core::mem::take(&mut self.timer).unwrap();
                self.initilizer = Some(SlaveInitilizer::new(timer));
                let initilizer = self.initilizer.as_mut().unwrap();
                initilizer.start(*count);
                initilizer.next_command()
            }
            NetworkInitilizerState::WaitInitSlaves(_) => {
                let initilizer = self.initilizer.as_mut().unwrap();
                initilizer.next_command()
            }
            NetworkInitilizerState::Complete => None,
        };
        if let Some((command, _)) = command_and_data {
            self.command = command;
        }
        command_and_data
    }

    fn recieve_and_process(&mut self, command: Command, data: &[u8], wkc: u16) -> bool {
        if command != self.command {
            self.state =
                NetworkInitilizerState::Error(NetworkInitError::Common(CommonError::PacketDropped));
        }

        match &mut self.state {
            NetworkInitilizerState::Idle => {}
            NetworkInitilizerState::Error(_) => {}
            NetworkInitilizerState::CountSlaves => {
                let mut is_error = false;
                for _ in 0..wkc {
                    if let Err(_) = self.network.as_mut().unwrap().push_slave(Slave::default()) {
                        self.state = NetworkInitilizerState::Error(NetworkInitError::TooManySlaves);
                        is_error = true;
                    }
                }
                if wkc <= 0 {
                    self.state = NetworkInitilizerState::Idle;
                } else if !is_error {
                    self.state = NetworkInitilizerState::StartInitSlaves(0);
                }
            }
            NetworkInitilizerState::StartInitSlaves(count) => {
                let initilizer = self.initilizer.as_mut().unwrap();
                initilizer.recieve_and_process(command, data, wkc);
                self.state = NetworkInitilizerState::WaitInitSlaves(*count);
                
            }
            NetworkInitilizerState::WaitInitSlaves(count) => {
                let initilizer = self.initilizer.as_mut().unwrap();
                initilizer.recieve_and_process(command, data, wkc);
                match initilizer.wait() {
                    Ok(info) => {
                        let slave = self.network.as_mut().unwrap().slave_mut(*count).unwrap();
                        slave.info = info.clone();
                        if *count + 1 < self.network.as_ref().unwrap().len() as u16 {
                            self.state = NetworkInitilizerState::StartInitSlaves(*count + 1);
                        } else {
                            self.state = NetworkInitilizerState::Complete;
                        }
                    }
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
