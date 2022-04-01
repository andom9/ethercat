use embedded_hal::timer::CountDown;
use ethercat_master::arch::*;
use ethercat_master::interface::*;
use ethercat_master::packet::*;
use ethercat_master::sii::SlaveInformationInterface;
use fugit::MicrosDurationU32;
use pnet::datalink::{self, Channel::Ethernet, DataLinkReceiver, DataLinkSender, NetworkInterface};
use std::env;
use std::time::Instant;

pub struct Timer(Instant, MicrosDurationU32);

impl Timer {
    fn new() -> Self {
        Timer(Instant::now(), MicrosDurationU32::from_ticks(0))
    }
}

impl CountDown for Timer {
    type Time = MicrosDurationU32;
    fn start<T>(&mut self, count: T)
    where
        T: Into<Self::Time>,
    {
        self.0 = Instant::now();
        self.1 = count.into();
    }

    fn wait(&mut self) -> nb::Result<(), void::Void> {
        if self.0.elapsed() > std::time::Duration::from_micros(self.1.to_micros() as u64) {
            Ok(())
        } else {
            Err(nb::Error::WouldBlock)
        }
    }
}

struct PnetDevice {
    tx_buf: [u8; 1500],
    tx: Box<dyn DataLinkSender + 'static>,
    rx: Box<dyn DataLinkReceiver + 'static>,
}

impl PnetDevice {
    fn open(network_interface_name: &str) -> Self {
        let interface_names_match = |iface: &NetworkInterface| iface.name == network_interface_name;
        let interfaces = datalink::interfaces();
        let interface = interfaces
            .into_iter()
            .find(interface_names_match)
            .expect("interface not found");
        let (tx, rx) = match datalink::channel(&interface, Default::default()) {
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

impl Device for PnetDevice {
    fn send<R, F>(&mut self, len: usize, f: F) -> Option<R>
    where
        F: FnOnce(&mut [u8]) -> Option<R>,
    {
        let b = f(&mut self.tx_buf[..len]);
        if let Some(r) = self.tx.send_to(&self.tx_buf[..len], None) {
            match r {
                Ok(_) => b,
                Err(_) => None,
            }
        } else {
            None
        }
    }

    fn recv<R, F>(&mut self, f: F) -> Option<R>
    where
        F: FnOnce(&[u8]) -> Option<R>,
    {
        self.rx.next().ok().map(|buf| f(buf)).flatten()
    }

    fn max_transmission_unit(&self) -> usize {
        1500
    }
}

fn main() {
    env::set_var("RUST_LOG", "info");
    env_logger::init();

    let args: Vec<String> = env::args().collect();

    if let Some(name) = args.get(1) {
        simple_test(&name);
    } else {
        println!("Specify the name of network interface as an argument from the following.");
        for (i, interface) in datalink::interfaces().iter().enumerate() {
            println!("{}:", i);
            println!("    Description: {}", interface.description);
            println!("    Name: {}", interface.name);
        }
    }
}

fn simple_test(interf_name: &str) {
    let timer = Timer::new();
    let mut buf = [0; 1500];
    let device = PnetDevice::open(&interf_name);

    let mut master = EtherCATInterface::new(device, timer, &mut buf);
    master
        .add_command(CommandType::BRD, 0, 0, 1, |_| ())
        .unwrap();
    master.poll(MicrosDurationU32::from_ticks(1000)).unwrap();
    let pdu = master.consume_command().next().unwrap();
    println!("command type: {:?}", CommandType::new(pdu.command_type()));
    println!("adp: {:?}", pdu.adp());
    println!("ado: {:?}", pdu.ado());
    println!("data: {:?}", pdu.data());
    println!("wkc: {:?}", pdu.wkc());

    let mut sii = SlaveInformationInterface::new(&mut master);
    let (eeprom_data, size) = sii.read(SlaveAddress::SlaveNumber(0), 0x0008).unwrap();
    println!("eeprom: {:x}", eeprom_data.sii_data());
    println!("read_size: {}", size);
}
