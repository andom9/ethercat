use super::super::{CyclicTask, EtherCatSystemTime};
use crate::frame::CommandType;
use crate::interface::*;
use crate::register::DcSystemTime;

#[derive(Debug)]
pub struct DcSyncTask {
    sys_time_offset: i64,
    last_dc_time: EtherCatSystemTime,
    first_dc_slave: u16,
    dc_slave_count: u16,
    pub invalid_wkc_count: usize,
    last_wkc: u16,
}

impl DcSyncTask {
    pub const fn required_buffer_size() -> usize {
        DcSystemTime::SIZE
    }

    pub fn new(first_dc_slave_pos: u16, num_dc_slaves: u16) -> Self {
        Self {
            sys_time_offset: 0,
            last_dc_time: EtherCatSystemTime(0),
            first_dc_slave: first_dc_slave_pos,
            dc_slave_count: num_dc_slaves,
            invalid_wkc_count: 0,
            last_wkc: 0,
        }
    }

    pub fn last_wkc(&self) -> u16 {
        self.last_wkc
    }

    pub fn expected_wkc(&self) -> u16 {
        self.dc_slave_count
    }

    /// offset = DC system time - local system time
    pub fn systemtime_offset_ns(&self) -> i64 {
        self.sys_time_offset
    }

    pub fn last_dc_time(&self) -> EtherCatSystemTime {
        self.last_dc_time
    }
}

impl CyclicTask for DcSyncTask {
    fn is_busy(&self) -> bool {
        false
    }

    fn next_pdu(&mut self, buf: &mut [u8]) -> Option<(Command, usize)> {
        let command = Command::new(
            CommandType::ARMW,
            SlaveAddress::SlavePosition(self.first_dc_slave).get_adp(),
            DcSystemTime::ADDRESS,
        );
        buf[..DcSystemTime::SIZE].fill(0);
        Some((command, DcSystemTime::SIZE))
    }

    fn recieve_and_process(&mut self, recv_data: &Pdu, systime: EtherCatSystemTime) {
        let data = {
            let Pdu { data, wkc, .. } = recv_data;
            let wkc = *wkc;
            if wkc != self.dc_slave_count {
                self.invalid_wkc_count = self.invalid_wkc_count.saturating_add(1);
            }
            data
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
