use super::{LogicalBits, SlaveId};

#[derive(Debug)]
pub struct SlaveConfig<'a, 'b> {
    input_pdo_mappings: &'a mut [PdoMapping<'b>],
    output_pdo_mappings: &'a mut [PdoMapping<'b>],
    expectec_id: Option<SlaveId>,
    pub sync_mode: SyncMode,
    pub cycle_time_ns: u32,
}

impl<'a, 'b> Default for SlaveConfig<'a, 'b> {
    fn default() -> Self {
        Self {
            cycle_time_ns: 0x0007A120_u32, //500us
            sync_mode: SyncMode::FreeRun,
            output_pdo_mappings: &mut [],
            input_pdo_mappings: &mut [],
            expectec_id: None,
        }
    }
}

impl<'a, 'b> SlaveConfig<'a, 'b> {
    pub fn input_process_data_mappings(&self) -> &[PdoMapping<'b>] {
        &self.input_pdo_mappings
    }

    pub fn input_process_data_mappings_mut(&mut self) -> &mut [PdoMapping<'b>] {
        &mut self.input_pdo_mappings
    }

    pub fn output_process_data_mappings(&self) -> &[PdoMapping<'b>] {
        self.output_pdo_mappings
    }

    pub fn output_process_data_mappings_mut(&mut self) -> &mut [PdoMapping<'b>] {
        &mut self.output_pdo_mappings
    }
}

#[derive(Debug, Clone)]
pub enum SyncMode {
    FreeRun = 0x00,
    SyncManagerEvent = 0x01,
    Sync0Event = 0x02,
    Sync1Event = 0x03,
}

impl Default for SyncMode {
    fn default() -> Self {
        SyncMode::FreeRun
    }
}

#[derive(Debug)]
pub struct PdoMapping<'a> {
    pub is_fixed: bool,
    pub index: u16,
    pub entries: &'a mut [PdoEntry],
}

#[derive(Debug, Clone)]
pub struct PdoEntry {
    logical_bits: LogicalBits,
    index: u16,
    sub_index: u8,
}

impl PdoEntry {
    pub fn new(index: u16, sub_index: u8, bit_length: u8) -> Self {
        let mut logical_bits = LogicalBits::new();
        logical_bits.set_bit_length(bit_length as u16);
        PdoEntry {
            logical_bits,
            index,
            sub_index,
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

    pub fn index(&self) -> u16 {
        self.index
    }

    pub fn sub_index(&self) -> u8 {
        self.sub_index
    }

    pub fn bit_length(&self) -> u8 {
        self.logical_bits.bit_length() as u8
    }

    pub(crate) fn byte_length(&self) -> u8 {
        self.logical_bits.byte_length() as u8
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
