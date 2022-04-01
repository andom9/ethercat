use crate::arch::*;
use crate::error::*;
use crate::interface::*;
use crate::packet::*;
use crate::register::{application::*, datalink::*};
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
}

impl From<CommonError> for AlStateTransitionError {
    fn from(err: CommonError) -> Self {
        Self::Common(err)
    }
}

pub struct ALStateTransfer<'a, 'b, D, T, U>
where
    D: Device,
    T: CountDown<Time = MicrosDurationU32>,
    U: CountDown<Time = MicrosDurationU32>,
{
    iface: &'a mut EtherCATInterface<'b, D, T>,
    timer: &'a mut U,
}

impl<'a, 'b, D, T, U> ALStateTransfer<'a, 'b, D, T, U>
where
    D: Device,
    T: CountDown<Time = MicrosDurationU32>,
    U: CountDown<Time = MicrosDurationU32>,
{
    pub fn new(iface: &'a mut EtherCATInterface<'b, D, T>, timer: &'a mut U) -> Self {
        Self { iface, timer }
    }

    pub fn al_state(
        &mut self,
        slave_address: SlaveAddress,
    ) -> Result<AlState, AlStateTransitionError> {
        let al_status = self.iface.read_al_status(slave_address)?;
        let al_state = AlState::from(al_status.state());
        Ok(al_state)
    }

    pub fn to_init_state(
        &mut self,
        slave_address: SlaveAddress,
    ) -> Result<(), AlStateTransitionError> {
        let al_state = self.al_state(slave_address)?;
        if let AlState::Init = al_state {
            return Ok(());
        }

        let timeout = BACK_TO_INIT_TIMEOUT_DEFAULT_MS;

        let mut al_control = ALControl::new();
        al_control.set_state(AlState::Init as u8);
        self.iface
            .write_al_control(slave_address, Some(al_control))?;
        self.timer
            .start(MillisDurationU32::from_ticks(timeout).convert());
        loop {
            let al_status = self.iface.read_al_status(slave_address)?;
            let al_state = AlState::from(al_status.state());
            if let AlState::Init = al_state {
                return Ok(());
            }
            match self.timer.wait() {
                Ok(_) => return Err(AlStateTransitionError::TimeoutMs(timeout)),
                Err(nb::Error::Other(_)) => {
                    return Err(AlStateTransitionError::Common(CommonError::TimerError))
                }
                Err(nb::Error::WouldBlock) => (),
            }
        }
    }
}

//TODO
#[derive(Debug, Clone)]
pub enum AlStatusCode {
    NoError = 0,
    InvalidInputConfig = 0x001E,
}
