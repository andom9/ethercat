use bitfield::*;

const R1: u16 = 0x0120;//RW
const R2: u16 = 0x0121;//RW
const R3: u16 = 0x0130;//R
const R4: u16 = 0x0131;//R
const R5: u16 = 0x0132;//R
const R6: u16 = 0x0134;//R
const R7: u16 = 0x0140;//R
const R8: u16 = 0x0150;//R


bitfield! {
    pub struct ALControl(MSB0 [u8]);
    pub u8, state, set_state: 3, 0;
    pub acknowledge, set_acknowledge: 4;
    pub appl_specific, set_appl_specific: 8*2-1, 8*1;
}

impl<B: AsRef<[u8]>> ALControl<B> {
    pub const ADDRESS: u16 = R1;
    pub const SIZE: u8 = 2;

    pub fn new(buf: B) -> Option<Self> {
        if buf.as_ref().len() < Self::SIZE.into() {
            None
        } else {
            Some(Self(buf))
        }
    }
}

bitfield! {
    pub struct ALStatus(MSB0 [u8]);
    pub u8, state, _: 3, 0;
    pub change_err, _: 4;
    pub appl_specific, _: 8*2-1, 8*1;
    pub al_status_code, _: 8*6-1, 8*4;
}

impl<B: AsRef<[u8]>> ALStatus<B> {
    pub const ADDRESS: u16 = R3;
    pub const SIZE: u8 = 2;

    pub fn new(buf: B) -> Option<Self> {
        if buf.as_ref().len() < Self::SIZE.into() {
            None
        } else {
            Some(Self(buf))
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash, Copy)]
pub enum AlState {
    Init = 0x1,
    PreOperational = 0x2,
    Bootstrap = 0x3,
    SafeOperational = 0x4,
    Operational = 0x8,
    Invalid,
}

impl From<u8> for AlState {
    fn from(v: u8) -> Self {
        if v == AlState::Init as u8 {
            AlState::Init
        } else if v == AlState::PreOperational as u8 {
            AlState::PreOperational
        } else if v == AlState::Bootstrap as u8 {
            AlState::Bootstrap
        } else if v == AlState::SafeOperational as u8 {
            AlState::SafeOperational
        } else if v == AlState::PreOperational as u8 {
            AlState::PreOperational
        } else if v == AlState::Operational as u8 {
            AlState::Operational
        } else {
            AlState::Invalid
        }
    }
}