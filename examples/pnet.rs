use ethercat_master::cyclic_task::socket::{CommandSocket, SocketOption, SocketsInterface};
use ethercat_master::cyclic_task::{tasks::*, *};
use ethercat_master::hal::*;
use ethercat_master::register::sii::ProductCode;
use ethercat_master::slave_network::AlState;
use ethercat_master::slave_network::NetworkDescription;
use ethercat_master::slave_network::Slave;
use ethercat_master::slave_network::SyncManager;
use ethercat_master::EtherCatMaster;
use pnet_datalink::{self, Channel::Ethernet, DataLinkReceiver, DataLinkSender, NetworkInterface};
use std::env;
use std::time::{Duration, Instant};

pub struct Timer(Instant, Duration);

impl Timer {
    fn new() -> Self {
        Timer(Instant::now(), Duration::default())
    }
}

impl CountDown for Timer {
    fn start<T>(&mut self, count: T)
    where
        T: Into<Duration>,
    {
        self.0 = Instant::now();
        self.1 = count.into();
    }

    fn wait(&mut self) -> nb::Result<(), void::Void> {
        if self.1 < self.0.elapsed() {
            Ok(())
        } else {
            Err(nb::Error::WouldBlock)
        }
    }
}

struct PnetDevice {
    tx_buf: [u8; 1500],
    tx: Box<dyn DataLinkSender>,
    rx: Box<dyn DataLinkReceiver>,
}

impl PnetDevice {
    fn open(network_interface_name: &str) -> Self {
        let interface_names_match = |iface: &NetworkInterface| iface.name == network_interface_name;
        let interfaces = pnet_datalink::interfaces();
        let interface = interfaces
            .into_iter()
            .find(interface_names_match)
            .expect("interface not found");
        let (tx, rx) = match pnet_datalink::channel(&interface, Default::default()) {
            Ok(Ethernet(tx, rx)) => (tx, rx),
            Ok(_) => panic!("unhandled interface"),
            Err(_e) => panic!("unenable to create channel"),
        };
        Self {
            tx_buf: [0; 1500],
            tx,
            rx,
        }
    }
}

impl<'a> Device<'a> for PnetDevice {
    type TxToken = PnetTxToken<'a>;
    type RxToken = PnetRxToken<'a>;
    fn transmit(&'a mut self) -> Option<Self::TxToken> {
        Some(PnetTxToken(&mut self.tx, &mut self.tx_buf))
    }

    fn receive(&'a mut self) -> Option<Self::RxToken> {
        Some(PnetRxToken(&mut self.rx))
    }
}

struct PnetTxToken<'a>(&'a mut Box<dyn DataLinkSender + 'static>, &'a mut [u8]);
impl<'a> TxToken for PnetTxToken<'a> {
    fn consume<F>(self, len: usize, f: F) -> Result<(), ()>
    where
        F: FnOnce(&mut [u8]) -> Result<(), ()>,
    {
        let b = f(&mut self.1[..len]);
        if let Some(r) = self.0.send_to(&self.1[..len], None) {
            match r {
                Ok(_) => b,
                Err(_) => Err(()),
            }
        } else {
            Err(())
        }
    }
}
struct PnetRxToken<'a>(&'a mut Box<dyn DataLinkReceiver>);
impl<'a> RxToken for PnetRxToken<'a> {
    fn consume<F>(self, f: F) -> Result<(), ()>
    where
        F: FnOnce(&[u8]) -> Result<(), ()>,
    {
        self.0.next().ok().map(|b| f(b)).unwrap_or(Err(()))
    }
}

fn main() {
    env::set_var("RUST_LOG", "info");
    env_logger::init();
    let args: Vec<String> = env::args().collect();

    if let Some(name) = args.get(1) {
        simple_test(name);
    } else {
        println!("Specify the name of network interface as an argument from the following.");
        for (i, interface) in pnet_datalink::interfaces().iter().enumerate() {
            println!("{}:", i);
            println!("    Description: {}", interface.description);
            println!("    Name: {}", interface.name);
        }
    }
}

fn simple_test(interf_name: &str) {
    dbg!("prepare resources");
    let timer = Timer::new();
    let device = PnetDevice::open(interf_name);
    let mut buf = vec![0; 1500];
    let iface = CommandInterface::new(device, timer, &mut buf);

    dbg!("crate interface");
    let mut socket_buf1 = vec![0; 256];
    let mut socket_buf2 = vec![0; 256];
    let sockets = [
        SocketOption::default(), // al state
        SocketOption::default(), // rx error
    ];
    let mut sif = SocketsInterface::new(iface, sockets);
    let handle1 = sif
        .add_socket(CommandSocket::new(&mut socket_buf1))
        .unwrap();
    let handle2 = sif
        .add_socket(CommandSocket::new(&mut socket_buf2))
        .unwrap();
    let (data, size) = sif
        .read_sii(
            &handle2,
            SlaveAddress::SlavePosition(1),
            ProductCode::ADDRESS,
        )
        .unwrap();

    dbg!(data);
}
