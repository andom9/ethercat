use super::super::interface::*;
use super::super::CommandData;
use super::super::EtherCatSystemTime;
use crate::cyclic_task::Cyclic;
use crate::register::AlStatus;
use crate::register::AlStatusCode;
use crate::slave_network::AlState;
use crate::util::const_max;

#[derive(Debug)]
pub struct AlStateReader {
    target: TargetSlave,
    command: Command,
    last_al_state: Option<AlState>,
    last_al_status_code: Option<AlStatusCode>,
    pub invalid_wkc_count: usize,
    last_wkc: u16,
}

impl AlStateReader {
    pub const fn required_buffer_size() -> usize {
        buffer_size()
    }

    pub fn new() -> Self {
        Self {
            target: TargetSlave::default(),
            command: Command::default(),
            last_al_state: None,
            last_al_status_code: None,
            invalid_wkc_count: 0,
            last_wkc: 0,
        }
    }

    pub fn set_target(&mut self, target_slave: TargetSlave) {
        self.target = target_slave;
    }

    pub fn last_al_state(&self) -> (Option<AlState>, Option<AlStatusCode>) {
        (self.last_al_state, self.last_al_status_code)
    }

    pub fn last_wkc(&self) -> u16 {
        self.last_wkc
    }
}

impl Cyclic for AlStateReader {
    fn is_finished(&self) -> bool {
        true
    }

    fn next_command(&mut self, buf: &mut [u8]) -> Option<(Command, usize)> {
        self.command = Command::new_read(self.target, AlStatus::ADDRESS);
        buf[..AlStatus::SIZE].fill(0);
        Some((self.command, AlStatus::SIZE))
    }

    fn recieve_and_process(&mut self, recv_data: &CommandData, _: EtherCatSystemTime) {
        let data = {
            let CommandData { wkc, data, .. } = recv_data;
            let wkc = *wkc;
            if wkc != self.target.num_targets() {
                self.invalid_wkc_count = self.invalid_wkc_count.saturating_add(1);
            }
            data
        };
        let al_status = AlStatus(data);
        let al_state = AlState::from(al_status.state());
        self.last_al_state = Some(al_state);
        if al_status.change_err() {
            self.last_al_status_code = Some(al_status.get_al_status_code());
        } else {
            self.last_al_status_code = None;
        }
    }
}

const fn buffer_size() -> usize {
    let mut size = 0;
    size = const_max(size, AlStatus::SIZE);
    size
}
