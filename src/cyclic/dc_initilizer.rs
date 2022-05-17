use super::al_state_transfer::*;
use crate::cyclic::Cyclic;
use crate::cyclic::*;
use crate::error::*;
use crate::interface::*;
use crate::register::{application::*, datalink::*};
use crate::slave::*;
use crate::util::*;
use bit_field::BitField;
use embedded_hal::timer::*;
use fugit::*;

#[derive(Debug, Clone)]
pub enum DCState {
    Idle,
    Error(DCError),
    Complete,
    Offset,
    Drift,
}

#[derive(Debug, Clone)]
pub enum DCError {
    Common(CommonError),
}

impl From<CommonError> for DCError {
    fn from(err: CommonError) -> Self {
        Self::Common(err)
    }
}

#[derive(Debug, Clone)]
pub struct DCInitializer<'a> {
    state: DCState,
    slaves: &'a [Slave],
    command: Command,
    buffer: [u8; buffer_size()],
    count: usize,
}

impl<'a> DCInitializer<'a> {
    pub fn new(slaves: &[Slave]) {}
}

impl<'a> Cyclic for DCInitializer<'a> {
    fn next_command(&mut self) -> Option<(Command, &[u8])> {
        todo!()
    }

    fn recieve_and_process(&mut self, command: Command, data: &[u8], wkc: u16) -> bool {
        todo!()
    }
}

const fn buffer_size() -> usize {
    let mut size = 0;
    size = const_max(size, DLControl::SIZE);
    size
}
