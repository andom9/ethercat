use crate::cyclic_task::Cyclic;
use crate::cyclic_task::{Command, CommandData, EtherCatSystemTime, SlaveAddress};
use crate::frame::CommandType;
use crate::register::{
    DcRecieveTime, DcSystemTime, DcSystemTimeOffset, DcSystemTimeTransmissionDelay,
};
use crate::slave_network::NetworkDescription;
use crate::util::const_max;
use crate::EcError;

#[derive(Debug, Clone, PartialEq)]
enum State {
    Idle,
    Error(EcError<()>),
    CompensateDrift,
}

#[derive(Debug)]
pub struct DcDriftCompensator {
    sys_time_offset: i64,
    last_dc_time: EtherCatSystemTime,
    state: State,
    command: Command,
    buffer: [u8; buffer_size()],
    first_dc_slave: Option<u16>,
    dc_slave_count: usize,
}

impl DcDriftCompensator {
    pub const fn required_buffer_size() -> usize {
        buffer_size()
    }

    pub fn new() -> Self {
        Self {
            sys_time_offset: 0,
            last_dc_time: EtherCatSystemTime(0),
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
    pub fn systemtime_offset_ns(&self) -> i64 {
        self.sys_time_offset
    }

    pub fn last_dc_time(&self) -> EtherCatSystemTime {
        self.last_dc_time
    }

    pub fn stop(&mut self) {
        self.state = State::Idle;
    }
}

impl Cyclic for DcDriftCompensator {
    fn is_finished(&self) -> bool {
        todo!()
    }

    fn next_command(&mut self, buf: &mut [u8]) -> Option<(Command, usize)> {
        match &self.state {
            State::Idle => None,
            State::Error(_) => None,
            State::CompensateDrift => {
                let command = Command::new(
                    CommandType::ARMW,
                    SlaveAddress::SlavePosition(self.first_dc_slave.unwrap()).get_ado(),
                    DcSystemTime::ADDRESS,
                );
                buf[..DcSystemTime::SIZE].fill(0);
                Some((command, DcSystemTime::SIZE))
            }
        }
    }

    fn recieve_and_process(
        &mut self,
        recv_data: Option<&CommandData>,
        systime: EtherCatSystemTime,
    ) {
        let (data, wkc) = if let Some(recv_data) = recv_data {
            let CommandData { command, data, wkc } = recv_data;
            let wkc = *wkc;
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
                    self.last_dc_time = EtherCatSystemTime(slave_systime);
                    if systime.0 < slave_systime {
                        let offset_abs = (slave_systime - systime.0).min(i64::MAX as u64);
                        self.sys_time_offset = offset_abs as i64;
                    } else {
                        let offset_abs = (systime.0 - slave_systime).min(i64::MAX as u64);
                        self.sys_time_offset = offset_abs as i64;
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
