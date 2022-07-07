pub(crate) fn get_ap_adp(slave_number: u16) -> u16 {
    if slave_number == 0 {
        0
    } else {
        0xFFFF - (slave_number - 1)
    }
}

pub(crate) const fn const_max(a: usize, b: usize) -> usize {
    if a > b {
        a
    } else {
        b
    }
}
