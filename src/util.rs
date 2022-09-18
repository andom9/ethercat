pub(crate) const fn const_max(a: usize, b: usize) -> usize {
    if a > b {
        a
    } else {
        b
    }
}

pub(crate) fn byte_length(bit_length: u16) -> u16 {
    if bit_length % 8 == 0 {
        bit_length >> 3
    } else {
        (bit_length >> 3) + 1
    }
}