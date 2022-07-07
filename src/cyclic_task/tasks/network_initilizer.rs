use super::slave_initializer::SlaveInitializerError;
use super::SlaveInitializer;
use crate::cyclic_task::CyclicProcess;
use crate::error::EcError;
use crate::frame::CommandType;
use super::super::interface::*;
use crate::slave_network::NetworkDescription;
use crate::register::DlControl;
use crate::slave_network::Slave;

use super::super::EtherCatSystemTime;
use super::super::ReceivedData;

#[derive(Debug, Clone)]
pub enum NetworkInitializerError {
    Init(EcError<SlaveInitializerError>),
    TooManySlaves,
}

impl From<NetworkInitializerError> for EcError<NetworkInitializerError> {
    fn from(err: NetworkInitializerError) -> Self {
        Self::TaskSpecific(err)
    }
}

impl From<EcError<SlaveInitializerError>> for NetworkInitializerError {
    fn from(err: EcError<SlaveInitializerError>) -> Self {
        Self::Init(err)
    }
}

#[derive(Debug)]
enum State {
    Idle,
    Error(EcError<NetworkInitializerError>),
    CountSlaves,
    StartInitSlaves(u16),
    WaitInitSlaves(u16),
    Complete,
}

#[derive(Debug)]
pub struct NetworkInitializer<'a, 'b, 'c, 'd> {
    initilizer: SlaveInitializer,
    network: &'d mut NetworkDescription<'a, 'b, 'c>,
    state: State,
    command: Command,
    buffer: [u8; buffer_size()],
    num_slaves: u16,
    lost_count: u8,
}

impl<'a, 'b, 'c, 'd> NetworkInitializer<'a, 'b, 'c, 'd> {
    pub fn new(network: &'d mut NetworkDescription<'a, 'b, 'c>) -> Self {
        Self {
            initilizer: SlaveInitializer::new(),
            state: State::Idle,
            command: Command::default(),
            buffer: [0; buffer_size()],
            num_slaves: 0,
            lost_count: 0,
            network,
        }
    }

    pub fn take(self) -> &'d mut NetworkDescription<'a, 'b, 'c> {
        self.network
    }

    pub fn start(&mut self) {
        self.buffer.fill(0);
        self.command = Command::default();

        self.state = State::CountSlaves;
        self.lost_count = 0;
    }

    pub fn wait(&mut self) -> Option<Result<(), EcError<NetworkInitializerError>>> {
        match &self.state {
            State::Complete => Some(Ok(())),
            State::Error(err) => Some(Err(err.clone())),
            _ => None,
        }
    }
}

impl<'a, 'b, 'c, 'd> CyclicProcess for NetworkInitializer<'a, 'b, 'c, 'd> {
    fn next_command(&mut self, sys_time: EtherCatSystemTime) -> Option<(Command, &[u8])> {
        log::info!("send {:?}", self.state);

        let command_and_data = match &self.state {
            State::Idle => None,
            State::Error(_) => None,
            State::CountSlaves => {
                let command = Command::new(CommandType::BWR, 0, DlControl::ADDRESS);
                self.buffer.fill(0);
                // ループポートを設定する。
                // ・EtherCat以外のフレームを削除する。
                // ・ソースMACアドレスを変更して送信する。
                // ・ポートを自動開閉する。
                let mut dl_control = DlControl(&mut self.buffer);
                dl_control.set_forwarding_rule(true);
                dl_control.set_tx_buffer_size(7);
                Some((command, &self.buffer[..DlControl::SIZE]))
            }
            State::StartInitSlaves(count) => {
                self.initilizer.start(*count);
                self.initilizer.next_command(sys_time)
            }
            State::WaitInitSlaves(_) => self.initilizer.next_command(sys_time),
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
        sys_time: EtherCatSystemTime,
    ) {
        //log::info!("recv {:?}",self.state);

        let wkc = if let Some(ref recv_data) = recv_data {
            let ReceivedData { command, wkc, .. } = recv_data;
            if !(command.c_type == self.command.c_type && command.ado == self.command.ado) {
                self.state = State::Error(EcError::UnexpectedCommand);
            }
            *wkc
        } else {
            if self.lost_count > 0 {
                self.state = State::Error(EcError::LostPacket);
                return;
            } else {
                self.lost_count += 1;
                return;
            }
        };

        match &mut self.state {
            State::Idle => {}
            State::Error(_) => {}
            State::CountSlaves => {
                self.num_slaves = wkc;
                self.network.clear();
                if wkc == 0 {
                    self.state = State::Complete;
                } else {
                    self.state = State::StartInitSlaves(0);
                }
            }
            State::StartInitSlaves(count) => {
                self.initilizer.recieve_and_process(recv_data, sys_time);
                self.state = State::WaitInitSlaves(*count);
            }
            State::WaitInitSlaves(count) => {
                self.initilizer.recieve_and_process(recv_data, sys_time);

                match self.initilizer.wait() {
                    Some(Ok(Some(slave_info))) => {
                        let mut slave = Slave::default();
                        slave.info = slave_info;
                        if self.network.push_slave(slave).is_err() {
                            self.state =
                                State::Error(NetworkInitializerError::TooManySlaves.into());
                        } else if *count + 1 < self.num_slaves {
                            self.state = State::StartInitSlaves(*count + 1);
                        } else {
                            self.state = State::Complete;
                        }
                    }
                    Some(Ok(None)) => unreachable!(),
                    None => {}
                    Some(Err(err)) => {
                        self.state = State::Error(NetworkInitializerError::Init(err).into());
                    }
                }
            }
            State::Complete => {}
        }
    }
}

const fn buffer_size() -> usize {
    DlControl::SIZE
}
