use crate::interface::*;
use crate::register::PortPhysics;
use core::cell::{Cell, RefCell};

#[derive(Debug, Clone)]
pub enum SlaveError {
    PdiNotOperational,
    UnexpectedAlState,
    SmSettingsAreNotCorrect,
    WatchdogTimeout,
    PdoStateError,
    PdoControlError,
    PdoToggleError,
    EarlySmEvent,
    SmEvnetJitterTooMuch,
    SmEventNotRecieved,
    OutputCalcAndCopyNotFinished,
    Sync0NotRecieved,
    Sync1NotRecieved,
    SyncEventNotDetected,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SlaveId {
    vender_id: u16,
    product_code: u16,
    revision_number: u16,
}

impl SlaveId {
    pub fn vender_id(&self) -> u16 {
        self.vender_id
    }

    pub fn product_code(&self) -> u16 {
        self.product_code
    }

    pub fn revision_number(&self) -> u16 {
        self.revision_number
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct SlaveIdBuilder {
    pub vender_id: u16,
    pub product_code: u16,
    pub revision_number: u16,
}

impl SlaveIdBuilder {
    pub(crate) fn build(self) -> SlaveId {
        let Self {
            vender_id,
            product_code,
            revision_number,
        } = self;
        SlaveId {
            vender_id,
            product_code,
            revision_number,
        }
    }
}

#[derive(Debug, Default)]
pub struct Slave {
    info: SlaveInfo,

    //0:Outputs
    //1:Inputs
    //2:MBoxState
    fmmu: [Option<FmmuConfig>; 3],

    al_state: AlState,
    mailbox_count: Cell<u8>,

    // for Dc init
    pub(crate) dc_context: RefCell<DcContext>,
}

impl Slave {
    pub fn info(&self) -> &SlaveInfo {
        &self.info
    }

    pub(crate) fn info_mut(&mut self) -> &mut SlaveInfo {
        &mut self.info
    }

    pub fn al_state(&self) -> AlState {
        self.al_state
    }

    pub(crate) fn mailbox_count(&self) -> u8 {
        self.mailbox_count.get()
    }

    /// Mailbox count is specified in the range of 1~7
    pub(crate) fn set_mailbox_count(&self, count: u8) -> Result<(), ()> {
        if count < 1 || 7 < count {
            self.mailbox_count.set(count);
            Ok(())
        } else {
            Err(())
        }
    }

    pub(crate) fn increment_mb_count(&self) -> u8 {
        let count = self.mailbox_count();
        if count < 7 {
            self.mailbox_count.set(count + 1);
        } else {
            self.mailbox_count.set(1);
        }
        self.mailbox_count()
    }

    pub fn fmmu_config(&self) -> &[Option<FmmuConfig>] {
        &self.fmmu
    }

    pub fn fmmu_config_mut(&mut self) -> &mut [Option<FmmuConfig>] {
        &mut self.fmmu
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct DcContext {
    pub parent_port: Option<(u16, u8)>,
    pub current_port: u8,
    pub recieved_port_time: [u32; 4],
    pub delay: u32,
    pub offset: u64,
}

#[derive(Debug, Default, Clone)]
pub struct SlaveInfo {
    // info
    configured_address: u16,
    id: SlaveId,
    linked_ports: [bool; 4],
    ports: [Option<PortPhysics>; 4],
    ram_size_kb: u8,

    number_of_fmmu: u8,
    number_of_sm: u8,

    pdo_start_address: Option<u16>,
    pdo_ram_size: u16,

    sm: [Option<SyncManagerType>; 4],

    support_dc: bool,
    support_fmmu_bit_operation: bool,

    support_coe: bool,

    strict_al_control: bool,
}

impl SlaveInfo {
    pub fn slave_address(&self) -> SlaveAddress {
        SlaveAddress::StationAddress(self.configured_address)
    }

    pub fn id(&self) -> SlaveId {
        self.id
    }

    pub fn linked_ports(&self) -> [bool; 4] {
        self.linked_ports
    }

    pub fn port_type(&self) -> [Option<PortPhysics>; 4] {
        self.ports
    }

    pub fn ram_size_kb(&self) -> u8 {
        self.ram_size_kb
    }

    pub fn number_of_fmmu(&self) -> u8 {
        self.number_of_fmmu
    }

    pub fn number_of_sm(&self) -> u8 {
        self.number_of_sm
    }

    pub fn support_dc(&self) -> bool {
        self.support_dc
    }

    pub fn support_coe(&self) -> bool {
        self.support_coe
    }

    pub fn mailbox_rx_sm(&self) -> Option<SyncManager> {
        for sm in self.sm.iter() {
            if let Some(SyncManagerType::MailboxRx(sm)) = sm {
                return Some(*sm);
            }
        }
        None
    }

    pub fn mailbox_tx_sm(&self) -> Option<SyncManager> {
        for sm in self.sm.iter() {
            if let Some(SyncManagerType::MailboxTx(sm)) = sm {
                return Some(*sm);
            }
        }
        None
    }

    pub fn process_data_rx_sm_number(&self) -> Option<u8> {
        for (i, sm) in self.sm.iter().enumerate() {
            if let Some(SyncManagerType::ProcessDataRx) = sm {
                return Some(i as u8);
            }
        }
        None
    }

    pub fn process_data_tx_sm_number(&self) -> Option<u8> {
        for (i, sm) in self.sm.iter().enumerate() {
            if let Some(SyncManagerType::ProcessDataTx) = sm {
                return Some(i as u8);
            }
        }
        None
    }

    pub fn process_data_physical_start_address(&self) -> Option<u16> {
        self.pdo_start_address
    }

    pub fn proces_data_size(&self) -> u16 {
        self.pdo_ram_size
    }
}

#[derive(Debug, Default, Clone)]
pub(crate) struct SlaveInfoBuilder {
    // info
    pub configured_address: u16,
    pub id: SlaveIdBuilder,
    pub linked_ports: [bool; 4],
    pub ports: [Option<PortPhysics>; 4],
    pub ram_size_kb: u8,

    pub number_of_fmmu: u8,
    pub number_of_sm: u8,

    pub pdo_start_address: Option<u16>,
    pub pdo_ram_size: u16,

    pub sm: [Option<SyncManagerTypeBuilder>; 4],

    pub support_dc: bool,
    pub support_fmmu_bit_operation: bool,

    pub support_coe: bool,

    pub strict_al_control: bool,
}

impl SlaveInfoBuilder {
    pub(crate) fn build(self) -> SlaveInfo {
        let Self {
            configured_address,
            id,
            linked_ports,
            ports,
            ram_size_kb,
            number_of_fmmu,
            number_of_sm,
            pdo_start_address,
            pdo_ram_size,
            sm,
            support_dc,
            support_fmmu_bit_operation,
            support_coe,
            strict_al_control,
        } = self;
        let mut sm_arr: [Option<SyncManagerType>; 4] = Default::default();
        sm_arr[0] = sm[0].clone().map(|builder| builder.build());
        sm_arr[1] = sm[1].clone().map(|builder| builder.build());
        sm_arr[2] = sm[2].clone().map(|builder| builder.build());
        sm_arr[3] = sm[3].clone().map(|builder| builder.build());

        SlaveInfo {
            configured_address,
            id: id.build(),
            linked_ports,
            ports,
            ram_size_kb,
            number_of_fmmu,
            number_of_sm,
            pdo_start_address,
            pdo_ram_size,
            sm: sm_arr,
            support_dc,
            support_fmmu_bit_operation,
            support_coe,
            strict_al_control,
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
    InvalidOrMixed,
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
        } else if v == AlState::Operational as u8 {
            AlState::Operational
        } else {
            AlState::InvalidOrMixed
        }
    }
}

impl Default for AlState {
    fn default() -> Self {
        AlState::InvalidOrMixed
    }
}

#[derive(Debug, Clone)]
pub enum SyncManagerType {
    MailboxRx(SyncManager),
    MailboxTx(SyncManager),
    ProcessDataRx,
    ProcessDataTx,
}

#[derive(Debug, Clone)]
pub(crate) enum SyncManagerTypeBuilder {
    MailboxRx(SyncManagerBuilder),
    MailboxTx(SyncManagerBuilder),
    ProcessDataRx,
    ProcessDataTx,
}

impl SyncManagerTypeBuilder {
    pub(crate) fn build(self) -> SyncManagerType {
        match self {
            Self::MailboxRx(sm) => SyncManagerType::MailboxRx(sm.build()),
            Self::MailboxTx(sm) => SyncManagerType::MailboxTx(sm.build()),
            Self::ProcessDataRx => SyncManagerType::ProcessDataRx,
            Self::ProcessDataTx => SyncManagerType::ProcessDataTx,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SyncManager {
    number: u8,
    size: u16,
    start_address: u16,
}

impl SyncManager {
    pub fn number(&self) -> u8 {
        self.number
    }

    pub fn size(&self) -> u16 {
        self.size
    }

    pub fn start_address(&self) -> u16 {
        self.start_address
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SyncManagerBuilder {
    pub number: u8,
    pub size: u16,
    pub start_address: u16,
}

impl SyncManagerBuilder {
    pub(crate) fn build(self) -> SyncManager {
        let Self {
            number,
            size,
            start_address,
        } = self;
        SyncManager {
            number,
            size,
            start_address,
        }
    }
}

#[derive(Debug)]
pub struct FmmuConfig {
    pub(crate) logical_start_address: Option<u32>,
    pub(crate) start_bit: u8,
    pub physical_address: u16,
    pub bit_length: u16,
    is_output: bool,
}

impl FmmuConfig {
    pub fn new(physical_address: u16, bit_length: u16, is_output: bool) -> Self {
        Self {
            logical_start_address: None,
            start_bit: 0,
            physical_address,
            bit_length,
            is_output,
        }
    }

    pub fn is_output(&self) -> bool {
        self.is_output
    }

    pub(crate) fn byte_length(&self) -> u16 {
        crate::util::byte_length(self.bit_length + self.start_bit as u16)
    }

    pub(crate) fn byte_length_and_end_bit(&self) -> (u16, u8) {
        let mod8 = (self.bit_length + self.start_bit as u16) % 8;
        if mod8 == 0 {
            (self.bit_length >> 3, 7)
        } else {
            ((self.bit_length >> 3) + 1, mod8 as u8 - 1)
        }
    }

    pub fn set_logical_address(&mut self, logical_address: u32) {
        self.logical_start_address = Some(logical_address);
    }
}
