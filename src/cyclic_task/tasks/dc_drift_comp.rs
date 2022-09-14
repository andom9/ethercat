use crate::cyclic_task::Cyclic;
use crate::cyclic_task::{Command, CommandData, EtherCatSystemTime, SlaveAddress};
use crate::frame::CommandType;
use crate::register::DcSystemTime;

#[derive(Debug)]
pub struct DcDriftCompensator {
    sys_time_offset: i64,
    last_dc_time: EtherCatSystemTime,
    command: Command,
    first_dc_slave: u16,
    dc_slave_count: u16,
    pub invalid_wkc_count: usize,
    pub lost_pdu_count: usize,
    last_wkc: u16,
}

impl DcDriftCompensator {
    pub const fn required_buffer_size() -> usize {
        DcSystemTime::SIZE
    }

    pub fn new(first_dc_slave_pos: u16, num_dc_slaves: u16) -> Self {
        Self {
            sys_time_offset: 0,
            last_dc_time: EtherCatSystemTime(0),
            command: Command::default(),
            first_dc_slave: first_dc_slave_pos,
            dc_slave_count: num_dc_slaves,
            invalid_wkc_count: 0,
            lost_pdu_count: 0,
            last_wkc: 0,
        }
    }

    pub fn last_wkc(&self) -> u16 {
        self.last_wkc
    }

    /// offset = DC system time - local system time
    pub fn systemtime_offset_ns(&self) -> i64 {
        self.sys_time_offset
    }

    pub fn last_dc_time(&self) -> EtherCatSystemTime {
        self.last_dc_time
    }
}

impl Cyclic for DcDriftCompensator {
    fn is_finished(&self) -> bool {
        true
    }

    fn next_command(&mut self, buf: &mut [u8]) -> Option<(Command, usize)> {
        let command = Command::new(
            CommandType::ARMW,
            SlaveAddress::SlavePosition(self.first_dc_slave).get_ado(),
            DcSystemTime::ADDRESS,
        );
        buf[..DcSystemTime::SIZE].fill(0);
        Some((command, DcSystemTime::SIZE))
    }

    fn recieve_and_process(
        &mut self,
        recv_data: Option<&CommandData>,
        systime: EtherCatSystemTime,
    ) {
        let data = if let Some(recv_data) = recv_data {
            let CommandData { command, data, wkc } = recv_data;
            let wkc = *wkc;
            if !(command.c_type == self.command.c_type && command.ado == self.command.ado) {
                self.lost_pdu_count = self.lost_pdu_count.saturating_add(1);
            } else if wkc != self.dc_slave_count {
                self.invalid_wkc_count = self.invalid_wkc_count.saturating_add(1);
            }
            data
        } else {
            self.lost_pdu_count = self.lost_pdu_count.saturating_add(1);
            return;
        };

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
