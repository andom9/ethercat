use super::TaskError;
use super::{CyclicTask, EtherCatSystemTime};
use crate::frame::CommandType;
use crate::interface::*;
use crate::register::{
    DcRecieveTime, DcSystemTime, DcSystemTimeDelta, DcSystemTimeOffset,
    DcSystemTimeTransmissionDelay,
};
use crate::slave::Network;
use crate::util::const_max;

//TODO:Dcスレーブについて、0x092C(システムタイムの差)を見る。

const COUNT_MAX: usize = 8;
const DRIFT_COUNT_MIN: usize = 2000;
const TARGET_DELTA_NS: u32 = 50;

#[derive(Debug, Clone, PartialEq)]
enum State {
    Idle,
    Error(TaskError<()>),
    ClearOffset,
    ClearDelay,
    RequestToLatch(usize),
    CalculateDelay((usize, u16)),
    SetOffset(u16),
    SetDelay(u16),
    CompensateDrift(usize),
    CheckDelta,
    Complete,
}

#[derive(Debug)]
pub struct DcInitTask<'a, 'b, 'c, 'd> {
    state: State,
    command: Command,
    first_dc_slave: Option<u16>,
    dc_slave_count: usize,
    network: &'d Network<'a, 'b, 'c>,
}

impl<'a, 'b, 'c, 'd> DcInitTask<'a, 'b, 'c, 'd> {
    pub const fn required_buffer_size() -> usize {
        buffer_size()
    }

    pub fn new(network: &'d Network<'a, 'b, 'c>) -> Self {
        Self {
            state: State::Idle,
            command: Command::default(),
            first_dc_slave: None,
            dc_slave_count: 0,
            network,
        }
    }

    pub fn start(&mut self) {
        self.command = Command::default();
        self.first_dc_slave = None;
        self.dc_slave_count = 0;

        for (slave, _) in self.network.slaves() {
            let mut dc = slave.dc_context.borrow_mut();
            *dc = Default::default();
        }

        // specify network topology
        let mut last_recv_slave = 0;
        let mut last_recv_port = 0;
        let mut first_slave = None;
        for recv_port in self.network.recieved_ports() {
            let slave_pos = recv_port.slave_position;
            let port_number = recv_port.port;
            let (slave, _) = self
                .network
                .slave(SlaveAddress::SlavePosition(slave_pos))
                .unwrap();
            let mut dc = slave.dc_context.borrow_mut();
            if slave.info().support_dc() && self.first_dc_slave.is_none() {
                self.first_dc_slave = Some(slave_pos);
            }
            if first_slave.is_none() {
                first_slave = Some(slave_pos);
                dc.parent_port = None;
            } else if dc.parent_port.is_none() {
                dc.parent_port = Some((last_recv_slave, last_recv_port));
            }

            last_recv_slave = slave_pos;
            last_recv_port = port_number;
        }
        for i in 0..self.network.num_slaves() {
            let (slave, _) = self
                .network
                .slave(SlaveAddress::SlavePosition(i as u16))
                .unwrap();
            if slave.info().support_dc() {
                self.dc_slave_count += 1;
            }
        }
        self.state = State::ClearOffset;
    }

    pub fn wait(&mut self) -> Option<Result<(), TaskError<()>>> {
        match &self.state {
            State::Complete => Some(Ok(())),
            State::Error(err) => Some(Err(err.clone())),
            _ => None,
        }
    }
}

impl<'a, 'b, 'c, 'd> CyclicTask for DcInitTask<'a, 'b, 'c, 'd> {
    fn is_busy(&self) -> bool {
        match self.state {
            State::Idle | State::Complete | State::Error(_) => false,
            _ => true,
        }
    }

