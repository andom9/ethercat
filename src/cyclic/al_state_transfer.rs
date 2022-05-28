use crate::cyclic::Cyclic;
use crate::error::*;
use crate::interface::*;
use crate::network::*;
use crate::register::application::*;
use crate::slave::*;
use crate::util::*;
use crate::*;
use embedded_hal::timer::CountDown;
use fugit::*;
use nb;

use super::EtherCATSystemTime;
//const TIMEOUT_MS: u32 = 200;

#[derive(Debug, Clone)]
pub enum AlStateTransitionError {
    Common(CommonError),
    TimeoutMs(u32),
    AlStatusCode(AlStatusCode),
    InvalidALState,
}

impl From<CommonError> for AlStateTransitionError {
    fn from(err: CommonError) -> Self {
        Self::Common(err)
    }
}

#[derive(Debug)]
enum TransferState {
    Error(AlStateTransitionError),
    Idle,
    Read,
    Complete,
    Request,
    Poll,
}

#[derive(Debug)]
pub struct ALStateTransfer {
    //timer: &'a mut T,
    timer_start: EtherCATSystemTime,
    state: TransferState,
    slave_address: SlaveAddress,
    target_al: Option<AlState>,
    command: Command,
    buffer: [u8; buffer_size()],
    current_al_state: Option<AlState>,
    timeout_ms: u32,
}

impl ALStateTransfer {
    pub fn new() -> Self {
        Self {
            timer_start: EtherCATSystemTime(0),
            state: TransferState::Idle,
            slave_address: SlaveAddress::SlaveNumber(0),
            target_al: None,
            command: Command::default(),
            buffer: [0; buffer_size()],
            current_al_state: None,
            timeout_ms: 0,
        }
    }

    pub fn start(&mut self, slave_address: SlaveAddress, target_al_state: Option<AlState>) {
        self.slave_address = slave_address;
        self.target_al = target_al_state;
        self.state = TransferState::Read;
        self.buffer.fill(0);
        self.command = Command::default();
    }

    pub fn wait(&mut self) -> nb::Result<AlState, AlStateTransitionError> {
        match &self.state {
            TransferState::Complete => Ok(self.current_al_state.unwrap()),
            TransferState::Error(err) => Err(nb::Error::Other(err.clone())),
            _ => Err(nb::Error::WouldBlock),
        }
    }

    //fn start(&mut self, slave_address: SlaveAddress, target_al_state: AlState) -> bool {
    //    match self.state {
    //        TransferState::Idle | TransferState::Complete | TransferState::Error(_) => {
    //            self.reset();
    //            self.target_al = target_al_state;
    //            self.slave_address = slave_address;
    //            self.state = TransferState::Read;
    //            true
    //        }
    //        _ => false,
    //    }
    //}

    //pub fn reset(&mut self) {
    //    self.state = TransferState::Read;
    //    self.command = Command::default();
    //    self.buffer.fill(0);
    //    self.current_al_state = None;
    //    self.timeout_ms = 0;
    //}

    //fn error(&self) -> Option<AlStateTransitionError> {
    //    if let TransferState::Error(err) = &self.state {
    //        Some(err.clone())
    //    } else {
    //        None
    //    }
    //}

    //fn wait_al_state(&self) -> Result<Option<AlState>, AlStateTransitionError> {
    //    if let TransferState::Error(err) = &self.state {
    //        Err(err.clone())
    //    } else {
    //        if let TransferState::Complete = &self.state {
    //            Ok(Some(self.current_al_state.unwrap()))
    //        } else {
    //            Ok(None)
    //        }
    //    }
    //}

    //pub(crate) fn take_timer(self) -> &'a mut T {
    //    self.timer
    //}
}

