use ethercat_master::arch::*;
use ethercat_master::cyclic::sdo::SdoUnit;
use ethercat_master::cyclic::sii_reader;
use ethercat_master::interface::*;
use ethercat_master::master::EtherCatMaster;
use ethercat_master::slave::AlState;
use ethercat_master::slave::Slave;
use pnet::datalink::{self, Channel::Ethernet, DataLinkReceiver, DataLinkSender, NetworkInterface};
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
    let device = PnetDevice::open(&interf_name);
    let mtu = device.max_transmission_unit();
    let mut command_buf = vec![0; mtu];
    let mut sdo_buf = vec![0; 1024];

    let iface = EtherCatInterface::new(device, timer, &mut command_buf);
    let mut slave_buf: [Option<Slave>; 10] = Default::default();

    dbg!("crate master");
    let mut master = EtherCatMaster::initilize(iface, &mut slave_buf).unwrap();
    dbg!("done");

    let (eeprom_data, size) = master
        .read_sii(
            SlaveAddress::SlavePosition(0),
            sii_reader::sii_reg::ProductCode::ADDRESS,
        )
        .unwrap();
    println!("product code: {:x}", eeprom_data.sii_data());
    println!("read_size: {}", size);

    let alstate = master
        .transfer_al_state(None, AlState::PreOperational)
        .unwrap();
    println!("al_state: {:?}", alstate);

    let alstate = master.read_al_state(None).unwrap();
    println!("al_state: {:?}", alstate);

    let sdo_unit = SdoUnit::new(&mut sdo_buf);
    let sdo_unit_handle = master.add_sdo_unit(sdo_unit).unwrap();
    master
        .read_sdo(
            &sdo_unit_handle,
            SlaveAddress::SlavePosition(0),
            0x6072,
            0x0,
        )
        .unwrap();
    let sdo_unit = master.get_sdo_unit(&sdo_unit_handle).unwrap();
    println!("sdo data: {:x?}", sdo_unit.sdo_data());

    master
        .write_sdo(
            &sdo_unit_handle,
            SlaveAddress::SlavePosition(0),
            0x6072,
            0x0,
            &0x1388_u16.to_le_bytes(),
        )
        .unwrap();

    master
        .read_sdo(
            &sdo_unit_handle,
            SlaveAddress::SlavePosition(0),
            0x6072,
            0x0,
        )
        .unwrap();
    let sdo_unit = master.get_sdo_unit(&sdo_unit_handle).unwrap();
    let data = sdo_unit.sdo_data();
    println!("sdo data: 0x{:x?}", u16::from_le_bytes([data[0], data[1]]));
}
