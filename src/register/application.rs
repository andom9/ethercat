use bitfield::*;

const R1: u16 = 0x0120; //RW
const R2: u16 = 0x0121; //RW
const R3: u16 = 0x0130; //R
const R4: u16 = 0x0131; //R
const R5: u16 = 0x0132; //R
const R6: u16 = 0x0134; //R
const R7: u16 = 0x0140; //R
const R8: u16 = 0x0150; //R

const DC_USER_P1: u16 = 0x0981; //RW
const DC_USER_P2: u16 = 0x0982; //R
const DC_USER_P3: u16 = 0x098E; //R
const DC_USER_P4: u16 = 0x0990; //RW
const DC_USER_P5: u16 = 0x09A0; //RW
const DC_USER_P6: u16 = 0x09A4; //RW
const DC_USER_P7: u16 = 0x09A8; //RW
const DC_USER_P8: u16 = 0x09AE; //RW
const DC_USER_P9: u16 = 0x09B0; //R
const DC_USER_P10: u16 = 0x09B8; //R
const DC_USER_P11: u16 = 0x09C0; //R
const DC_USER_P12: u16 = 0x09CC; //R

bitfield! {
    #[derive(Debug, Clone)]
    pub struct ALControl([u8]);
    pub u8, state, set_state: 3, 0;
    pub acknowledge, set_acknowledge: 4;
    pub u8, appl_specific, set_appl_specific: 8*2-1, 8*1;
}

impl ALControl<[u8; 2]> {
    pub const ADDRESS: u16 = R1;
    pub const SIZE: usize = 2;

    pub fn new() -> Self {
        Self([0; Self::SIZE])
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

impl ALStatus<[u8; 2]> {
    pub const ADDRESS: u16 = R3;
    pub const SIZE: usize = 2;

    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct PDIControl([u8]);
    pub u8, pdi_type, _: 7, 0;
    pub strict_al_control, _: 8;
}

impl PDIControl<[u8; 2]> {
    pub const ADDRESS: u16 = R7;
    pub const SIZE: usize = 2;

    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct PDIConfig([u8]);
    pub u8, application_specific, _: 7, 0;
}

impl PDIConfig<[u8; 1]> {
    pub const ADDRESS: u16 = R8;
    pub const SIZE: usize = 1;

    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct SyncConfig([u8]);
    pub u8, signal_conditioning_sync0, _: 1, 0;
    pub enable_signal_sync0, _: 2;
    pub enable_interrupt_sync0, _: 3;
    pub u8, signal_conditioning_sync1, _: 5, 4;
    pub enbale_signal_sync1, _: 6;
    pub enbale_interrupt_sync1, _: 7;
}

impl SyncConfig<[u8; 1]> {
    pub const ADDRESS: u16 = R8 + 1;
    pub const SIZE: usize = 1;

    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct DCActivation([u8]);
    pub cyclic_operation_enable, set_cyclic_operation_enable: 0;
    pub sync0_activate, set_sync0_activate: 1;
    pub sync1_activate, set_sync1_activate: 2;
}

impl DCActivation<[u8; 1]> {
    pub const ADDRESS: u16 = DC_USER_P1;
    pub const SIZE: usize = 1;

    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct SyncPulse([u8]);
    pub u16, sync_pulse, _: 15, 0;
}

impl SyncPulse<[u8; 2]> {
    pub const ADDRESS: u16 = DC_USER_P2;
    pub const SIZE: usize = 2;

    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct InterruptStatus([u8]);
    pub interrupt0_status, _: 0;
    pub interrupt1_status, _: 8;
}

impl InterruptStatus<[u8; 2]> {
    pub const ADDRESS: u16 = DC_USER_P3;
    pub const SIZE: usize = 2;

    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct CyclicOperationStartTime([u8]);
    pub u32, cyclic_operation_start_time, set_cyclic_operation_start_time: 31, 0;
}

impl CyclicOperationStartTime<[u8; 4]> {
    pub const ADDRESS: u16 = DC_USER_P4;
    pub const SIZE: usize = 4;

    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct Sync0CycleTime([u8]);
    pub u32, sync0_cycle_time, set_sync0_cycle_time: 31, 0;
}

impl Sync0CycleTime<[u8; 4]> {
    pub const ADDRESS: u16 = DC_USER_P5;
    pub const SIZE: usize = 4;

    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct Sync1CycleTime([u8]);
    pub u32, sync1_cycle_time, set_sync1_cycle_time: 31, 0;
}

impl Sync1CycleTime<[u8; 4]> {
    pub const ADDRESS: u16 = DC_USER_P6;
    pub const SIZE: usize = 4;

    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct LatchEdge([u8]);
    pub latch0_positive_edge, set_latch0_positive_edge: 0;
    pub latch0_negative_edge, set_latch0_negative_edge: 1;
    pub latch1_positive_edge, set_latch1_positive_edge: 8;
    pub latch1_negative_edge, set_latch1_negative_edge: 9;
}

impl LatchEdge<[u8; 2]> {
    pub const ADDRESS: u16 = DC_USER_P7;
    pub const SIZE: usize = 2;

    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct LatchEvent([u8]);
    pub latch0_positive_event, set_latch0_positive_event: 0;
    pub latch0_negative_event, set_latch0_negative_event: 1;
    pub latch1_positive_event, set_latch1_positive_event: 8;
    pub latch1_negative_event, set_latch1_negative_event: 9;
}

impl LatchEvent<[i8; 2]> {
    pub const ADDRESS: u16 = DC_USER_P8;
    pub const SIZE: usize = 2;

    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct Latch0PositiveEdgeValue([u8]);
    pub u32, latch0_positive_edge_value, set_latch0_positive_edge_value: 31, 0;
}

impl Latch0PositiveEdgeValue<[u8; 4]> {
    pub const ADDRESS: u16 = DC_USER_P9;
    pub const SIZE: usize = 4;

    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct Latch0NegativeEdgeValue([u8]);
    pub u32, latch0_negative_edge_value, set_latch0_negative_edge_value: 31, 0;
}

impl Latch0NegativeEdgeValue<[u8; 4]> {
    pub const ADDRESS: u16 = DC_USER_P10;
    pub const SIZE: usize = 4;

    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct Latch1PositiveEdgeValue([u8]);
    pub u32, latch0_positive_edge_value, set_latch0_positive_edge_value: 31, 0;
}

impl Latch1PositiveEdgeValue<[u8; 4]> {
    pub const ADDRESS: u16 = DC_USER_P11;
    pub const SIZE: usize = 4;

    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct Latch1NegativeEdgeValue([u8]);
    pub u32, latch0_negative_edge_value, set_latch0_negative_edge_value: 31, 0;
}

impl Latch1NegativeEdgeValue<[u8; 4]> {
    pub const ADDRESS: u16 = DC_USER_P12;
    pub const SIZE: usize = 4;

    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}
