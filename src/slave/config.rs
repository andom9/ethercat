#[derive(Debug)]
pub struct SlaveConfig<'a, 'b> {
    // inputs
    tx_pdo_mappings: &'a mut [PdoMapping<'b>],

    // outputs
    rx_pdo_mappings: &'a mut [PdoMapping<'b>],

    pub operation_mode: SyncMode,
    pub cycle_time_ns: u32,
}

impl<'a, 'b> Default for SlaveConfig<'a, 'b> {
    fn default() -> Self {
        Self {
            cycle_time_ns: 0x0007A120_u32, //500us
            operation_mode: SyncMode::FreeRun,
            rx_pdo_mappings: &mut [],
            tx_pdo_mappings: &mut [],
        }
    }
}

impl<'a, 'b> SlaveConfig<'a, 'b> {
    pub fn tx_process_data_mappings(&self) -> &[PdoMapping<'b>] {
        &self.tx_pdo_mappings
    }

    pub fn tx_process_data_mappings_mut(&mut self) -> &mut [PdoMapping<'b>] {
        &mut self.tx_pdo_mappings
    }

    pub fn rx_process_data_mappings(&self) -> &[PdoMapping<'b>] {
        self.rx_pdo_mappings
    }

    pub fn rx_process_data_mappings_mut(&mut self) -> &mut [PdoMapping<'b>] {
        &mut self.rx_pdo_mappings
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
    pub logical_start_address: Option<u32>,
    pub start_bit: u8,
    pub bit_length: u8,
    pub index: u16,
    pub sub_index: u8,
}

impl PdoEntry {
    pub fn new(index: u16, sub_index: u8, bit_length: u8) -> Self {
        PdoEntry {
            logical_start_address: None,
            start_bit: 0,
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

    pub fn bit_length(&self) -> u8 {
        self.bit_length
    }

    pub(crate) fn byte_length(&self) -> u8 {
        let length = self.bit_length + self.start_bit;
        if length % 8 == 0 {
            length >> 3
        } else {
            (length >> 3) + 1
        }
    }

    pub fn read(
        &self,
        logical_address_offset: u32,
        pdo_image: &[u8],
        buf: &mut [u8],
    ) -> Option<()> {
        let size = self.byte_length() as usize;
        let logical_start_address = self.logical_start_address?;
        let start_bit = self.start_bit;
        let pdo_offset = (logical_start_address - logical_address_offset) as usize;
        pdo_image.get(pdo_offset + size - 1)?;
        buf.get(size - 1)?;
        (0..size - 1).for_each(|i| {
            let v = pdo_image[pdo_offset + i] >> start_bit;
            let next_v = pdo_image[pdo_offset + i + 1] << (7 - start_bit);
            buf[i] = v | next_v;
        });
        buf[size - 1] = pdo_image[pdo_offset + size - 1] >> start_bit;
        Some(())
    }

    pub fn write(
        &self,
        logical_address_offset: u32,
        pdo_image: &mut [u8],
        data: &[u8],
    ) -> Option<()> {
        let size = self.byte_length() as usize;
        let logical_start_address = self.logical_start_address?;
        let start_bit = self.start_bit;
        let pdo_offset = (logical_start_address - logical_address_offset) as usize;
        pdo_image.get(pdo_offset + size - 1)?;
        (0..size - 1).for_each(|i| {
            pdo_image[pdo_offset + i] &= 0xFF >> (7 - start_bit);
            pdo_image[pdo_offset + i] |= data[i] << start_bit;
        });
        pdo_image[pdo_offset + size - 1] &= 0xFF << ((self.bit_length + start_bit) % 8) as u8;
        pdo_image[pdo_offset + size - 1] |= data[size - 1] << start_bit;
        if size - 2 != 0 {
            pdo_image[pdo_offset + size - 1] |= data[size - 2] >> (7 - start_bit);
        }
        Some(())
    }
}
