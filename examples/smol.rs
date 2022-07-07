use ethercat_master::hal::*;

struct SmolDeviceWrapper<D>
where
    D: for<'a> smoltcp::phy::Device<'a>,
{
    device: D,
}

struct SmolTxTokenWrapper<T: smoltcp::phy::TxToken>(T);
impl<T> TxToken for SmolTxTokenWrapper<T>
where
    T: smoltcp::phy::TxToken,
{
    fn consume<F>(self, len: usize, f: F) -> Result<(), ()>
    where
        F: FnOnce(&mut [u8]) -> Result<(), ()>,
    {
        self.0
            .consume(smoltcp::time::Instant::from_secs(0), len, |buf| {
                f(buf).map_err(|_| smoltcp::Error::Illegal)
            })
            .map_err(|_| ())
    }
}

struct SmolRxTokenWrapper<T: smoltcp::phy::RxToken>(T);
impl<T> RxToken for SmolRxTokenWrapper<T>
where
    T: smoltcp::phy::RxToken,
{
    fn consume<F>(self, f: F) -> Result<(), ()>
    where
        F: FnOnce(&[u8]) -> Result<(), ()>,
    {
        self.0
            .consume(smoltcp::time::Instant::from_secs(0), |buf| {
                f(buf).map_err(|_| smoltcp::Error::Illegal)
            })
            .map_err(|_| ())
    }
}

impl<'a, D> Device<'a> for SmolDeviceWrapper<D>
where
    D: for<'d> smoltcp::phy::Device<'d>,
{
    type TxToken = SmolTxTokenWrapper<<D as smoltcp::phy::Device<'a>>::TxToken>;
    type RxToken = SmolRxTokenWrapper<<D as smoltcp::phy::Device<'a>>::RxToken>;
    fn transmit(&'a mut self) -> Option<Self::TxToken> {
        self.device
            .transmit()
            .map(|token| SmolTxTokenWrapper(token))
    }

    fn receive(&'a mut self) -> Option<Self::RxToken> {
        self.device
            .receive()
            .map(|(token, _)| SmolRxTokenWrapper(token))
    }

    fn max_transmission_unit(&self) -> usize {
        self.device.capabilities().max_transmission_unit
    }
}

fn main() {
    todo!()
}