impl Cyclic for ALStateTransfer {
    fn next_command(
        &mut self,
        _: &mut NetworkDescription,
        _: EtherCATSystemTime,
    ) -> Option<(Command, &[u8])> {
        match self.state {
            TransferState::Idle => None,
            TransferState::Error(_) => None,
            TransferState::Read => {
                self.command = Command::new_read(self.slave_address, ALStatus::ADDRESS);
                self.buffer.fill(0);
                Some((self.command, &self.buffer[..ALStatus::SIZE]))
            }
            TransferState::Request => {
                self.buffer.fill(0);
                let mut al_control = ALControl(self.buffer);
                let target_al = self.target_al.unwrap();
                al_control.set_state(target_al as u8);
                self.command = Command::new_write(self.slave_address, ALControl::ADDRESS);
                self.timeout_ms = match (self.current_al_state.unwrap(), target_al) {
                    (AlState::PreOperational, AlState::SafeOperational)
                    | (_, AlState::Operational) => SAFEOP_OP_TIMEOUT_DEFAULT_MS,
                    (_, AlState::PreOperational) | (_, AlState::Bootstrap) => {
                        PREOP_TIMEOUT_DEFAULT_MS
                    }
                    (_, AlState::Init) => BACK_TO_INIT_TIMEOUT_DEFAULT_MS,
                    (_, AlState::SafeOperational) => BACK_TO_SAFEOP_TIMEOUT_DEFAULT_MS,
                    (_, AlState::Invalid) => unreachable!(),
                };
                Some((self.command, &self.buffer[..ALControl::SIZE]))
            }
            TransferState::Poll => {
                self.command = Command::new_read(self.slave_address, ALStatus::ADDRESS);
                self.buffer.fill(0);
                Some((self.command, &self.buffer[..ALStatus::SIZE]))
            }
            TransferState::Complete => None,
        }
    }

    fn recieve_and_process(
        &mut self,
        command: Command,
        data: &[u8],
        wkc: u16,
        _: &mut NetworkDescription,
        sys_time: EtherCATSystemTime,
    ) -> bool {
        if command != self.command {
            self.state =
                TransferState::Error(AlStateTransitionError::Common(CommonError::PacketDropped));
        }
        if wkc != 1 {
            self.state = TransferState::Error(AlStateTransitionError::Common(
                CommonError::UnexpectedWKC(wkc),
            ));
        }

        match self.state {
            TransferState::Idle => {}
            TransferState::Error(_) => {}
            TransferState::Read => {
                let al_status = ALStatus(data);
                let al_state = AlState::from(al_status.state());
                self.current_al_state = Some(al_state);
                if let Some(target_al) = self.target_al {
                    if target_al == al_state {
                        self.state = TransferState::Complete;
                    } else if al_state == AlState::Invalid {
                        self.state = TransferState::Error(AlStateTransitionError::InvalidALState);
                    } else {
                        self.state = TransferState::Request;
                    }
                } else {
                    self.state = TransferState::Complete;
                }
            }
            TransferState::Request => {
                //self.timer
                //.as_mut()
                //.unwrap()
                //    .start(MicrosDurationU32::from_ticks(self.timeout_ms * 1000));
                self.timer_start = sys_time;
                self.state = TransferState::Poll;
            }
            TransferState::Poll => {
                let al_status = ALStatus(data);
                let al_state = AlState::from(al_status.state());
                self.current_al_state = Some(al_state);
                if self.target_al.unwrap() == al_state {
                    self.state = TransferState::Complete;
                } else if al_state == AlState::Invalid {
                    self.state = TransferState::Error(AlStateTransitionError::InvalidALState);
                } else {
                    if self.timer_start.0 < sys_time.0
                        && self.timeout_ms as u64 * 1000 < sys_time.0 - self.timer_start.0
                    {
                        self.state = TransferState::Error(AlStateTransitionError::TimeoutMs(
                            self.timeout_ms,
                        ));
                    }
                }
            }
            TransferState::Complete => {}
        }

        if let TransferState::Error(_) = self.state {
            false
        } else {
            true
        }
    }
}

const fn buffer_size() -> usize {
    let mut size = 0;
    size = const_max(size, ALStatus::SIZE);
    size = const_max(size, ALControl::SIZE);
    size
}

//TODO
#[derive(Debug, Clone)]
pub enum AlStatusCode {
    NoError = 0,
    InvalidInputConfig = 0x001E,
}
