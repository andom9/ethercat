use bitfield::*;

bitfield! {
    #[derive(Debug, Clone)]
    pub struct PdoEntry([u8]);
    pub u16, index, set_index: 8*2-1, 8*0;
    pub u8, sub_index, set_sub_index: 8*3-1, 8*2;
    pub u8, bit_length, set_bit_length: 8*4-1, 8*3;
}

impl PdoEntry<[u8; 4]> {
    pub const SIZE: usize = 4;

    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}
