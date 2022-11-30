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
        //if 1 <= count || count <= 7 {
        if count <= 7 {
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
    pub latched_local_sys_time: u64,
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

#[derive(Debug, Clone)]
pub(crate) struct LogicalBits {
    logical_address: Option<u32>,
    start_bit: u8,
    bit_length: u16,
}

impl LogicalBits {
    pub fn new() -> Self {
        Self {
            logical_address: None,
            start_bit: 0,
            bit_length: 0,
        }
    }

    pub fn logical_address(&self) -> Option<u32> {
        self.logical_address
    }

    pub fn set_logical_address(&mut self, logical_address: Option<u32>) {
        self.logical_address = logical_address;
    }

    pub fn start_bit(&self) -> u8 {
        self.start_bit
    }

    pub fn set_start_bit(&mut self, start_bit: u8) {
        self.start_bit = start_bit;
    }

    pub fn bit_length(&self) -> u16 {
        self.bit_length
    }

    pub fn set_bit_length(&mut self, bit_length: u16) {
        self.bit_length = bit_length;
    }

    pub fn byte_length(&self) -> u16 {
        let length = self.bit_length + self.start_bit as u16;
        if length % 8 == 0 {
            length >> 3
        } else {
            (length >> 3) + 1
        }
    }

    pub fn end_bit(&self) -> u8 {
        let mod8 = (self.bit_length + self.start_bit as u16) % 8;
        if mod8 == 0 {
            7
        } else {
            mod8 as u8 - 1
        }
    }

    pub fn read_to_buffer(
        &self,
        logical_address_offset: u32,
        process_data_image: &[u8],
        buf: &mut [u8],
    ) -> Option<()> {
        let size = self.byte_length() as usize;
        let logical_start_address = self.logical_address?;
        let start_bit = self.start_bit;
        let pdo_offset = (logical_start_address - logical_address_offset) as usize;
        process_data_image.get(pdo_offset + size - 1)?;
        buf.get(size - 1)?;
        (0..size - 1).for_each(|i| {
            let v = process_data_image[pdo_offset + i] >> start_bit;
            let next_v = process_data_image[pdo_offset + i + 1] << (7 - start_bit);
            buf[i] = v | next_v;
        });
        buf[size - 1] = process_data_image[pdo_offset + size - 1] >> start_bit;
        Some(())
    }

    pub fn write_from_buffer(
        &self,
        logical_address_offset: u32,
        process_data_image: &mut [u8],
        buf: &[u8],
    ) -> Option<()> {
        let size = self.byte_length() as usize;
        let logical_start_address = self.logical_address?;
        let start_bit = self.start_bit;
        let pdo_offset = (logical_start_address - logical_address_offset) as usize;
        process_data_image.get(pdo_offset + size - 1)?;
        (0..size - 1).for_each(|i| {
            process_data_image[pdo_offset + i] &= 0xFF >> (7 - start_bit);
            process_data_image[pdo_offset + i] |= buf[i] << start_bit;
        });
        process_data_image[pdo_offset + size - 1] &=
            0xFF << ((self.bit_length + start_bit as u16) % 8) as u8;
        process_data_image[pdo_offset + size - 1] |= buf[size - 1] << start_bit;
        if size - 2 != 0 {
            process_data_image[pdo_offset + size - 1] |= buf[size - 2] >> (7 - start_bit);
        }
        Some(())
    }
}

#[derive(Debug, Clone)]
pub struct FmmuConfig {
    logical_bits: LogicalBits,
    physical_address: u16,
    direction: Direction,
}

impl FmmuConfig {
    pub fn new(physical_address: u16, bit_length: u16, direction: Direction) -> Self {
        let mut logical_bits = LogicalBits::new();
        logical_bits.set_bit_length(bit_length);
        Self {
            logical_bits,
            physical_address,
            direction,
        }
    }

    pub fn logical_address(&self) -> Option<u32> {
        self.logical_bits.logical_address()
    }

    pub fn set_logical_address(&mut self, logical_address: Option<u32>) {
        self.logical_bits.set_logical_address(logical_address);
    }

    pub fn start_bit(&self) -> u8 {
        self.logical_bits.start_bit()
    }

    pub fn set_start_bit(&mut self, start_bit: u8) {
        self.logical_bits.set_start_bit(start_bit);
    }

    pub fn bit_length(&self) -> u16 {
        self.logical_bits.bit_length()
    }

    pub fn set_bit_length(&mut self, bit_length: u16) {
        self.logical_bits.set_bit_length(bit_length);
    }

    pub fn physical_address(&self) -> u16 {
        self.physical_address
    }

    pub fn set_physical_address(&mut self, physical_address: u16) {
        self.physical_address = physical_address;
    }

    pub fn direction(&self) -> Direction {
        self.direction
    }

    pub fn set_direction(&mut self, direction: Direction) {
        self.direction = direction;
    }

    pub fn byte_length(&self) -> u16 {
        self.logical_bits.byte_length()
    }

    pub fn end_bit(&self) -> u8 {
        self.logical_bits.end_bit()
    }

    pub fn read_to_buffer(
        &self,
        logical_address_offset: u32,
        process_data_image: &[u8],
        buf: &mut [u8],
    ) -> Option<()> {
        self.logical_bits
            .read_to_buffer(logical_address_offset, process_data_image, buf)
    }

    pub fn write_from_buffer(
        &self,
        logical_address_offset: u32,
        process_data_image: &mut [u8],
        buf: &[u8],
    ) -> Option<()> {
        self.logical_bits
            .write_from_buffer(logical_address_offset, process_data_image, buf)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Direction {
    Input,
    Output,
}

impl Direction {
    pub fn is_output(&self) -> bool {
        match self {
            Direction::Output => true,
            Direction::Input => false,
        }
    }
}

// #[derive(Debug)]
// pub struct ProcessDataImage<'a>{
//     logical_address_offset: u32,
//     buffer: &'a mut [u8],
// }

// impl<'a> ProcessDataImage<'a>{
//     pub fn new(logical_address_offset: u32, buffer: &'a mut [u8])->Self{
//         Self { logical_address_offset, buffer }
//     }

//     pub fn logical_address_offset(&self) -> u32{
//         self.logical_address_offset
//     }

//     pub fn buffer(&self) -> &[u8]{
//         &self.buffer
//     }

//     pub fn buffer_mut(&mut self) -> &mut [u8]{
//         &mut self.buffer
//     }
// }
