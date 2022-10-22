use super::super::{CyclicTask, EtherCatSystemTime};
use crate::interface::*;

#[derive(Debug)]
pub struct ProcessTask {
    command: Command,
    start_logical_address: u32,
    expected_wkc: u16,
    image_size: usize,
    pub invalid_wkc_count: usize,
    last_wkc: u16,
}

impl ProcessTask {
    pub fn new(start_logical_address: u32, expected_wkc: u16, image_size: usize) -> Self {
        Self {
            command: Command::new_logical_read_write(start_logical_address),
            start_logical_address,
            expected_wkc,
            image_size,
            invalid_wkc_count: 0,
            last_wkc: 0,
        }
    }

    pub fn last_wkc(&self) -> u16 {
        self.last_wkc
    }

    pub fn expected_wkc(&self) -> u16 {
        self.expected_wkc
    }

    pub fn set_expected_wkc(&mut self, expected_wkc: u16) {
        self.expected_wkc = expected_wkc;
    }

    pub fn start_logical_address(&self) -> u32 {
        self.start_logical_address
    }

    pub fn set_start_logical_address(&mut self, start_logical_address: u32) {
        self.start_logical_address = start_logical_address;
    }

    pub fn image_size(&self) -> usize {
        self.image_size
    }

    pub fn set_image_size(&mut self, image_size: usize) {
        self.image_size = image_size;
    }
}

impl CyclicTask for ProcessTask {
    fn is_finished(&self) -> bool {
        true
    }

    fn next_pdu(&mut self, _buf: &mut [u8]) -> Option<(Command, usize)> {
        if self.expected_wkc == 0 {
            None
        } else {
            Some((self.command, self.image_size))
        }
    }

    fn recieve_and_process(&mut self, recv_data: &Pdu, _systime: EtherCatSystemTime) {
        let Pdu { wkc, .. } = recv_data;
        let wkc = *wkc;
        if wkc != self.expected_wkc {
            self.invalid_wkc_count = self.invalid_wkc_count.saturating_add(1);
        }
    }
}
