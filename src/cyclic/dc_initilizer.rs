use crate::cyclic::CyclicProcess;
use crate::cyclic::{
    Command, CommandType, EcError, EtherCatSystemTime, NetworkDescription, ReceivedData,
    SlaveAddress,
};
use crate::register::datalink::{
    DcRecieveTime, DcSystemTime, DcSystemTimeOffset, DcSystemTimeTransmissionDelay,
};
use crate::util::const_max;

//TODO:Dcスレーブについて、0x092C(システムタイムの差)を見る。

const OFFSET_COUNT_MAX: usize = 16;
const DRIFT_COUNT_MAX: usize = 15000;

#[derive(Debug, Clone)]
enum State {
    Idle,
    Error(EcError<()>),
    Complete,
    RequestToLatch(usize),
    CalculateOffset((usize, u16)),
    CalculateDelay((usize, u16)),
    SetOffset(u16),
    SetDelay(u16),
    CompensateDrift(usize),
}

//#[derive(Debug, Clone)]
//pub enum Error {
//    Common(EcError),
//}

//impl From<Error> for EcError<Error> {
//    fn from(err: Error) -> Self {
//        Self::UnitSpecific(err)
//    }
//}

#[derive(Debug, Clone)]
pub struct DcInitializer {
    sys_time: EtherCatSystemTime,
    state: State,
    command: Command,
    buffer: [u8; buffer_size()],
    first_dc_slave: Option<u16>,
    dc_slave_count: usize,
}

impl DcInitializer {
    pub fn new() -> Self {
        Self {
            sys_time: EtherCatSystemTime(0),
            state: State::Idle,
            command: Command::default(),
            buffer: [0; buffer_size()],
            first_dc_slave: None,
            dc_slave_count: 0,
        }
    }

    pub fn start(&mut self, desc: &mut NetworkDescription) {
        self.sys_time = EtherCatSystemTime(0);
        self.command = Command::default();
        self.buffer.fill(0);
        self.first_dc_slave = None;
        self.dc_slave_count = 0;

        // MEMO:ネットワークイニシャライザーの方にもっていく？
        // specify network topology
        let mut last_recv_slave = 0;
        let mut last_recv_port = 0;
        let mut first_slave = None;
        for recv_port in desc.recieved_ports() {
            let slave_pos = recv_port.position;
            let port_number = recv_port.port;
            let slave = desc.slave(SlaveAddress::SlavePosition(slave_pos)).unwrap();
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
            let first_slave = desc
                .slave(SlaveAddress::SlavePosition(first_slave))
                .unwrap();
            let mut dc = first_slave.dc_context.borrow_mut();
            dc.parent_port = None;
        }
        for i in 0..desc.len() {
            let slave = desc.slave(SlaveAddress::SlavePosition(i as u16)).unwrap();
            if slave.info.support_dc {
                self.dc_slave_count += 1;
            }
        }
        self.state = State::RequestToLatch(0);
    }

    //TODO:Waitがない
}

impl CyclicProcess for DcInitializer {
    fn next_command(
        &mut self,
        desc: &mut NetworkDescription,
        sys_time: EtherCatSystemTime,
    ) -> Option<(Command, &[u8])> {
        match &self.state {
            State::Idle => None,
            State::Error(_) => None,
            State::Complete => None,
            State::RequestToLatch(_) => {
                let command = Command::new(CommandType::BWR, 0, DcRecieveTime::ADDRESS);
                self.buffer.fill(0);
                Some((command, &self.buffer[..DcRecieveTime::SIZE]))
            }
            State::CalculateOffset((_, pos)) => {
                let command =
                    Command::new_read(SlaveAddress::SlavePosition(*pos), DcSystemTime::ADDRESS);
                self.buffer.fill(0);
                //self.time_buf = self.duration_from_2000_0_0.;
                Some((command, &self.buffer[..DcSystemTime::SIZE]))
            }
            State::CalculateDelay((_, pos)) => {
                let command =
                    Command::new_read(SlaveAddress::SlavePosition(*pos), DcRecieveTime::ADDRESS);
                self.buffer.fill(0);
                self.sys_time = sys_time;
                Some((command, &self.buffer[..DcRecieveTime::SIZE]))
            }
            State::SetOffset(pos) => {
                let command = Command::new_write(
                    SlaveAddress::SlavePosition(*pos),
                    DcSystemTimeOffset::ADDRESS,
                );
                self.buffer.fill(0);
                let slave = desc.slave(SlaveAddress::SlavePosition(*pos)).unwrap();
                let dc = slave.dc_context.borrow();
                DcSystemTimeOffset(&mut self.buffer).set_system_time_offset(dc.offset);
                Some((command, &self.buffer[..DcSystemTimeOffset::SIZE]))
            }
            State::SetDelay(pos) => {
                let command = Command::new_write(
                    SlaveAddress::SlavePosition(*pos),
                    DcSystemTimeTransmissionDelay::ADDRESS,
                );
                self.buffer.fill(0);
                let slave = desc.slave(SlaveAddress::SlavePosition(*pos)).unwrap();
                let dc = slave.dc_context.borrow();
                DcSystemTimeTransmissionDelay(&mut self.buffer)
                    .set_system_time_transmission_delay(dc.delay);
                Some((command, &self.buffer[..DcSystemTimeTransmissionDelay::SIZE]))
            }
            State::CompensateDrift(_) => {
                let command = Command::new(
                    CommandType::ARMW,
                    SlaveAddress::SlavePosition(self.first_dc_slave.unwrap()).get_ado(),
                    DcSystemTime::ADDRESS,
                );
                self.buffer.fill(0);
                Some((command, &self.buffer[..DcSystemTime::SIZE]))
            }
        }
    }

