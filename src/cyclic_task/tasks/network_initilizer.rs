use super::super::interface::*;
use super::slave_initializer::SlaveInitializerError;
use super::SlaveInitializer;
use crate::cyclic_task::Cyclic;
use crate::error::EcError;
use crate::frame::CommandType;
use crate::register::DlControl;
use crate::register::SyncManagerStatus;
use crate::slave_network::FmmuConfig;
use crate::slave_network::NetworkDescription;
use crate::slave_network::Slave;

use super::super::CommandData;
use super::super::EtherCatSystemTime;

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, PartialEq)]
enum State {
    Idle,
    Error(EcError<NetworkInitializerError>),
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

    pub fn wait(&mut self) -> Option<Result<(), EcError<NetworkInitializerError>>> {
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
        log::info!("send {:?}", self.state);

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
            // State::SetMBoxStateFmmu((slave_count, add_count)) => {
            //     // FMMU2でメールボックスステータスをポーリングする。
            //     let addr = SlaveAddress::SlavePosition(*slave_count);
            //     if let Some(tx_sm) = self
            //         .network
            //         .slave(addr)
            //         .filter(|s| 3 <= s.info().number_of_fmmu)
            //         .and_then(|s| s.info().mailbox_tx_sm())
            //     {
            //         let mb_tx_sm_address = SyncManagerStatus::ADDRESS + 0x08 * tx_sm.number as u16;
            //         buf[..FmmuRegister::SIZE].fill(0);
            //         let mut fmmu2 = FmmuRegister(buf);
            //         let logical_address = *add_count as u32 * SyncManagerStatus::SIZE as u32;
            //         self.network
            //             .slave_mut(addr)
            //             .unwrap()
            //             .fmmu2_process_data = Some(ProcessDataConfig::Memory(MemoryProcessData{
            //                 logical_start_address: Some(logical_address),
            //                 address: mb_tx_sm_address,
            //                 bit_length: (SyncManagerStatus::SIZE *8) as u16,
            //             }));
            //         fmmu2.set_logical_start_address(logical_address);
            //         fmmu2.set_length(SyncManagerStatus::SIZE as u16);
            //         fmmu2.set_logical_start_address(0);
            //         fmmu2.set_logical_end_bit(0);
            //         fmmu2.set_physical_start_address(mb_tx_sm_address);
            //         fmmu2.set_physical_start_bit(0);
            //         fmmu2.set_read_enable(false);
            //         fmmu2.set_write_enable(true);
            //         fmmu2.set_enable(true);
            //         let fmmu_reg_addr = FmmuRegister::ADDRESS + 2 * FmmuRegister::SIZE as u16;
            //         let command = Command::new_write(addr.into(), fmmu_reg_addr);
            //         self.state = State::SetMBoxStateFmmu((*slave_count, add_count + 1));
            //         Some((command, FmmuRegister::SIZE))
            //     } else {
            //         // clear
            //         buf[..FmmuRegister::SIZE].fill(0);
            //         let fmmu_reg_addr = FmmuRegister::ADDRESS + 2 * FmmuRegister::SIZE as u16;
            //         let command = Command::new_write(addr.into(), fmmu_reg_addr);
            //         Some((command, FmmuRegister::SIZE))
            //     }
            // }
            State::Complete => None,
        };
        if let Some((command, _)) = command_and_data {
            self.command = command;
        }
        command_and_data
    }

    fn recieve_and_process(
        &mut self,
        recv_data: Option<&CommandData>,
        sys_time: EtherCatSystemTime,
    ) {
        //log::info!("recv {:?}",self.state);

        let wkc = if let Some(ref recv_data) = recv_data {
            let CommandData { command, wkc, .. } = recv_data;
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
                        slave.mailbox_count.set(1);
                        //3番目のfmmuはメールボックスのポーリングに使う
                        if 2 <= slave.info.number_of_fmmu && slave.info.mailbox_tx_sm().is_some() {
                            let tx_sm_number = slave.info.mailbox_tx_sm().unwrap().number;
                            let mb_tx_sm_status =
                                SyncManagerStatus::ADDRESS + 0x08 * tx_sm_number as u16;
                            let bit_length = SyncManagerStatus::SIZE * 8;
                            slave.fmmu[2] =
                                Some(FmmuConfig::new(mb_tx_sm_status, bit_length as u16, true));
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
            // State::SetMBoxStateFmmu((slave_count, add_count)) => {
            //     if *slave_count + 1 < self.num_slaves {
            //         self.state = State::SetMBoxStateFmmu((*slave_count + 1, *add_count));
            //     } else {
            //         self.state = State::Complete;
            //     }
            // }
            State::Complete => {}
        }
    }
}

const fn buffer_size() -> usize {
    DlControl::SIZE
}
