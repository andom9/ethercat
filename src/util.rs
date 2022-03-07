use crate::error::*;
use crate::packet::*;

pub fn check_wkc<B: AsRef<[u8]>>(pdu: &EtherCATPDU<B>, expected_wkc: u16) -> Result<(), Error> {
    let wkc = pdu.wkc().ok_or(Error::Dropped)?;
    if wkc != expected_wkc {
        Err(Error::UnexpectedWKC(wkc))
    } else {
        Ok(())
    }
}

pub fn get_ap_adp(slave_number: u16) -> u16 {
    if slave_number == 0 {
        0
    } else {
        0xFFFF - (slave_number - 1)
    }
}
