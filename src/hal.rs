//use smoltcp::phy::{RxToken, TxToken};
use core::time::Duration;
// use nb;
// use void::Void;

/// A smoltcp-like raw packet network interface.
pub trait RawEthernetDevice<'a> {
    type TxToken: TxToken + 'a;
    type RxToken: RxToken + 'a;

    /// If phi is not busy, return token. It should be non-blocking.
    fn transmit(&'a mut self) -> Option<Self::TxToken>;

    /// If phi is not busy, return token. It should be non-blocking.
    fn receive(&'a mut self) -> Option<Self::RxToken>;
}

pub trait TxToken {
    fn consume<F>(self, len: usize, f: F) -> Result<(), ()>
    where
        F: FnOnce(&mut [u8]) -> Result<(), ()>;
}

pub trait RxToken {
    fn consume<F>(self, f: F) -> Result<(), ()>
    where
        F: FnOnce(&[u8]) -> Result<(), ()>;
}
