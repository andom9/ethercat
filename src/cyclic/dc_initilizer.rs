use crate::cyclic::Cyclic;
use crate::cyclic::*;
use crate::error::*;
use crate::interface::*;
use crate::network::*;
use crate::register::datalink::*;
use crate::util::*;

const OFFSET_COUNT_MAX: usize = 16;
const DRIFT_COUNT_MAX: usize = 15000;

#[derive(Debug, Clone)]
pub enum DCState {
    Idle,
    Error(DCError),
    Complete,
    RequestToLatch(usize),
    CalculateOffset((usize, u16)),
    CalculateDelay((usize, u16)),
    SetOffset(u16),
    SetDelay(u16),
    ConpensateDrift(usize),
}

#[derive(Debug, Clone)]
pub enum DCError {
    Common(CommonError),
}

impl From<CommonError> for DCError {
    fn from(err: CommonError) -> Self {
        Self::Common(err)
    }
}

#[derive(Debug, Clone)]
pub struct DCInitializer {
    sys_time: EtherCATSystemTime,
    state: DCState,
    command: Command,
    buffer: [u8; buffer_size()],
    first_dc_slave: Option<u16>,
    dc_slave_count: usize,
}

impl DCInitializer {
    pub fn new() -> Self {
        Self {
            sys_time: EtherCATSystemTime(0),
            state: DCState::Idle,
            command: Command::default(),
            buffer: [0; buffer_size()],
            first_dc_slave: None,
            dc_slave_count: 0,
        }
    }

    pub fn start(&mut self, desc: &mut NetworkDescription) {
        self.sys_time = EtherCATSystemTime(0);
        self.command = Command::default();
        self.buffer.fill(0);
        self.first_dc_slave = None;
        self.dc_slave_count = 0;

        // specofy network topology
        let mut last_recv_slave = 0;
        let mut last_recv_port = 0;
        let mut first_slave = None;
        for recv_port in desc.recieved_ports() {
            let slave_pos = recv_port.position;
            let port_number = recv_port.port;
            let slave = desc.slave(slave_pos).unwrap();
            let mut dc = slave.dc_context.borrow_mut();
            if slave.info.support_dc && self.first_dc_slave.is_none() {
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
            let first_slave = desc.slave(first_slave).unwrap();
            let mut dc = first_slave.dc_context.borrow_mut();
            dc.parent_port = None;
        }
        for i in 0..desc.len() {
            let slave = desc.slave(i as u16).unwrap();
            if slave.info.support_dc {
                self.dc_slave_count += 1;
            }
        }
        self.state = DCState::RequestToLatch(0);
    }
}

impl Cyclic for DCInitializer {
    fn next_command(
        &mut self,
        desc: &mut NetworkDescription,
        sys_time: EtherCATSystemTime,
    ) -> Option<(Command, &[u8])> {
        match &self.state {
            DCState::Idle => None,
            DCState::Error(_) => None,
            DCState::Complete => None,
            DCState::RequestToLatch(_) => {
                let command = Command::new(CommandType::BWR, 0, DCRecieveTime::ADDRESS);
                self.buffer.fill(0);
                Some((command, &self.buffer[..DCRecieveTime::SIZE]))
            }
            DCState::CalculateOffset((_, pos)) => {
                let command =
                    Command::new_read(SlaveAddress::SlaveNumber(*pos), DCSystemTime::ADDRESS);
                self.buffer.fill(0);
                //self.time_buf = self.duration_from_2000_0_0.;
                Some((command, &self.buffer[..DCSystemTime::SIZE]))
            }
            DCState::CalculateDelay((_, pos)) => {
                let command =
                    Command::new_read(SlaveAddress::SlaveNumber(*pos), DCRecieveTime::ADDRESS);
                self.buffer.fill(0);
                self.sys_time = sys_time;
                Some((command, &self.buffer[..DCRecieveTime::SIZE]))
            }
            DCState::SetOffset(pos) => {
                let command = Command::new_write(
                    SlaveAddress::SlaveNumber(*pos),
                    DCSystemTimeOffset::ADDRESS,
                );
                self.buffer.fill(0);
                let slave = desc.slave(*pos).unwrap();
                let dc = slave.dc_context.borrow();
                DCSystemTimeOffset(&mut self.buffer).set_system_time_offset(dc.offset);
                Some((command, &self.buffer[..DCSystemTimeOffset::SIZE]))
            }
            DCState::SetDelay(pos) => {
                let command = Command::new_write(
                    SlaveAddress::SlaveNumber(*pos),
                    DCSystemTimeTransmissionDelay::ADDRESS,
                );
                self.buffer.fill(0);
                let slave = desc.slave(*pos).unwrap();
                let dc = slave.dc_context.borrow();
                DCSystemTimeTransmissionDelay(&mut self.buffer)
                    .set_system_time_transmission_delay(dc.delay);
                Some((command, &self.buffer[..DCSystemTimeTransmissionDelay::SIZE]))
            }
            DCState::ConpensateDrift(_) => {
                let command = Command::new(
                    CommandType::ARMW,
                    SlaveAddress::SlaveNumber(self.first_dc_slave.unwrap()).get_ado(),
                    DCSystemTime::ADDRESS,
                );
                self.buffer.fill(0);
                Some((command, &self.buffer[..DCSystemTime::SIZE]))
            }
        }
    }

