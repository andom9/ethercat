use super::slave_initializer::SlaveInitializerError;
use super::SlaveInitializer;
use super::TaskError;
use super::{Cyclic, EtherCatSystemTime};
use crate::frame::CommandType;
use crate::interface::*;
use crate::network::FmmuConfig;
use crate::network::NetworkDescription;
use crate::network::Slave;
use crate::register::DlControl;
use crate::register::SyncManagerStatus;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkInitializerError {
    Init(TaskError<SlaveInitializerError>),
    TooManySlaves,
}

impl From<NetworkInitializerError> for TaskError<NetworkInitializerError> {
    fn from(err: NetworkInitializerError) -> Self {
        Self::TaskSpecific(err)
    }
}

impl From<TaskError<SlaveInitializerError>> for NetworkInitializerError {
    fn from(err: TaskError<SlaveInitializerError>) -> Self {
        Self::Init(err)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum State {
    Idle,
    Error(TaskError<NetworkInitializerError>),
    CountSlaves,
    StartInitSlaves(u16),
    WaitInitSlaves(u16),
    //SetMBoxStateFmmu((u16, usize)),
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
    pub const fn required_buffer_size() -> usize {
        buffer_size()
    }

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

    pub fn wait(&mut self) -> Option<Result<(), TaskError<NetworkInitializerError>>> {
        match &self.state {
            State::Complete => Some(Ok(())),
            State::Error(err) => Some(Err(err.clone())),
            _ => None,
        }
    }
}

impl<'a, 'b, 'c, 'd> Cyclic for NetworkInitializer<'a, 'b, 'c, 'd> {
    fn is_finished(&self) -> bool {
        match self.state {
            State::Complete | State::Error(_) => true,
            _ => false,
        }
    }

    fn next_command(&mut self, buf: &mut [u8]) -> Option<(Command, usize)> {
        let command_and_data = match &self.state {
            State::Idle => None,
            State::Error(_) => None,
            State::CountSlaves => {
                let command = Command::new(CommandType::BWR, 0, DlControl::ADDRESS);
                buf[..DlControl::SIZE].fill(0);
                // ループポートを設定する。
                // ・EtherCat以外のフレームを削除する。
                // ・ソースMACアドレスを変更して送信する。
                // ・ポートを自動開閉する。
                let mut dl_control = DlControl(buf);
                dl_control.set_forwarding_rule(true);
                dl_control.set_tx_buffer_size(7);
                Some((command, DlControl::SIZE))
            }
            State::StartInitSlaves(count) => {
                self.initilizer.start(*count);
                self.initilizer.next_command(buf)
            }
            State::WaitInitSlaves(_) => self.initilizer.next_command(buf),
            State::Complete => None,
        };
        if let Some((command, _)) = command_and_data {
            self.command = command;
        }
        command_and_data
    }

    fn recieve_and_process(&mut self, recv_data: &CommandData, sys_time: EtherCatSystemTime) {
        let wkc = {
            let CommandData { command, wkc, .. } = recv_data;
            if !(command.c_type == self.command.c_type && command.ado == self.command.ado) {
                self.state = State::Error(TaskError::UnexpectedCommand);
            }
            *wkc
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
                        *slave.info_mut() = slave_info;
                        slave.set_mailbox_count(1).expect("unreachable");
                        //3番目のfmmuはメールボックスのポーリングに使う
                        if 2 <= slave.info().number_of_fmmu()
                            && slave.info().mailbox_tx_sm().is_some()
                        {
                            let tx_sm_number = slave.info().mailbox_tx_sm().unwrap().number();
                            let mb_tx_sm_status =
                                SyncManagerStatus::ADDRESS + 0x08 * tx_sm_number as u16;
                            let bit_length = SyncManagerStatus::SIZE * 8;
                            slave.fmmu_config_mut()[2] =
                                Some(FmmuConfig::new(mb_tx_sm_status, bit_length as u16, false));
                        }
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
