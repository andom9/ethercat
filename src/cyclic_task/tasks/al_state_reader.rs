use super::super::interface::*;
use super::super::CommandData;
use super::super::EtherCatSystemTime;
use crate::cyclic_task::Cyclic;
use crate::error::EcError;
use crate::register::AlStatus;
use crate::register::AlStatusCode;
use crate::slave_network::AlState;
use crate::util::const_max;

// #[derive(Debug, Clone, PartialEq)]
// enum State {
//     Error(EcError<()>),
//     Idle,
//     Read,
//     Complete,
// }

#[derive(Debug)]
pub struct AlStateReader {
    //state: State,
    target: TargetSlave,
    command: Command,
    //buffer: [u8; buffer_size()],
    last_al_state: Option<AlState>,
    last_al_status_code: Option<AlStatusCode>,
    pub invalid_wkc_count: usize,
    pub lost_pdu_count: usize,
    last_wkc: u16,
}

impl AlStateReader {
    pub const fn required_buffer_size() -> usize {
        buffer_size()
    }

    pub fn new() -> Self {
        Self {
            //state: State::Idle,
            target: TargetSlave::default(),
            command: Command::default(),
            //buffer: [0; buffer_size()],
            last_al_state: None,
            last_al_status_code: None,
            invalid_wkc_count: 0,
            lost_pdu_count: 0,
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

    //pub fn start(&mut self, slave_address: TargetSlave) {
    //    self.slave_address = slave_address;
    //    self.state = State::Read;
    //    //self.buffer.fill(0);
    //    self.command = Command::default();
    //}

    // pub fn wait(&mut self) -> Option<Result<(AlState, Option<AlStatusCode>), EcError<()>>> {
    //     match &self.state {
    //         State::Complete => Some(Ok((self.current_al_state, self.current_al_status_code))),
    //         State::Error(err) => Some(Err(err.clone())),
    //         _ => None,
    //     }
    // }
}

impl Cyclic for AlStateReader {
    fn is_finished(&self) -> bool {
        true
    }

    fn next_command(&mut self, buf: &mut [u8]) -> Option<(Command, usize)> {
        // match self.state {
        //     State::Idle => None,
        //     State::Error(_) => None,
        //     State::Read => {
        self.command = Command::new_read(self.target, AlStatus::ADDRESS);
        //self.buffer.fill(0);
        buf[..AlStatus::SIZE].fill(0);
        Some((self.command, AlStatus::SIZE))
        // }
        // State::Complete => None,
        //}
    }

    fn recieve_and_process(&mut self, recv_data: Option<&CommandData>, _: EtherCatSystemTime) {
        let data = if let Some(recv_data) = recv_data {
            let CommandData {
                command, wkc, data, ..
            } = recv_data;
            let wkc = *wkc;
            if !(command.c_type == self.command.c_type) {
                self.lost_pdu_count = self.lost_pdu_count.saturating_add(1);
            } else if wkc != self.target.num_targets() {
                self.invalid_wkc_count = self.invalid_wkc_count.saturating_add(1);
            }
            data
        } else {
            self.lost_pdu_count = self.lost_pdu_count.saturating_add(1);
            return;
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
