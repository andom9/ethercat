use super::slave_initializer;
use crate::cyclic::slave_initializer::SlaveInitilizer;
use crate::cyclic::CyclicProcess;
use crate::error::EcError;
use crate::interface::Command;
use crate::network::NetworkDescription;
use crate::packet::ethercat::CommandType;
use crate::register::datalink::DlControl;

use super::EtherCatSystemTime;
use super::ReceivedData;

#[derive(Debug, Clone)]
pub enum Error {
    Init(EcError<slave_initializer::Error>),
    TooManySlaves,
}

impl From<Error> for EcError<Error> {
    fn from(err: Error) -> Self {
        Self::UnitSpecific(err)
    }
}

impl From<EcError<slave_initializer::Error>> for Error {
    fn from(err: EcError<slave_initializer::Error>) -> Self {
        Self::Init(err)
    }
}

#[derive(Debug)]
enum State {
    Idle,
    Error(EcError<Error>),
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
    lost_count: usize,
}

impl NetworkInitilizer {
    pub fn new() -> Self {
        Self {
            initilizer: SlaveInitilizer::new(),
            state: State::Idle,
            command: Command::default(),
            buffer: [0; buffer_size()],
            num_slaves: 0,
            lost_count: 0,
        }
    }

    pub fn start(&mut self) {
        self.buffer.fill(0);
        self.command = Command::default();

        self.state = State::CountSlaves;
        self.lost_count = 0;
    }

    pub fn wait(&mut self) -> Option<Result<(), EcError<Error>>> {
        match &self.state {
            State::Complete => Some(Ok(())),
            State::Error(err) => Some(Err(err.clone())),
            //State::Idle => Err(EcError::NotStarted.into()),
            _ => None,
        }
    }
}

impl CyclicProcess for NetworkInitilizer {
    fn next_command(
        &mut self,
        desc: &mut NetworkDescription,
        sys_time: EtherCatSystemTime,
    ) -> Option<(Command, &[u8])> {
        //log::info!("send {:?}",self.state);

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
        //log::info!("recv {:?}",self.state);

        let wkc = if let Some(ref recv_data) = recv_data {
            let ReceivedData { command, wkc, .. } = recv_data;
            if !(command.c_type == self.command.c_type && command.ado == self.command.ado) {
                self.state = State::Error(EcError::UnexpectedCommand);
            }
            *wkc
        } else {
            //self.state = State::Error(EcError::LostCommand);
            //return;
            if self.lost_count > 0 {
                self.state = State::Error(EcError::LostCommand);
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
                    Some(Ok(Some(slave))) => {
                        if desc.push_slave(slave).is_err() {
                            self.state = State::Error(Error::TooManySlaves.into());
                        } else if *count + 1 < self.num_slaves {
                            self.state = State::StartInitSlaves(*count + 1);
                        } else {
                            self.state = State::Complete;
                        }
                    }
                    Some(Ok(None)) => unreachable!(),
                    None => {}
                    Some(Err(err)) => {
                        self.state = State::Error(Error::Init(err).into());
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
