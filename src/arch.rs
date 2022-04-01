use smoltcp::phy::{RxToken, TxToken};

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

pub struct SmoltcpWrapper<D: for<'a> smoltcp::phy::Device<'a>>(D);

impl<D> From<D> for SmoltcpWrapper<D>
where
    D: for<'a> smoltcp::phy::Device<'a>,
{
    fn from(device: D) -> Self {
        Self(device)
    }
}

impl<D> Device for SmoltcpWrapper<D>
where
    D: for<'a> smoltcp::phy::Device<'a>,
{
    fn send<R, F>(&mut self, len: usize, f: F) -> Option<R>
    where
        F: FnOnce(&mut [u8]) -> Option<R>,
    {
        if let Some(tx_token) = self.0.transmit() {
            if let Ok(ret) =
                tx_token.consume(smoltcp::time::Instant::from_micros(0), len, |frame| {
                    Ok(f(frame))
                })
            {
                ret
            } else {
                None
            }
        } else {
            None
        }
    }

    fn recv<R, F>(&mut self, f: F) -> Option<R>
    where
        F: FnOnce(&[u8]) -> Option<R>,
    {
        if let Some((rx_token, _tx_token)) = self.0.receive() {
            if let Ok(ret) =
                rx_token.consume(smoltcp::time::Instant::from_micros(0), |frame| Ok(f(frame)))
            {
                ret
            } else {
                None
            }
        } else {
            None
        }
    }

    fn max_transmission_unit(&self) -> usize {
        self.0.capabilities().max_transmission_unit
    }
}
