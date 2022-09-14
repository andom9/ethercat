//use smoltcp::phy::{RxToken, TxToken};
use core::time::Duration;
use nb;
use void::Void;

/// A smoltcp-like raw packet network interface.
pub trait Device<'a> {
    type TxToken: TxToken + 'a;
    type RxToken: RxToken + 'a;

    /// Return TxToken if it is available for transmission. It should be non-blocking.
    fn transmit(&'a mut self) -> Option<Self::TxToken>;

    /// Return TxToken if it is receivable. It should be non-blocking.
    fn receive(&'a mut self) -> Option<Self::RxToken>;

    //fn max_transmission_unit(&self) -> usize;
}

pub trait TxToken {
    /// It should be non-blocking.
    fn consume<F>(self, len: usize, f: F) -> Result<(), ()>
    where
        F: FnOnce(&mut [u8]) -> Result<(), ()>;
}

pub trait RxToken {
    /// It should be non-blocking.
    fn consume<F>(self, f: F) -> Result<(), ()>
    where
        F: FnOnce(&[u8]) -> Result<(), ()>;
}

/// A count down timer
pub trait CountDown {
    fn start<T>(&mut self, count: T)
    where
        T: Into<Duration>;
    fn wait(&mut self) -> nb::Result<(), Void>;
}