    fn recieve_and_process(
        &mut self,
        recv_data: Option<ReceivedData>,
        desc: &mut NetworkDescription,
        _: EtherCatSystemTime,
    ) {
        let (data, wkc) = if let Some(recv_data) = recv_data {
            let ReceivedData { command, data, wkc } = recv_data;
            if !(command.c_type == self.command.c_type && command.ado == self.command.ado) {
                self.state = State::Error(EcError::UnexpectedCommand);
            }
            (data, wkc)
        } else {
            self.state = State::Error(EcError::LostCommand);
            return;
        };

        match &self.state {
            State::Idle => {}
            State::Error(_) => {}
            State::Complete => {}
            State::RequestToLatch(count) => {
                if wkc != desc.len() as u16 {
                    self.state = State::Error(EcError::UnexpectedWKC(wkc));
                } else {
                    self.state = State::CalculateOffset((*count, 0))
                }
            }
            State::CalculateOffset((count, pos)) => {
                if wkc != 1 {
                    self.state = State::Error(EcError::UnexpectedWKC(wkc));
                } else {
                    let master_time = self.sys_time.0;
                    let slave = desc.slave(SlaveAddress::SlavePosition(*pos)).unwrap();
                    let mut dc = slave.dc_context.borrow_mut();
                    let sys_time = DcSystemTime(data);
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
                        self.state = State::CalculateOffset((*count, pos + 1));
                    } else {
                        self.state = State::CalculateDelay((*count, 0));
                    }
                }
            }
            State::CalculateDelay((count, pos)) => {
                if wkc != 1 {
                    self.state = State::Error(EcError::UnexpectedWKC(wkc));
                } else {
                    let recv_time = DcRecieveTime(data);
                    let slave = desc.slave(SlaveAddress::SlavePosition(*pos)).unwrap();
                    let mut dc = slave.dc_context.borrow_mut();
                    dc.recieved_port_time = [
                        recv_time.receive_time_port0(),
                        recv_time.receive_time_port1(),
                        recv_time.receive_time_port2(),
                        recv_time.receive_time_port3(),
                    ];

                    if self.first_dc_slave.is_some() && self.first_dc_slave.unwrap() < *pos {
                        let first_recieved_port = slave
                            .status
                            .linked_ports
                            .iter()
                            .position(|is_active| *is_active)
                            .unwrap();
                        let last_recieved_port = slave
                            .status
                            .linked_ports
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
                        let parent = desc.slave(SlaveAddress::SlavePosition(parent_pos)).unwrap();
                        let parent_dc = parent.dc_context.borrow();
                        let parent_next_port = parent
                            .status
                            .linked_ports
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
                    if desc.len() <= (pos + 1) as usize {
                        self.state = State::CalculateDelay((*count, pos + 1));
                    } else if *count < OFFSET_COUNT_MAX {
                        self.state = State::RequestToLatch(*count);
                    } else if let Some(first_dc_slave) = self.first_dc_slave {
                        self.state = State::SetOffset(first_dc_slave);
                    } else {
                        self.state = State::Complete;
                    }
                }
            }
            State::SetOffset(pos) => {
                if wkc != 1 {
                    self.state = State::Error(EcError::UnexpectedWKC(wkc));
                } else {
                    let next_pos = desc
                        .slaves()
                        //.iter()
                        //.filter_map(|s| s.as_ref())
                        .enumerate()
                        .position(|(i, s)| s.info.support_dc && *pos < i as u16);
                    if let Some(next_pos) = next_pos {
                        self.state = State::SetOffset(next_pos as u16);
                    } else {
                        self.state = State::SetDelay(self.first_dc_slave.unwrap());
                    }
                }
            }
            State::SetDelay(pos) => {
                if wkc != 1 {
                    self.state = State::Error(EcError::UnexpectedWKC(wkc));
                } else {
                    let next_pos = desc
                        .slaves()
                        //.iter()
                        //.filter_map(|s| s.as_ref())
                        .enumerate()
                        .position(|(i, s)| s.info.support_dc && *pos < i as u16);
                    if let Some(next_pos) = next_pos {
                        self.state = State::SetDelay(next_pos as u16);
                    } else {
                        self.state = State::CompensateDrift(0);
                    }
                }
            }
            State::CompensateDrift(count) => {
                if wkc != self.dc_slave_count as u16 {
                    self.state = State::Error(EcError::UnexpectedWKC(wkc));
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
