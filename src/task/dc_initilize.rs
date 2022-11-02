use super::TaskError;
use super::{CyclicTask, EtherCatSystemTime};
use crate::frame::CommandType;
use crate::interface::*;
use crate::register::{
    DcRecieveTime, DcSystemTime, DcSystemTimeOffset, DcSystemTimeTransmissionDelay,
};
use crate::slave::Network;
use crate::util::const_max;

//TODO:Dcスレーブについて、0x092C(システムタイムの差)を見る。

const OFFSET_COUNT_MAX: usize = 16;
const DRIFT_COUNT_MAX: usize = 100;

#[derive(Debug, Clone, PartialEq)]
enum State {
    Idle,
    Error(TaskError<()>),
    RequestToLatch(usize),
    CalculateOffset((usize, u16)),
    CalculateDelay((usize, u16)),
    SetOffset(u16),
    SetDelay(u16),
    CompensateDrift(usize),
    Complete,
}

#[derive(Debug)]
pub struct DcInitTask<'a, 'b, 'c, 'd> {
    //sys_time: EtherCatSystemTime,
    state: State,
    command: Command,
    //buffer: [u8; buffer_size()],
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
            //sys_time: EtherCatSystemTime(0),
            state: State::Idle,
            command: Command::default(),
            //buffer: [0; buffer_size()],
            first_dc_slave: None,
            dc_slave_count: 0,
            network,
        }
    }

    pub fn start(&mut self) {
        //self.sys_time = EtherCatSystemTime(0);
        self.command = Command::default();
        //self.buffer.fill(0);
        self.first_dc_slave = None;
        self.dc_slave_count = 0;

        // MEMO:ネットワークイニシャライザーの方にもっていく？
        // specify network topology
        let mut last_recv_slave = 0;
        let mut last_recv_port = 0;
        let mut first_slave = None;
        for recv_port in self.network.recieved_ports() {
            let slave_pos = recv_port.slave_position;
            dbg!(slave_pos);
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
            }

            if dc.parent_port.is_none() {
                dc.parent_port = Some((last_recv_slave, last_recv_port));
            } else {
            }
            last_recv_slave = slave_pos;
            last_recv_port = port_number;
        }
        if let Some(first_slave) = first_slave {
            let (first_slave, _) = self
                .network
                .slave(SlaveAddress::SlavePosition(first_slave))
                .unwrap();
            let mut dc = first_slave.dc_context.borrow_mut();
            dc.parent_port = None;
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
        dbg!(self.dc_slave_count);
        self.state = State::RequestToLatch(0);
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
    fn is_finished(&self) -> bool {
        match self.state {
            State::Complete | State::Error(_) => true,
            _ => false,
        }
    }

    fn next_pdu(&mut self, buf: &mut [u8]) -> Option<(Command, usize)> {
        dbg!(&self.state);
        match &self.state {
            State::Idle => None,
            State::Error(_) => None,
            State::Complete => None,
            State::RequestToLatch(_) => {
                self.command = Command::new(CommandType::BWR, 0, DcRecieveTime::ADDRESS);
                buf[..DcRecieveTime::SIZE].fill(0);
                Some((self.command, DcRecieveTime::SIZE))
            }
            State::CalculateOffset((_, pos)) => {
                self.command = Command::new_read(
                    SlaveAddress::SlavePosition(*pos).into(),
                    DcSystemTime::ADDRESS,
                );
                buf[..DcSystemTime::SIZE].fill(0);
                Some((self.command, DcSystemTime::SIZE))
            }
            State::CalculateDelay((_, pos)) => {
                self.command = Command::new_read(
                    SlaveAddress::SlavePosition(*pos).into(),
                    DcRecieveTime::ADDRESS,
                );
                buf[..DcRecieveTime::SIZE].fill(0);
                //self.sys_time = sys_time;
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
                    SlaveAddress::SlavePosition(self.first_dc_slave.unwrap()).get_ado(),
                    DcSystemTime::ADDRESS,
                );
                buf[..DcSystemTime::SIZE].fill(0);
                Some((self.command, DcSystemTime::SIZE))
            }
        }
    }

    fn recieve_and_process(&mut self, recv_data: &Pdu, sys_time: EtherCatSystemTime) {
        dbg!(&self.state);
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
            State::RequestToLatch(count) => {
                let num_slaves = self.network.num_slaves() as u16;
                if wkc != num_slaves {
                    self.state = State::Error(TaskError::UnexpectedWkc((num_slaves, wkc).into()));
                } else {
                    self.state = State::CalculateOffset((*count, 0))
                }
            }
            State::CalculateOffset((count, pos)) => {
                if wkc != 1 {
                    self.state = State::Error(TaskError::UnexpectedWkc((1, wkc).into()));
                } else {
                    let master_time = sys_time.0;
                    let (slave, _) = self
                        .network
                        .slave(SlaveAddress::SlavePosition(*pos))
                        .unwrap();
                    let mut dc = slave.dc_context.borrow_mut();
                    let slave_sys_time = DcSystemTime(data);
                    let offset = if master_time > slave_sys_time.local_system_time() {
                        master_time - slave_sys_time.local_system_time()
                    } else {
                        0
                    };
                    if dc.offset != 0 {
                        dc.offset /= 2;
                        dc.offset += offset / 2;
                    } else {
                        dc.offset = offset;
                    }
                    if self.network.num_slaves() < (pos + 1) {
                        self.state = State::CalculateOffset((*count, pos + 1));
                    } else {
                        self.state = State::CalculateDelay((*count, 0));
                    }
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
                    let mut dc = slave.dc_context.borrow_mut();
                    dc.recieved_port_time = [
                        recv_time.receive_time_port0(),
                        recv_time.receive_time_port1(),
                        recv_time.receive_time_port2(),
                        recv_time.receive_time_port3(),
                    ];

                    if self.first_dc_slave.is_some() && self.first_dc_slave.unwrap() < *pos {
                        let first_recieved_port = slave
                            .info()
                            .linked_ports()
                            .iter()
                            .position(|is_active| *is_active)
                            .unwrap();
                        let last_recieved_port = slave
                            .info()
                            .linked_ports()
                            .iter()
                            .enumerate()
                            .rev()
                            .find(|(i, &is_active)| is_active && first_recieved_port < *i)
                            .map(|(i, _)| i);
                        let inner_loop_duration =
                            if let Some(last_recieved_port) = last_recieved_port {
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
                            let second_recv_time = parent_dc.recieved_port_time[parent_next_port];
                            if first_recv_time < second_recv_time {
                                second_recv_time - first_recv_time
                            } else {
                                ((1_u64 << 32) + second_recv_time as u64 - first_recv_time as u64)
                                    as u32
                            }
                        };
                        //let phy_delay_ns = match parent.info.ports[parent_port as usize].unwrap() {
                        //    PortPhysics::MII => 40,
                        //    PortPhysics::EBUS => 20,
                        //};
                        let parent_delay = parent_dc.delay;
                        let delay_delta = (outer_loop_duration - inner_loop_duration) / 2; // + phy_delay_ns;
                        if dc.delay != 0 {
                            dc.delay /= 2;
                            dc.delay += (parent_delay + delay_delta) / 2;
                        } else {
                            dc.delay = parent_delay + delay_delta;
                        }
                    }
                    if self.network.num_slaves() < (pos + 1) {
                        self.state = State::CalculateDelay((*count, pos + 1));
                    } else if *count < OFFSET_COUNT_MAX {
                        self.state = State::RequestToLatch(*count + 1);
                    } else if let Some(first_dc_slave) = self.first_dc_slave {
                        self.state = State::SetOffset(first_dc_slave);
                    } else {
                        self.state = State::Complete;
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
                } else if count + 1 < DRIFT_COUNT_MAX {
                    self.state = State::CompensateDrift(count + 1);
                } else {
                    self.state = State::Complete;
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
    size
}
