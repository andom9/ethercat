//use smoltcp::phy::{RxToken, TxToken};
use core::time::Duration;
use nb;
use void::Void;

/// Raw Packet Device
pub trait Device {
    fn send<R, F>(&mut self, len: usize, f: F) -> Option<R>
    where
        F: FnOnce(&mut [u8]) -> Option<R>;

    fn recv<R, F>(&mut self, f: F) -> Option<R>
    where
        F: FnOnce(&[u8]) -> Option<R>;

    fn max_transmission_unit(&self) -> usize;
}

/// A count down timer
pub trait CountDown {
    fn start<T>(&mut self, count: T)
    where
        T: Into<Duration>;
    fn wait(&mut self) -> nb::Result<(), Void>;
}

//pub struct SmolDeviceWrapper<D: for<'a> smoltcp::phy::Device<'a>>(D);
//
//impl<D> From<D> for SmolDeviceWrapper<D>
//where
//    D: for<'a> smoltcp::phy::Device<'a>,
//{
//    fn from(device: D) -> Self {
//        Self(device)
//    }
//}

//impl<D> Device for SmolDeviceWrapper<D>
//where
//    D: for<'a> smoltcp::phy::Device<'a>,
//{
//    fn send<R, F>(&mut self, len: usize, f: F) -> Option<R>
//    where
//        F: FnOnce(&mut [u8]) -> Option<R>,
//    {
//        if let Some(tx_token) = self.0.transmit() {
//            if let Ok(ret) =
//                tx_token.consume(smoltcp::time::Instant::from_micros(0), len, |frame| {
//                    Ok(f(frame))
//                })
//            {
//                ret
//            } else {
//                None
//            }
//        } else {
//            None
//        }
//    }
//
//    fn recv<R, F>(&mut self, f: F) -> Option<R>
//    where
//        F: FnOnce(&[u8]) -> Option<R>,
//    {
//        if let Some((rx_token, _tx_token)) = self.0.receive() {
//            if let Ok(ret) =
//                rx_token.consume(smoltcp::time::Instant::from_micros(0), |frame| Ok(f(frame)))
//            {
//                ret
//            } else {
//                None
//            }
//        } else {
//            None
//        }
//    }
//
//    fn max_transmission_unit(&self) -> usize {
//        self.0.capabilities().max_transmission_unit
//    }
//}
