use crate::memory::PortPhysics;
use crate::task::*;
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

#[derive(Debug, Clone, Default)]
pub struct SlaveId {
    pub vender_id: u16,
    pub product_code: u16,
    pub revision_number: u16,
}

#[derive(Debug, Default)]
pub struct Slave<'a, 'b> {
    pub(crate) info: SlaveInfo,

    //0:Outputs
    //1:Inputs
    //2:MBoxState
    pub(crate) fmmu: [Option<FmmuConfig>; 3],

    pub(crate) al_state: AlState,
    pub(crate) mailbox_count: Cell<u8>,

    // for Dc init
    pub(crate) dc_context: RefCell<DcContext>,

    // inputs
    tx_pdo_mappings: &'a mut [PdoMapping<'b>],

    // outputs
    rx_pdo_mappings: &'a mut [PdoMapping<'b>],
}

impl<'a, 'b> Slave<'a, 'b> {
    pub fn info(&self) -> &SlaveInfo {
        &self.info
    }

    pub fn al_state(&self) -> AlState {
        self.al_state
    }

    pub(crate) fn mailbox_count(&self) -> u8 {
        self.mailbox_count.get()
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

    pub fn tx_process_data_mappings(&'a self) -> Option<&'a [PdoMapping<'b>]> {
        if self.tx_pdo_mappings.is_empty() {
            None
        } else {
            Some(self.tx_pdo_mappings)
        }
    }

    pub fn rx_process_data_mappings(&'a self) -> Option<&'a [PdoMapping<'b>]> {
        if self.rx_pdo_mappings.is_empty() {
            None
        } else {
            Some(self.rx_pdo_mappings)
        }
    }

    pub fn set_tx_pdo_mappings(&mut self, mappings: &'a mut [PdoMapping<'b>]) {
        self.tx_pdo_mappings = mappings;
    }

    pub fn set_rx_pdo_mappings(&mut self, mappings: &'a mut [PdoMapping<'b>]) {
        self.rx_pdo_mappings = mappings;
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
    pub configured_address: u16,
    pub id: SlaveId,
    pub(crate) linked_ports: [bool; 4],
    pub ports: [Option<PortPhysics>; 4],
    pub ram_size_kb: u8,

    pub number_of_fmmu: u8,
    pub number_of_sm: u8,

    pub pdo_start_address: Option<u16>,
    pub pdo_ram_size: u16,

    pub sm: [Option<SyncManagerType>; 4],

    pub support_dc: bool,
    pub support_fmmu_bit_operation: bool,

    pub support_coe: bool,

    pub strict_al_control: bool,
}

impl SlaveInfo {
    pub fn slave_address(&self) -> SlaveAddress {
        SlaveAddress::StationAddress(self.configured_address)
    }

    pub fn linked_ports(&self) -> [bool; 4] {
        self.linked_ports
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

#[derive(Debug, Clone, Copy, Default)]
pub struct SyncManager {
    pub number: u8,
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
pub struct FmmuConfig {
    pub(crate) logical_start_address: Option<u32>,
    pub physical_address: u16,
    pub bit_length: u16,
    is_output: bool,
}
impl FmmuConfig {
    pub fn new(physical_address: u16, bit_length: u16, is_output: bool) -> Self {
        Self {
            logical_start_address: None,
            physical_address,
            bit_length,
            is_output,
        }
    }
    pub fn is_output(&self) -> bool {
        self.is_output
    }
    pub(crate) fn byte_length(&self) -> u16 {
        if self.bit_length % 8 == 0 {
            self.bit_length >> 3
        } else {
            (self.bit_length >> 3) + 1
        }
    }
    pub fn set_logical_address(&mut self, logical_address: u32) {
        self.logical_start_address = Some(logical_address);
    }
}

// #[derive(Debug)]
// pub enum ProcessDataConfig<'a, 'b> {
//     Memory(MemoryProcessData),
//     CoE(&'a mut [PdoMapping<'b>]),
// }

// impl<'a, 'b> From<MemoryProcessData> for ProcessDataConfig<'a, 'b> {
//     fn from(config: MemoryProcessData) -> Self {
//         Self::Memory(config)
//     }
// }

// impl<'a, 'b> From<&'a mut [PdoMapping<'b>]> for ProcessDataConfig<'a, 'b> {
//     fn from(config: &'a mut [PdoMapping<'b>]) -> Self {
//         Self::CoE(config)
//     }
// }

// #[derive(Debug, Clone)]
// pub struct MemoryProcessData {
//     pub(crate) logical_start_address: Option<u32>,
//     pub physical_address: u16,
//     pub bit_length: u16,
// }
// impl MemoryProcessData {
//     pub fn new(physical_address: u16, bit_length: u16) -> Self {
//         Self {
//             logical_start_address: None,
//             physical_address,
//             bit_length,
//         }
//     }
//     pub(crate) fn byte_length(&self) -> u16 {
//         if self.bit_length % 8 == 0 {
//             self.bit_length / 8
//         } else {
//             self.bit_length / 8 + 1
//         }
//     }
// }

//#[derive(Debug)]
//pub struct SlavePdo<'a, 'b> {
//    pub rx_mapping: &'a mut [PdoMapping<'b>],
//    pub tx_mapping: &'a mut [PdoMapping<'b>],
//}

#[derive(Debug)]
pub struct PdoMapping<'a> {
    pub is_fixed: bool,
    pub index: u16,
    pub entries: &'a mut [PdoEntry],
}

#[derive(Debug, Clone)]
pub struct PdoEntry {
    pub(crate) logical_start_address: Option<u32>,
    pub(crate) index: u16,
    pub(crate) sub_index: u8,
    pub(crate) bit_length: u16,
}
impl PdoEntry {
    pub fn new(index: u16, sub_index: u8, bit_length: u16) -> Self {
        PdoEntry {
            logical_start_address: None,
            index,
            sub_index,
            bit_length,
        }
    }

    pub fn index(&self) -> u16 {
        self.index
    }

    pub fn sub_index(&self) -> u8 {
        self.sub_index
    }

    pub fn bit_length(&self) -> u16 {
        self.bit_length
    }

    pub(crate) fn byte_length(&self) -> u16 {
        if self.bit_length % 8 == 0 {
            self.bit_length / 8
        } else {
            self.bit_length / 8 + 1
        }
    }

    pub fn read<'a>(&self, logical_image: &'a [u8]) -> Option<&'a [u8]> {
        let size = self.byte_length() as usize;
        self.logical_start_address?;
        let logical_start_address = self.logical_start_address.unwrap() as usize;
        logical_image.get(logical_start_address + size)?;
        Some(&logical_image[logical_start_address..logical_start_address + size])
    }

    pub fn read_unchecked<'a>(&self, logical_image: &'a [u8]) -> &'a [u8] {
        let size = self.byte_length() as usize;
        let logical_start_address = self.logical_start_address.unwrap() as usize;
        &logical_image[logical_start_address..logical_start_address + size]
    }

    pub fn write<'a>(&self, logical_image: &'a mut [u8], data: &[u8]) -> Option<()> {
        let size = self.byte_length() as usize;
        self.logical_start_address?;
        let logical_start_address = self.logical_start_address.unwrap() as usize;
        logical_image.get(logical_start_address + size)?;
        logical_image[logical_start_address..logical_start_address + size]
            .iter_mut()
            .zip(data)
            .for_each(|(image, data)| *image = *data);
        Some(())
    }

    pub fn write_unchecked<'a>(&self, logical_image: &'a mut [u8], data: &[u8]) {
        let size = self.byte_length() as usize;
        let logical_start_address = self.logical_start_address.unwrap() as usize;
        logical_image[logical_start_address..logical_start_address + size]
            .iter_mut()
            .zip(data)
            .for_each(|(image, data)| *image = *data);
    }
}
