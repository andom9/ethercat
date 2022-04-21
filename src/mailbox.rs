use crate::al_state_transfer::*;
use crate::arch::*;
use crate::error::*;
use crate::interface::*;
use crate::packet::*;
use crate::register::datalink::*;
use crate::sii::*;
use crate::slave_status::*;
use bit_field::BitField;
use embedded_hal::timer::*;
use fugit::*;

#[derive(Debug, Clone)]
pub enum MailboxError {
    Common(CommonError),
}
