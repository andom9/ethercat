use crate::register::datalink::PortPhysics;
use core::cell::RefCell;

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

#[derive(Debug, Clone, Default)]
pub struct SlaveId {
    pub vender_id: u16,
    pub product_code: u16,
    pub revision_number: u16,
}

#[derive(Debug, Clone, Default)]
pub struct SlaveStatus {
    pub error: Option<SlaveError>,
    pub al_state: AlState,
    pub(crate) mailbox_count: u8,
    pub linked_ports: [bool; 4],
}

#[derive(Debug, Default)]
pub struct Slave<'a, 'b> {
    pub(crate) info: SlaveInfo,
    pub(crate) status: SlaveStatus,
    pub(crate) pdo_mappings: Option<SlavePdo<'a, 'b>>,

    // for Dc init
    pub(crate) dc_context: RefCell<DcContext>,
}

impl<'a, 'b> Slave<'a, 'b> {
    pub(crate) fn increment_mb_count(&mut self) {
        if self.status.mailbox_count < 7 {
            self.status.mailbox_count += 1;
        } else {
            self.status.mailbox_count = 1;
        }
    }

    pub fn info(&self) -> &SlaveInfo {
        &self.info
    }

    pub fn status(&self) -> &SlaveStatus {
        &self.status
    }

    pub fn pdo_mappings(&self) -> Option<&SlavePdo<'a, 'b>> {
        self.pdo_mappings.as_ref()
    }

    pub fn set_rx_pdo_mappings(&mut self, mappings: &'a [PdoMapping<'b>]) {
        if let Some(ref mut pdo_mappings) = self.pdo_mappings {
            pdo_mappings.rx_mapping = mappings;
        } else {
            self.pdo_mappings = Some(SlavePdo {
                rx_mapping: mappings,
                tx_mapping: &[],
            });
        }
    }

    pub fn set_tx_pdo_mappings(&mut self, mappings: &'a [PdoMapping<'b>]) {
        if let Some(ref mut pdo_mappings) = self.pdo_mappings {
            pdo_mappings.tx_mapping = mappings;
        } else {
            self.pdo_mappings = Some(SlavePdo {
                tx_mapping: mappings,
                rx_mapping: &[],
            });
        }
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
    pub configured_address: u16,

    pub id: SlaveId,
    pub ports: [Option<PortPhysics>; 4],
    pub ram_size_kb: u8,

    pub number_of_fmmu: u8,
    pub number_of_sm: u8,

    pub pdo_start_address: Option<u16>,
    pub pdo_ram_size: u16,

    pub sm: [Option<SyncManager>; 4],

    pub support_dc: bool,
    pub is_dc_range_64bits: bool,
    pub support_fmmu_bit_operation: bool,
    pub support_lrw: bool,
    pub support_rw: bool,

    pub support_coe: bool,

    pub strict_al_control: bool,
}

impl SlaveInfo {
    pub(crate) fn mailbox_rx_sm(&self) -> Option<(u16, MailboxSyncManager)> {
        for (i, sm) in self.sm.iter().enumerate() {
            if let Some(SyncManager::MailboxRx(sm)) = sm {
                return Some((i as u16, *sm));
            }
        }
        None
    }
    pub(crate) fn mailbox_tx_sm(&self) -> Option<(u16, MailboxSyncManager)> {
        for (i, sm) in self.sm.iter().enumerate() {
            if let Some(SyncManager::MailboxTx(sm)) = sm {
                return Some((i as u16, *sm));
            }
        }
        None
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
pub enum SyncManager {
    MailboxRx(MailboxSyncManager),
    MailboxTx(MailboxSyncManager),
    ProcessDataRx,
    ProcessDataTx,
}

#[derive(Debug, Clone, Copy)]
pub struct MailboxSyncManager {
    pub size: u16,
    pub start_address: u16,
}

#[derive(Debug, Clone)]
pub enum OperationMode {
    FreeRun,
    Sync0Event,
    Sync1Event,
    SyncManagerEvent,
}

impl Default for OperationMode {
    fn default() -> Self {
        OperationMode::FreeRun
    }
}

#[derive(Debug)]
pub struct SlavePdo<'a, 'b> {
    pub rx_mapping: &'a [PdoMapping<'b>],
    pub tx_mapping: &'a [PdoMapping<'b>],
}

#[derive(Debug)]
pub struct PdoMapping<'a> {
    pub is_fixed: bool,
    pub index: u16,
    pub entries: &'a [PdoEntry],
}

#[derive(Debug)]
pub struct PdoEntry {
    pub index: u16,
    pub sub_index: u8,
    pub bit_length: usize,
}
