use crate::cyclic_task::{Command, CommandData, EtherCatSystemTime};
use crate::cyclic_task::{Cyclic, TargetSlave};
use crate::register::RxErrorCounter;

#[derive(Debug)]
pub struct RxErrorChecker {
    command: Command,
    target: TargetSlave,
    rx_error_count: RxErrorCounter<[u8; RxErrorCounter::SIZE]>,
    pub invalid_wkc_count: usize,
    last_wkc: u16,
}

impl RxErrorChecker {
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

    pub fn rx_error_count(&self) -> &RxErrorCounter<[u8; RxErrorCounter::SIZE]> {
        &self.rx_error_count
    }
}

impl Cyclic for RxErrorChecker {
    fn is_finished(&self) -> bool {
        true
    }

    fn next_command(&mut self, buf: &mut [u8]) -> Option<(Command, usize)> {
        buf[..RxErrorCounter::SIZE].fill(0);
        self.command = Command::new_read(self.target, RxErrorCounter::ADDRESS);
        Some((self.command, RxErrorCounter::SIZE))
    }

    fn recieve_and_process(&mut self, recv_data: &CommandData, _systime: EtherCatSystemTime) {
        let data = {
            let CommandData { wkc, data, .. } = recv_data;
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
            .for_each(|(b, d)| *b = d.clone());
    }
}