    fn next_pdu(&mut self, buf: &mut [u8]) -> Option<(Command, usize)> {
        match &self.state {
            State::Idle => None,
            State::Error(_) => None,
            State::Complete => None,
            State::ClearOffset => {
                self.command = Command::new(CommandType::BWR, 0, DcSystemTimeOffset::ADDRESS);
                buf[..DcSystemTimeOffset::SIZE].fill(0);
                Some((self.command, DcSystemTimeOffset::SIZE))
            }
            State::ClearDelay => {
                self.command =
                    Command::new(CommandType::BWR, 0, DcSystemTimeTransmissionDelay::ADDRESS);
                buf[..DcSystemTimeTransmissionDelay::SIZE].fill(0);
                Some((self.command, DcSystemTimeTransmissionDelay::SIZE))
            }
            State::RequestToLatch(_) => {
                self.command = Command::new(CommandType::BWR, 0, DcRecieveTime::ADDRESS);
                buf[..DcRecieveTime::SIZE].fill(0);
                Some((self.command, DcRecieveTime::SIZE))
            }
            State::CalculateDelay((_, pos)) => {
                self.command = Command::new_read(
                    SlaveAddress::SlavePosition(*pos).into(),
                    DcRecieveTime::ADDRESS,
                );
                buf[..DcRecieveTime::SIZE].fill(0);
                Some((self.command, DcRecieveTime::SIZE))
            }
            State::SetOffset(pos) => {
                self.command = Command::new_write(
                    SlaveAddress::SlavePosition(*pos).into(),
                    DcSystemTimeOffset::ADDRESS,
                );
                buf[..DcSystemTimeOffset::SIZE].fill(0);
                let (slave, _) = self
                    .network
                    .slave(SlaveAddress::SlavePosition(*pos))
                    .unwrap();
                let dc = slave.dc_context.borrow();
                DcSystemTimeOffset(buf).set_system_time_offset(dc.offset);
                Some((self.command, DcSystemTimeOffset::SIZE))
            }
            State::SetDelay(pos) => {
                self.command = Command::new_write(
                    SlaveAddress::SlavePosition(*pos).into(),
                    DcSystemTimeTransmissionDelay::ADDRESS,
                );
                buf[..DcSystemTimeTransmissionDelay::SIZE].fill(0);
                let (slave, _) = self
                    .network
                    .slave(SlaveAddress::SlavePosition(*pos))
                    .unwrap();
                let dc = slave.dc_context.borrow();
                DcSystemTimeTransmissionDelay(buf).set_system_time_transmission_delay(dc.delay);
                Some((self.command, DcSystemTimeTransmissionDelay::SIZE))
            }
            State::CompensateDrift(_) => {
                self.command = Command::new(
                    CommandType::ARMW,
                    SlaveAddress::SlavePosition(self.first_dc_slave.unwrap()).get_adp(),
                    DcSystemTime::ADDRESS,
                );
                buf[..DcSystemTime::SIZE].fill(0);
                Some((self.command, DcSystemTime::SIZE))
            }
            State::CheckDelta => {
                self.command = Command::new(CommandType::BRD, 0, DcSystemTimeDelta::ADDRESS);
                buf[..DcSystemTimeDelta::SIZE].fill(0);
                Some((self.command, DcSystemTimeDelta::SIZE))
            }
        }
    }