    fn recieve_and_process(
        &mut self,
        command: Command,
        data: &[u8],
        wkc: u16,
        desc: &mut NetworkDescription,
        _: EtherCATSystemTime,
    ) -> bool {
        if command != self.command {
            self.state = DCState::Error(DCError::Common(CommonError::PacketDropped));
        }

        match &self.state {
            DCState::Idle => {}
            DCState::Error(_) => {}
            DCState::Complete => {}
            DCState::RequestToLatch(count) => {
                if wkc != desc.len() as u16 {
                    self.state = DCState::Error(DCError::Common(CommonError::UnexpectedWKC(wkc)));
                } else {
                    self.state = DCState::CalculateOffset((*count, 0))
                }
            }
            DCState::CalculateOffset((count, pos)) => {
                if wkc != 1 {
                    self.state = DCState::Error(DCError::Common(CommonError::UnexpectedWKC(wkc)));
                } else {
                    let master_time = self.sys_time.0;
                    let slave = desc.slave(*pos).unwrap();
                    let mut dc = slave.dc_context.borrow_mut();
                    let sys_time = DCSystemTime(data);
                    let offset = if master_time > sys_time.local_system_time() {
                        master_time - sys_time.local_system_time()
                    } else {
                        0
                    };
                    if dc.offset != 0 {
                        dc.offset /= 2;
                        dc.offset += offset / 2;
                    } else {
                        dc.offset = offset;
                    }
                    if desc.len() <= (pos + 1) as usize {
                        self.state = DCState::CalculateOffset((*count, pos + 1));
                    } else {
                        self.state = DCState::CalculateDelay((*count, 0));
                    }
                }
            }
            DCState::CalculateDelay((count, pos)) => {
                if wkc != 1 {
                    self.state = DCState::Error(DCError::Common(CommonError::UnexpectedWKC(wkc)));
                } else {
                    let recv_time = DCRecieveTime(data);
                    let slave = desc.slave(*pos).unwrap();
                    let mut dc = slave.dc_context.borrow_mut();
                    dc.recieved_port_time = [
                        recv_time.receive_time_port0(),
                        recv_time.receive_time_port1(),
                        recv_time.receive_time_port2(),
                        recv_time.receive_time_port3(),
                    ];

                    if self.first_dc_slave.is_some() && self.first_dc_slave.unwrap() < *pos {
                        let first_recieved_port = slave
                            .linked_ports
                            .iter()
                            .position(|is_active| *is_active)
                            .unwrap();
                        let second_recieved_port = slave
                            .linked_ports
                            .iter()
                            .enumerate()
                            .position(|(i, is_active)| *is_active && first_recieved_port < i);
                        let inner_loop_duration =
                            if let Some(second_recieved_port) = second_recieved_port {
                                let first_recv_time = dc.recieved_port_time[first_recieved_port];
                                let second_recv_time = dc.recieved_port_time[second_recieved_port];
                                first_recv_time.abs_diff(second_recv_time)
                            } else {
                                0
                            };

                        let (parent_pos, parent_port) = dc.parent_port.unwrap();
                        let parent = desc.slave(parent_pos).unwrap();
                        let parent_dc = parent.dc_context.borrow();
                        let parent_next_port = parent
                            .linked_ports
                            .iter()
                            .enumerate()
                            .position(|(i, is_active)| *is_active && (parent_port as usize) < i)
                            .unwrap();
                        let outer_loop_duration = {
                            let first_recv_time =
                                parent_dc.recieved_port_time[parent_port as usize];
                            let second_recv_time = parent_dc.recieved_port_time[parent_next_port];
                            first_recv_time.abs_diff(second_recv_time)
                        };
                        let tx_delay_ns = match parent.info.ports[parent_port as usize].unwrap() {
                            PortPhysics::MII => 40,
                            PortPhysics::EBUS => 20,
                        };
                        let parent_delay = parent_dc.delay;
                        let delay_delta =
                            (outer_loop_duration - inner_loop_duration) / 2 + tx_delay_ns;
                        if dc.delay != 0 {
                            dc.delay /= 2;
                            dc.delay += (parent_delay + delay_delta) / 2;
                        } else {
                            dc.delay = parent_delay + delay_delta;
                        }
                    }
                    if desc.len() <= (pos + 1) as usize {
                        self.state = DCState::CalculateDelay((*count, pos + 1));
                    } else {
                        if *count < OFFSET_COUNT_MAX {
                            self.state = DCState::RequestToLatch(*count);
                        } else {
                            if let Some(first_dc_slave) = self.first_dc_slave {
                                self.state = DCState::SetOffset(first_dc_slave);
                            } else {
                                self.state = DCState::Complete;
                            }
                        }
                    }
                }
            }
            DCState::SetOffset(pos) => {
                if wkc != 1 {
                    self.state = DCState::Error(DCError::Common(CommonError::UnexpectedWKC(wkc)));
                } else {
                    let next_pos = desc
                        .slaves()
                        .iter()
                        .filter_map(|s| s.as_ref())
                        .enumerate()
                        .position(|(i, s)| s.info.support_dc && *pos < i as u16);
                    if let Some(next_pos) = next_pos {
                        self.state = DCState::SetOffset(next_pos as u16);
                    } else {
                        self.state = DCState::SetDelay(self.first_dc_slave.unwrap());
                    }
                }
            }
            DCState::SetDelay(pos) => {
                if wkc != 1 {
                    self.state = DCState::Error(DCError::Common(CommonError::UnexpectedWKC(wkc)));
                } else {
                    let next_pos = desc
                        .slaves()
                        .iter()
                        .filter_map(|s| s.as_ref())
                        .enumerate()
                        .position(|(i, s)| s.info.support_dc && *pos < i as u16);
                    if let Some(next_pos) = next_pos {
                        self.state = DCState::SetDelay(next_pos as u16);
                    } else {
                        self.state = DCState::ConpensateDrift(0);
                    }
                }
            }
            DCState::ConpensateDrift(count) => {
                if wkc != self.dc_slave_count as u16 {
                    self.state = DCState::Error(DCError::Common(CommonError::UnexpectedWKC(wkc)));
                } else {
                    if count + 1 < DRIFT_COUNT_MAX {
                        self.state = DCState::ConpensateDrift(count + 1);
                    } else {
                        self.state = DCState::Complete;
                    }
                }
            }
        }

        if let DCState::Error(_) = self.state {
            false
        } else {
            true
        }
    }
}

const fn buffer_size() -> usize {
    let mut size = 0;
    size = const_max(size, DCRecieveTime::SIZE);
    size = const_max(size, DCSystemTime::SIZE);
    size = const_max(size, DCSystemTimeOffset::SIZE);
    size = const_max(size, DCSystemTimeTransmissionDelay::SIZE);
    size
}
