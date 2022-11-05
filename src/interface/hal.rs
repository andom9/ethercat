/// A smoltcp-like raw network interface.
pub trait RawEthernetDevice {
    type TxToken<'a>: TxToken
    where
        Self: 'a;
    type RxToken<'a>: RxToken
    where
        Self: 'a;

    fn transmit<'a>(&'a mut self) -> Option<Self::TxToken<'a>>;

    fn receive<'a>(&'a mut self) -> Option<Self::RxToken<'a>>;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeviceError {
    Device,
    Function,
}

pub trait TxToken {
    fn consume<F>(self, len: usize, f: F) -> Result<(), DeviceError>
    where
        F: FnOnce(&mut [u8]) -> Result<(), ()>;
}

pub trait RxToken {
    fn consume<F>(self, f: F) -> Result<(), DeviceError>
    where
        F: FnOnce(&[u8]) -> Result<(), ()>;
}

#[cfg(feature = "smoltcp")]
pub mod smoltcp {
    use super::{DeviceError, RawEthernetDevice, RxToken, TxToken};

    pub struct SmolDevice<D>
    where
        D: for<'a> smoltcp::phy::Device<'a>,
    {
        device: D,
    }

    impl<D> From<D> for SmolDevice<D>
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
        fn consume<F>(self, len: usize, f: F) -> Result<(), DeviceError>
        where
            F: FnOnce(&mut [u8]) -> Result<(), ()>,
        {
            self.0
                .consume(smoltcp::time::Instant::from_secs(0), len, |buf| {
                    f(buf).map_err(|_| smoltcp::Error::Illegal)
                })
                .map_err(|_| DeviceError::Device)
        }
    }

    pub struct SmolRxTokenWrapper<T: smoltcp::phy::RxToken>(T);

    impl<T> RxToken for SmolRxTokenWrapper<T>
    where
        T: smoltcp::phy::RxToken,
    {
        fn consume<F>(self, f: F) -> Result<(), DeviceError>
        where
            F: FnOnce(&[u8]) -> Result<(), ()>,
        {
            self.0
                .consume(smoltcp::time::Instant::from_secs(0), |buf| {
                    f(buf).map_err(|_| smoltcp::Error::Illegal)
                })
                .map_err(|_| DeviceError::Device)
        }
    }

    impl<D> RawEthernetDevice for SmolDevice<D>
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

#[cfg(feature = "pcap")]
#[cfg(windows)]
pub mod pcap {
    use pcap::{Active, Capture, Device, Packet};

    use crate::frame::ETHERNET_FRAME_SIZE_WITHOUT_FCS;

    use super::{DeviceError, RawEthernetDevice, RxToken, TxToken};

    pub struct PcapDevice<'a> {
        cap: Capture<Active>,
        buf: &'a mut [u8],
    }

    impl<'a> PcapDevice<'a> {
        pub fn new(timeout_ms: i32, buf: &'a mut [u8]) -> Self {
            assert!(ETHERNET_FRAME_SIZE_WITHOUT_FCS <= buf.len());
            let cap = pcap::Capture::from_device(Device::lookup().unwrap().unwrap())
                .unwrap()
                .promisc(true)
                .immediate_mode(true)
                .timeout(timeout_ms)
                .open()
                .unwrap()
                .setnonblock()
                .unwrap();
            Self { cap, buf }
        }
    }

    impl<'a> RawEthernetDevice for PcapDevice<'a> {
        type TxToken<'b> = PcapTxToken<'b>
    where
        Self: 'b;

        type RxToken<'b> = PcapRxToken<'b>
    where
        Self: 'b;

        fn transmit<'b>(&'b mut self) -> Option<Self::TxToken<'b>> {
            let Self { cap, buf } = self;
            Some(PcapTxToken { cap, buf })
        }

        fn receive<'b>(&'b mut self) -> Option<Self::RxToken<'b>> {
            match self.cap.next_packet() {
                Ok(packet) => Some(PcapRxToken(packet)),
                Err(_) => None,
            }
        }
    }

    pub struct PcapTxToken<'a> {
        cap: &'a mut Capture<Active>,
        buf: &'a mut [u8],
    }

    impl<'a> TxToken for PcapTxToken<'a> {
        fn consume<F>(self, len: usize, f: F) -> Result<(), DeviceError>
        where
            F: FnOnce(&mut [u8]) -> Result<(), ()>,
        {
            f(&mut self.buf[..len]).map_err(|_| DeviceError::Function)?;
            self.cap
                .sendpacket(&mut self.buf[..len])
                .map_err(|_| ())
                .map_err(|_| DeviceError::Device)?;
            Ok(())
        }
    }

    pub struct PcapRxToken<'a>(Packet<'a>);

    impl<'a> RxToken for PcapRxToken<'a> {
        fn consume<F>(self, f: F) -> Result<(), DeviceError>
        where
            F: FnOnce(&[u8]) -> Result<(), ()>,
        {
            let len = self.0.header.caplen as usize;
            f(&self.0.data[..len]).map_err(|_| DeviceError::Function)
        }
    }
}