    fn recieve_and_process(&mut self, recv_data: &Pdu, _sys_time: EtherCatSystemTime) {
        let (data, wkc) = {
            let Pdu { command, data, wkc } = recv_data;
            let wkc = *wkc;
            if !(command.c_type == self.command.c_type && command.ado == self.command.ado) {
                self.state = State::Error(TaskError::UnexpectedCommand);
            }
            (data, wkc)
        };

        match &self.state {
            State::Idle => {}
            State::Error(_) => {}
            State::Complete => {}
            State::ClearOffset => {
                let num_slaves = self.network.num_slaves() as u16;
                if wkc != num_slaves {
                    self.state = State::Error(TaskError::UnexpectedWkc((num_slaves, wkc).into()));
                } else {
                    self.state = State::ClearDelay
                }
            }
            State::ClearDelay => {
                let num_slaves = self.network.num_slaves() as u16;
                if wkc != num_slaves {
                    self.state = State::Error(TaskError::UnexpectedWkc((num_slaves, wkc).into()));
                } else {
                    self.state = State::RequestToLatch(0)
                }
            }
            State::RequestToLatch(count) => {
                let num_slaves = self.network.num_slaves() as u16;
                if wkc != num_slaves {
                    self.state = State::Error(TaskError::UnexpectedWkc((num_slaves, wkc).into()));
                } else {
                    self.state = State::CalculateDelay((*count, 0));
                }
            }
            State::CalculateDelay((count, pos)) => {
                if wkc != 1 {
                    self.state = State::Error(TaskError::UnexpectedWkc((1, wkc).into()));
                } else {
                    let recv_time = DcRecieveTime(data);
                    let (slave, _) = self
                        .network
                        .slave(SlaveAddress::SlavePosition(*pos))
                        .unwrap();
                    {
                        let mut dc = slave.dc_context.borrow_mut();
                        dc.recieved_port_time = [
                            recv_time.receive_time_port0(),
                            recv_time.receive_time_port1(),
                            recv_time.receive_time_port2(),
                            recv_time.receive_time_port3(),
                        ];

                        let first_recieved_port = slave
                            .info()
                            .linked_ports()
                            .iter()
                            .position(|is_active| *is_active)
                            .unwrap();
                        dc.latched_local_sys_time =
                            dc.recieved_port_time[first_recieved_port] as u64;

                        if self.first_dc_slave.is_some() && self.first_dc_slave.unwrap() < *pos {
                            let last_recieved_port = slave
                                .info()
                                .linked_ports()
                                .iter()
                                .enumerate()
                                .rev()
                                .find(|(i, &is_active)| is_active && first_recieved_port < *i)
                                .map(|(i, _)| i);
                            let inner_loop_duration = if let Some(last_recieved_port) =
                                last_recieved_port
                            {
                                let first_recv_time = dc.recieved_port_time[first_recieved_port];
                                let last_recv_time = dc.recieved_port_time[last_recieved_port];
                                if first_recv_time < last_recv_time {
                                    last_recv_time - first_recv_time
                                } else {
                                    ((1_u64 << 32) + last_recv_time as u64 - first_recv_time as u64)
                                        as u32
                                }
                            } else {
                                0
                            };

                            let (parent_pos, parent_port) = dc.parent_port.unwrap();
                            let (parent, _) = self
                                .network
                                .slave(SlaveAddress::SlavePosition(parent_pos))
                                .unwrap();
                            let parent_dc = parent.dc_context.borrow();
                            let parent_next_port = parent
                                .info()
                                .linked_ports()
                                .iter()
                                .enumerate()
                                .position(|(i, is_active)| *is_active && (parent_port as usize) < i)
                                .unwrap();
                            let outer_loop_duration = {
                                let first_recv_time =
                                    parent_dc.recieved_port_time[parent_port as usize];
                                let second_recv_time =
                                    parent_dc.recieved_port_time[parent_next_port];
                                if first_recv_time < second_recv_time {
                                    second_recv_time - first_recv_time
                                } else {
                                    ((1_u64 << 32) + second_recv_time as u64
                                        - first_recv_time as u64)
                                        as u32
                                }
                            };
                            //let additional_delay_ns = match parent.info.ports[parent_port as usize].unwrap() {
                            //    PortPhysics::MII => 40,
                            //    PortPhysics::EBUS => 20,
                            //};
                            let parent_delay = parent_dc.delay;
                            let delay_delta = (outer_loop_duration - inner_loop_duration) / 2; // + phy_delay_ns;
                            if dc.delay == 0 {
                                dc.delay = parent_delay + delay_delta;
                            } else {
                                dc.delay /= 2;
                                dc.delay += (parent_delay + delay_delta) / 2;
                            }
                        }
                    }
                    if (pos + 1) < self.network.num_slaves() {
                        self.state = State::CalculateDelay((*count, pos + 1));
                    } else {
                        let mut max_time = 0;
                        for (slave, _) in self.network.slaves() {
                            let dc = slave.dc_context.borrow();
                            max_time = max_time.max(dc.latched_local_sys_time);
                        }
                        for (slave, _) in self.network.slaves() {
                            let mut dc = slave.dc_context.borrow_mut();
                            let offset = max_time - dc.latched_local_sys_time;

                            if dc.offset == 0 {
                                dc.offset = offset;
                            } else {
                                dc.offset /= 2;
                                dc.offset += offset / 2;
                            }
                        }
                        if *count + 1 < COUNT_MAX {
                            self.state = State::RequestToLatch(*count + 1);
                        } else if let Some(first_dc_slave) = self.first_dc_slave {
                            self.state = State::SetOffset(first_dc_slave);
                        } else {
                            self.state = State::Complete;
                        }
                    }
                }
            }
            State::SetOffset(pos) => {
                if wkc != 1 {
                    self.state = State::Error(TaskError::UnexpectedWkc((1, wkc).into()));
                } else {
                    let next_pos = self
                        .network
                        .slaves()
                        .enumerate()
                        .position(|(i, (s, _))| s.info().support_dc() && *pos < i as u16);
                    if let Some(next_pos) = next_pos {
                        self.state = State::SetOffset(next_pos as u16);
                    } else {
                        self.state = State::SetDelay(self.first_dc_slave.unwrap());
                    }
                }
            }
            State::SetDelay(pos) => {
                if wkc != 1 {
                    self.state = State::Error(TaskError::UnexpectedWkc((1, wkc).into()));
                } else {
                    let next_pos = self
                        .network
                        .slaves()
                        .enumerate()
                        .position(|(i, (s, _))| s.info().support_dc() && *pos < i as u16);
                    if let Some(next_pos) = next_pos {
                        self.state = State::SetDelay(next_pos as u16);
                    } else {
                        self.state = State::CompensateDrift(0);
                    }
                }
            }
            State::CompensateDrift(count) => {
                let num_dc_slave = self.dc_slave_count as u16;
                if wkc != num_dc_slave {
                    self.state = State::Error(TaskError::UnexpectedWkc((num_dc_slave, wkc).into()));
                } else if count + 1 < DRIFT_COUNT_MIN {
                    self.state = State::CompensateDrift(count + 1);
                } else {
                    self.state = State::CheckDelta;
                }
            }
            State::CheckDelta => {
                let num_dc_slave = self.dc_slave_count as u16;
                if wkc != num_dc_slave {
                    self.state = State::Error(TaskError::UnexpectedWkc((num_dc_slave, wkc).into()));
                } else {
                    let delta = DcSystemTimeDelta(data).delta();
                    if delta <= TARGET_DELTA_NS {
                        self.state = State::Complete;
                    } else {
                        let count = if 100 < DRIFT_COUNT_MIN {
                            DRIFT_COUNT_MIN - 100
                        } else {
                            0
                        };
                        self.state = State::CompensateDrift(count);
                    }
                }
            }
        }
    }
}

const fn buffer_size() -> usize {
    let mut size = 0;
    size = const_max(size, DcRecieveTime::SIZE);
    size = const_max(size, DcSystemTime::SIZE);
    size = const_max(size, DcSystemTimeOffset::SIZE);
    size = const_max(size, DcSystemTimeTransmissionDelay::SIZE);
    size = const_max(size, DcSystemTimeDelta::SIZE);
    size
}
