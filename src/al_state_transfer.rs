use crate::error::*;
use crate::interface::*;
use crate::master::*;
use crate::register::application::*;
use crate::slave_status::*;
use crate::util::*;
use crate::*;
use embedded_hal::timer::CountDown;
use fugit::*;

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
pub struct ALStateTransfer<'a, T>
where
    T: CountDown<Time = MicrosDurationU32>,
{
    pub(crate) timer: Option<&'a mut T>,
    state: TransferState,
    slave_address: SlaveAddress,
    target_al: AlState,
    command: Command,
    buffer: [u8; buffer_size()],
    current_al_state: Option<AlState>,
    timeout_ms: u32,
}

impl<'a, T> ALStateTransfer<'a, T>
where
    T: CountDown<Time = MicrosDurationU32>,
{
    pub fn new(timer: &'a mut T) -> Self {
        Self {
            timer: Some(timer),
            state: TransferState::Idle,
            slave_address: SlaveAddress::default(),
            target_al: AlState::Init,
            command: Command::default(),
            buffer: [0; buffer_size()],
            current_al_state: None,
            timeout_ms: 0,
        }
    }

    pub fn start(&mut self, slave_address: SlaveAddress, target_al_state: AlState) -> bool {
        match self.state {
            TransferState::Idle | TransferState::Complete | TransferState::Error(_) => {
                self.reset();
                self.target_al = target_al_state;
                self.slave_address = slave_address;
                self.state = TransferState::Read;
                true
            }
            _ => false,
        }
    }

    pub fn reset(&mut self) {
        self.state = TransferState::Idle;
        self.target_al = AlState::Init;
        self.slave_address = SlaveAddress::default();
        self.command = Command::default();
        self.buffer.fill(0);
        self.current_al_state = None;
        self.timeout_ms = 0;
    }

    pub fn error(&self) -> Option<AlStateTransitionError> {
        if let TransferState::Error(err) = &self.state {
            Some(err.clone())
        } else {
            None
        }
    }

    pub fn wait_al_state(&self) -> Result<Option<AlState>, AlStateTransitionError> {
        if let TransferState::Error(err) = &self.state {
            Err(err.clone())
        } else {
            if let TransferState::Complete = &self.state {
                Ok(Some(self.current_al_state.unwrap()))
            } else {
                Ok(None)
            }
        }
    }
}

impl<'a, T> Cyclic for ALStateTransfer<'a, T>
where
    T: CountDown<Time = MicrosDurationU32>,
{
    fn next_transmission_data(&mut self) -> Option<(Command, &[u8])> {
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
                al_control.set_state(self.target_al as u8);
                self.command = Command::new_write(self.slave_address, ALControl::ADDRESS);
                self.timeout_ms = match (self.current_al_state.unwrap(), self.target_al) {
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

    fn recieve_and_process(&mut self, command: Command, data: &[u8], wkc: u16) -> bool {
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
                if self.target_al == al_state {
                    self.state = TransferState::Complete;
                } else if al_state == AlState::Invalid {
                    self.state = TransferState::Error(AlStateTransitionError::InvalidALState);
                } else {
                    self.state = TransferState::Request;
                }
            }
            TransferState::Request => {
                self.timer
                    .as_mut()
                    .unwrap()
                    .start(MicrosDurationU32::from_ticks(self.timeout_ms * 1000));
                self.state = TransferState::Poll;
            }
            TransferState::Poll => {
                let al_status = ALStatus(data);
                let al_state = AlState::from(al_status.state());
                self.current_al_state = Some(al_state);
                if self.target_al == al_state {
                    self.state = TransferState::Complete;
                } else if al_state == AlState::Invalid {
                    self.state = TransferState::Error(AlStateTransitionError::InvalidALState);
                } else {
                    match self.timer.as_mut().unwrap().wait() {
                        Ok(_) => {
                            self.state = TransferState::Error(AlStateTransitionError::TimeoutMs(
                                self.timeout_ms,
                            ))
                        }
                        Err(nb::Error::Other(_)) => {
                            self.state =
                                TransferState::Error(CommonError::UnspcifiedTimerError.into())
                        }
                        Err(nb::Error::WouldBlock) => (),
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
//pub struct ALStateTransfer<'a, 'b, D, T, U>
//where
//    D: Device,
//    T: CountDown<Time = MicrosDurationU32>,
//    U: CountDown<Time = MicrosDurationU32>,
//{
//    iface: &'a mut EtherCATInterface<'b, D, T>,
//    timer: &'a mut U,
//}
//
//impl<'a, 'b, D, T, U> ALStateTransfer<'a, 'b, D, T, U>
//where
//    D: Device,
//    T: CountDown<Time = MicrosDurationU32>,
//    U: CountDown<Time = MicrosDurationU32>,
//{
//    pub fn new(iface: &'a mut EtherCATInterface<'b, D, T>, timer: &'a mut U) -> Self {
//        Self { iface, timer }
//    }
//
//    pub fn al_state(
//        &mut self,
//        slave_address: SlaveAddress,
//    ) -> Result<AlState, AlStateTransitionError> {
//        let al_status = self.iface.read_al_status(slave_address)?;
//        let al_state = AlState::from(al_status.state());
//        Ok(al_state)
//    }
//
//    pub fn change_al_state(
//        &mut self,
//        slave_address: SlaveAddress,
//        al_state: AlState,
//    ) -> Result<(), AlStateTransitionError> {
//        let current_al_state = self.al_state(slave_address)?;
//        if al_state == current_al_state {
//            return Ok(());
//        }
//
//        let timeout = match (current_al_state, al_state) {
//            (AlState::PreOperational, AlState::SafeOperational)
//            | (AlState::SafeOperational, AlState::Operational) => SAFEOP_OP_TIMEOUT_DEFAULT_MS,
//            (_, AlState::PreOperational) | (_, AlState::Bootstrap) => PREOP_TIMEOUT_DEFAULT_MS,
//            (_, AlState::Init) => BACK_TO_INIT_TIMEOUT_DEFAULT_MS,
//            (_, AlState::SafeOperational) => BACK_TO_SAFEOP_TIMEOUT_DEFAULT_MS,
//        };
//
//        let mut al_control = ALControl::new();
//        al_control.set_state(al_state as u8);
//        self.iface
//            .write_al_control(slave_address, Some(al_control))?;
//        self.timer
//            .start(MillisDurationU32::from_ticks(timeout).convert());
//        loop {
//            let current_al_status = self.iface.read_al_status(slave_address)?;
//            let current_al_state = AlState::from(current_al_status.state());
//            if al_state == current_al_state {
//                return Ok(());
//            }
//            match self.timer.wait() {
//                Ok(_) => return Err(AlStateTransitionError::TimeoutMs(timeout)),
//                Err(nb::Error::Other(_)) => {
//                    return Err(AlStateTransitionError::Common(CommonError::UnspcifiedTimerError))
//                }
//                Err(nb::Error::WouldBlock) => (),
//            }
//        }
//    }
//}

//TODO
#[derive(Debug, Clone)]
pub enum AlStatusCode {
    NoError = 0,
    InvalidInputConfig = 0x001E,
}
