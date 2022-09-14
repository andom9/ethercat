use crate::cyclic_task::Cyclic;
use crate::cyclic_task::{Command, CommandData, EtherCatSystemTime};

#[derive(Debug)]
pub struct LogicalProcessData {
    command: Command,
    start_logical_address: u32,
    expected_wkc: u16,
    image_size: usize,
    pub invalid_wkc_count: usize,
    pub lost_pdu_count: usize,
    last_wkc: u16,
}

impl LogicalProcessData {
    pub fn new(start_logical_address: u32, expected_wkc: u16, image_size: usize) -> Self {
        Self {
            command: Command::new_logical_read_write(start_logical_address),
            start_logical_address,
            expected_wkc,
            image_size,
            invalid_wkc_count: 0,
            lost_pdu_count: 0,
            last_wkc: 0,
        }
    }

    pub fn last_wkc(&self) -> u16 {
        self.last_wkc
    }

    pub fn start_logical_address(&self) -> u32 {
        self.start_logical_address
    }

    pub fn image_size(&self) -> usize {
        self.image_size
    }
}

impl Cyclic for LogicalProcessData {
    fn is_finished(&self) -> bool {
        true
    }

    fn next_command(&mut self, _buf: &mut [u8]) -> Option<(Command, usize)> {
        Some((self.command, self.image_size))
    }

    fn recieve_and_process(
        &mut self,
        recv_data: Option<&CommandData>,
        _systime: EtherCatSystemTime,
    ) {
        if let Some(recv_data) = recv_data {
            let CommandData { command, wkc, .. } = recv_data;
            let wkc = *wkc;
            if !(command.c_type == self.command.c_type) {
                self.lost_pdu_count = self.lost_pdu_count.saturating_add(1);
            } else if wkc != self.expected_wkc {
                self.invalid_wkc_count = self.invalid_wkc_count.saturating_add(1);
            }
        } else {
            self.lost_pdu_count = self.lost_pdu_count.saturating_add(1);
            return;
        };
    }
}
