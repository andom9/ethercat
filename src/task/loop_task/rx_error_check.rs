use super::super::{CyclicTask, EtherCatSystemTime};
use crate::interface::*;
use crate::register::RxErrorCounter;

#[derive(Debug)]
pub struct RxErrorReadTask {
    command: Command,
    target: TargetSlave,
    rx_error_count: RxErrorCounter<[u8; RxErrorCounter::SIZE]>,
    pub invalid_wkc_count: usize,
    last_wkc: u16,
}

impl RxErrorReadTask {
    pub const fn required_buffer_size() -> usize {
        RxErrorCounter::SIZE
    }

    pub fn new() -> Self {
        Self {
            command: Command::default(),
            target: TargetSlave::default(),
            rx_error_count: RxErrorCounter::new(),
            invalid_wkc_count: 0,
            last_wkc: 0,
        }
    }

    pub fn set_target(&mut self, target_slave: TargetSlave) {
        self.target = target_slave;
    }

    pub fn last_wkc(&self) -> u16 {
        self.last_wkc
    }

    pub fn expected_wkc(&self) -> u16 {
        match self.target {
            TargetSlave::All(num) => num,
            TargetSlave::Single(_) => 1,
        }
    }

    pub fn rx_error_count(&self) -> &RxErrorCounter<[u8; RxErrorCounter::SIZE]> {
        &self.rx_error_count
    }
}

impl CyclicTask for RxErrorReadTask {
    fn is_finished(&self) -> bool {
        true
    }

    fn next_pdu(&mut self, buf: &mut [u8]) -> Option<(Command, usize)> {
        buf[..RxErrorCounter::SIZE].fill(0);
        self.command = Command::new_read(self.target, RxErrorCounter::ADDRESS);
        Some((self.command, RxErrorCounter::SIZE))
    }

    fn recieve_and_process(&mut self, recv_data: &Pdu, _systime: EtherCatSystemTime) {
        let data = {
            let Pdu { wkc, data, .. } = recv_data;
            let wkc = *wkc;
            if wkc != self.target.num_targets() {
                self.invalid_wkc_count = self.invalid_wkc_count.saturating_add(1);
            }
            data
        };
        self.rx_error_count
            .0
            .iter_mut()
            .zip(data[0..RxErrorCounter::SIZE].iter())
            .for_each(|(b, d)| *b = *d);
    }
}
