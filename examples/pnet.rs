use ethercat_master::arch::*;
use ethercat_master::cyclic;
use ethercat_master::cyclic::mailbox_reader;
use ethercat_master::cyclic::mailbox_reader::MailboxReader;
use ethercat_master::cyclic::sdo_downloader;
use ethercat_master::cyclic::sdo_downloader::SdoDownloader;
use ethercat_master::cyclic::sdo_uploader;
use ethercat_master::cyclic::sdo_uploader::SdoUploader;
use ethercat_master::cyclic::sii_reader;
use ethercat_master::cyclic::CyclicUnits;
use ethercat_master::cyclic::EtherCatSystemTime;
use ethercat_master::interface::*;
use ethercat_master::master::EtherCatMaster;
//use ethercat_master::master::*;
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
    let mut buf = vec![0; mtu];
    let mut sdo_up_send_buf = vec![0; 1024];
    let mut sdo_up_recv_buf = vec![0; 1024];
    let mut sdo_down_send_buf = vec![0; 1024];
    let mut sdo_down_recv_buf = vec![0; 1024];

    let iface = EtherCatInterface::new(device, timer, &mut buf);
    let mut slave_buf: [Option<Slave>; 10] = Default::default();

    dbg!("crate masetr");
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

    //let mut unit = SdoUploader::new(&mut sdo_send_buf, &mut sdo_recv_buf);
    //unit.start(SlaveAddress::SlavePosition(0), 0x1000, 0x00);
    //let handle = master.add_sdo_uploader(unit).unwrap();
    //let mut count = 0;
    //loop {
    //    master
    //        .poll(EtherCatSystemTime(count), Duration::from_millis(1000))
    //        .unwrap();
    //    let sdo_uploader = master.get_sdo_uploader(&handle).unwrap();
    //    match sdo_uploader.wait() {
    //        Some(Ok(_)) => {
    //            break;
    //        }
    //        None => {}
    //        Some(Err(other)) => panic!("sdo error {:?}", other), //return Err(other.into()),
    //    }
    //    count += 1000;
    //}
    //let sdo_uploader = master.remove_sdo_uploader(handle).unwrap();
    //let sdo_data = sdo_uploader.sdo_data();
    //println!("sdo data: {:?}", sdo_data);

    let sdo_uploader = SdoUploader::new(&mut sdo_up_send_buf, &mut sdo_up_recv_buf);
    let sdo_uploader_handle = master.add_sdo_uploader(sdo_uploader).unwrap();
    master
        .read_sdo(
            &sdo_uploader_handle,
            SlaveAddress::SlavePosition(0),
            0x6072,
            0x0,
        )
        .unwrap();
    let sdo_uploader = master.get_sdo_uploader(&sdo_uploader_handle).unwrap();
    println!("sdo data: {:x?}", sdo_uploader.sdo_data());

    let sdo_downloader = SdoDownloader::new(&mut sdo_down_send_buf, &mut sdo_down_recv_buf);
    let sdo_downloader_handle = master.add_sdo_downloader(sdo_downloader).unwrap();
    master
        .write_sdo(
            &sdo_downloader_handle,
            SlaveAddress::SlavePosition(0),
            0x6072,
            0x0,
            &0x1388_u16.to_le_bytes(),
        )
        .unwrap();

    master
        .read_sdo(
            &sdo_uploader_handle,
            SlaveAddress::SlavePosition(0),
            0x6072,
            0x0,
        )
        .unwrap();
    let sdo_uploader = master.get_sdo_uploader(&sdo_uploader_handle).unwrap();
    let data = sdo_uploader.sdo_data();
    println!("sdo data: 0x{:x?}", u16::from_le_bytes([data[0], data[1]]));
}
