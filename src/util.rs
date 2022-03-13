use crate::error::*;
use crate::interface::EtherCATInterface;
use crate::packet::*;

pub fn check_wkc<B: AsRef<[u8]>>(
    pdu: &EtherCATPDU<B>,
    expected_wkc: u16,
) -> Result<(), CommonError> {
    let wkc = pdu.wkc().ok_or(CommonError::Dropped)?;
    if wkc != expected_wkc {
        Err(CommonError::UnexpectedWKC(wkc))
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

// TODO: リードレジスターマクロを作る。
// TODO: ライトレジスターマクロを作る。
