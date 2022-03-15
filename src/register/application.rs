use bitfield::*;

const R1: u16 = 0x0120; //RW
const R2: u16 = 0x0121; //RW
const R3: u16 = 0x0130; //R
const R4: u16 = 0x0131; //R
const R5: u16 = 0x0132; //R
const R6: u16 = 0x0134; //R
const R7: u16 = 0x0140; //R
const R8: u16 = 0x0150; //R

bitfield! {
    #[derive(Debug, Clone)]
    pub struct ALControl([u8]);
    pub u8, state, set_state: 3, 0;
    pub acknowledge, set_acknowledge: 4;
    pub u8, appl_specific, set_appl_specific: 8*2-1, 8*1;
}

impl<B: AsRef<[u8]>> ALControl<B> {
    pub const ADDRESS: u16 = R1;
    pub const SIZE: usize = 2;

    pub fn new(buf: B) -> Option<Self> {
        if buf.as_ref().len() < Self::SIZE.into() {
            None
        } else {
            Some(Self(buf))
        }
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct ALStatus([u8]);
    pub u8, state, _: 3, 0;
    pub change_err, _: 4;
    pub u8, appl_specific, _: 8*2-1, 8*1;
    pub u16, al_status_code, _: 8*6-1, 8*4;
}

impl<B: AsRef<[u8]>> ALStatus<B> {
    pub const ADDRESS: u16 = R3;
    pub const SIZE: usize = 2;

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

bitfield! {
    #[derive(Debug, Clone)]
    pub struct PDIControl([u8]);
    pub u8, pdi_type, _: 7, 0;
    pub strict_al_control, _: 8;
}

impl<B: AsRef<[u8]>> PDIControl<B> {
    pub const ADDRESS: u16 = R7;
    pub const SIZE: usize = 2;

    pub fn new(buf: B) -> Option<Self> {
        if buf.as_ref().len() < Self::SIZE.into() {
            None
        } else {
            Some(Self(buf))
        }
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct PDIConfig([u8]);
    pub u8, application_specific, _: 7, 0;
}

impl<B: AsRef<[u8]>> PDIConfig<B> {
    pub const ADDRESS: u16 = R8;
    pub const SIZE: usize = 1;

    pub fn new(buf: B) -> Option<Self> {
        if buf.as_ref().len() < Self::SIZE.into() {
            None
        } else {
            Some(Self(buf))
        }
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct SyncConfig([u8]);
    pub u8, signal_conditioning_sync0, _: 1, 0;
    pub enbale_signal_sync0, _: 2;
    pub enbale_interrupt_sync0, _: 3;
    pub u8, signal_conditioning_sync1, _: 5, 4;
    pub enbale_signal_sync1, _: 6;
    pub enbale_interrupt_sync1, _: 7;
}

impl<B: AsRef<[u8]>> SyncConfig<B> {
    pub const ADDRESS: u16 = R8 + 1;
    pub const SIZE: usize = 1;

    pub fn new(buf: B) -> Option<Self> {
        if buf.as_ref().len() < Self::SIZE.into() {
            None
        } else {
            Some(Self(buf))
        }
    }
}
