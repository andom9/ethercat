use crate::cyclic_task::CyclicProcess;
use crate::cyclic_task::{
    Command, CommandType, EcError, EtherCatSystemTime, ReceivedData,
    SlaveAddress,
};
use crate::register::{
    DcRecieveTime, DcSystemTime, DcSystemTimeOffset, DcSystemTimeTransmissionDelay,
};
use crate::slave_network::NetworkDescription;
use crate::util::const_max;

#[derive(Debug, Clone)]
enum State {
    Idle,
    Error(EcError<()>),
    CompensateDrift,
}

#[derive(Debug)]
pub struct DcDriftCompensator {
    sys_time_offset: i32,
    state: State,
    command: Command,
    buffer: [u8; buffer_size()],
    first_dc_slave: Option<u16>,
    dc_slave_count: usize,
}

impl DcDriftCompensator {
    pub fn new() -> Self {
        Self {
            sys_time_offset: 0,
            state: State::Idle,
            command: Command::default(),
            buffer: [0; buffer_size()],
            first_dc_slave: None,
            dc_slave_count: 0,
        }
    }

    pub fn start(&mut self, network: &NetworkDescription) {
        self.sys_time_offset = 0;
        self.command = Command::default();
        self.buffer.fill(0);
        self.first_dc_slave = None;
        self.dc_slave_count = 0;

        for (i, slave) in network.slaves().into_iter().enumerate() {
            if slave.info.support_dc && self.first_dc_slave.is_none() {
                self.first_dc_slave = Some(i as u16);
            }
            if slave.info.support_dc {
                self.dc_slave_count += 1;
            }
        }
        if self.first_dc_slave.is_some() {
            self.state = State::CompensateDrift;
        } else {
            self.state = State::Idle;
        }
    }

    /// offset = DC system time - local system time
    pub fn systemtime_offset_ns(&self) -> i32 {
        self.sys_time_offset
    }

    pub fn stop(&mut self) {
        self.state = State::Idle;
    }
}

impl CyclicProcess for DcDriftCompensator {
    fn next_command(&mut self, _: EtherCatSystemTime) -> Option<(Command, &[u8])> {
        match &self.state {
            State::Idle => None,
            State::Error(_) => None,
            State::CompensateDrift => {
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
        systime: EtherCatSystemTime,
    ) {
        let (data, wkc) = if let Some(recv_data) = recv_data {
            let ReceivedData { command, data, wkc } = recv_data;
            if !(command.c_type == self.command.c_type && command.ado == self.command.ado) {
                self.state = State::Error(EcError::UnexpectedCommand);
            }
            (data, wkc)
        } else {
            self.state = State::Error(EcError::LostPacket);
            return;
        };

        match &self.state {
            State::Idle => {}
            State::Error(_) => {}
            State::CompensateDrift => {
                if wkc != self.dc_slave_count as u16 {
                    self.state = State::Error(EcError::UnexpectedWkc(wkc));
                } else {
                    let slave_systime = DcSystemTime(data).local_system_time();
                    if systime.0 < slave_systime {
                        let offset_abs = (slave_systime - systime.0).min(i32::MAX as u64);
                        self.sys_time_offset = offset_abs as i32;
                    } else {
                        let offset_abs = (systime.0 - slave_systime).min(i32::MAX as u64);
                        self.sys_time_offset = offset_abs as i32;
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
    size
}
