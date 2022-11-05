/// A smoltcp-like raw network interface.
pub trait RawEthernetDevice {
    type TxToken<'a>: TxToken
    where
        Self: 'a;
    type RxToken<'a>: RxToken
    where
        Self: 'a;

    /// If phi is not busy, return token.
    fn transmit<'a>(&'a mut self) -> Option<Self::TxToken<'a>>;

    /// If phi is not busy, return token.
    fn receive<'a>(&'a mut self) -> Option<Self::RxToken<'a>>;
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

#[cfg(feature = "smoltcp")]
pub mod smoltcp_wrapper {
    use super::{RawEthernetDevice, RxToken, TxToken};

    pub struct SmolDeviceWrapper<D>
    where
        D: for<'a> smoltcp::phy::Device<'a>,
    {
        device: D,
    }

    impl<D> From<D> for SmolDeviceWrapper<D>
    where
        D: for<'a> smoltcp::phy::Device<'a>,
    {
        fn from(device: D) -> Self {
            Self { device }
        }
    }

    pub struct SmolTxTokenWrapper<T: smoltcp::phy::TxToken>(T);

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

    pub struct SmolRxTokenWrapper<T: smoltcp::phy::RxToken>(T);

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

    impl<D> RawEthernetDevice for SmolDeviceWrapper<D>
    where
        D: for<'d> smoltcp::phy::Device<'d>,
    {
        type TxToken<'a> = SmolTxTokenWrapper<<D as smoltcp::phy::Device<'a>>::TxToken> where Self: 'a;
        type RxToken<'a> = SmolRxTokenWrapper<<D as smoltcp::phy::Device<'a>>::RxToken> where Self: 'a;
        fn transmit<'a>(&'a mut self) -> Option<Self::TxToken<'a>> {
            self.device.transmit().map(SmolTxTokenWrapper)
        }

        fn receive<'a>(&'a mut self) -> Option<Self::RxToken<'a>> {
            self.device
                .receive()
                .map(|(token, _)| SmolRxTokenWrapper(token))
        }
    }
}
