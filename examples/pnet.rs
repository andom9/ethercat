
use ethercat_master::hal::*;
use ethercat_master::memory::sii::ProductCode;
use ethercat_master::network::AlState;



use ethercat_master::task::socket::{CommandSocket, SocketOption, SocketsInterface};
use ethercat_master::task::{*};
use ethercat_master::EtherCatMaster;
use pnet_datalink::{self, Channel::Ethernet, DataLinkReceiver, DataLinkSender, NetworkInterface};
use std::env;


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

impl<'a> RawEthernetDevice<'a> for PnetDevice {
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
        self.0.next().ok().map(f).unwrap_or(Err(()))
    }
}

fn main() {
    env::set_var("RUST_LOG", "info");
    env_logger::init();
    let args: Vec<String> = env::args().collect();

    if let Some(name) = args.get(1) {
        read_eeprom_test(name);
        sdo_test(name);
    } else {
        println!("Specify the name of network interface as an argument from the following.");
        for (i, interface) in pnet_datalink::interfaces().iter().enumerate() {
            println!("{}:", i);
            println!("    Description: {}", interface.description);
            println!("    Name: {}", interface.name);
        }
    }
}

fn read_eeprom_test(interf_name: &str) {
    dbg!("prepare resources");
    let device = PnetDevice::open(interf_name);
    let mut buf = vec![0; 1500];
    let iface = CommandInterface::new(device, &mut buf);

    let mut socket_buf = vec![0; 256];
    let sockets = [
        SocketOption::default(), // al state
    ];
    let mut sif = SocketsInterface::new(iface, sockets);
    let handle = sif.add_socket(CommandSocket::new(&mut socket_buf)).unwrap();
    let (data, size) = sif
        .read_sii(
            &handle,
            SlaveAddress::SlavePosition(1),
            ProductCode::ADDRESS,
        )
        .unwrap();
    dbg!(data.data(size));
}

fn sdo_test(interf_name: &str) {
    dbg!("prepare resources");
    let device = PnetDevice::open(interf_name);
    let mut buf = vec![0; 1500];
    let iface = CommandInterface::new(device, &mut buf);

    let mut slaves: [_; 10] = Default::default();
    let mut pdu_buffer = vec![0; 1500];
    let mut master = EtherCatMaster::new(&mut slaves, &mut pdu_buffer, iface);
    master.initilize_slaves().unwrap();
    let num_slaves = master.network().len() as u16;
    master
        .change_al_state(TargetSlave::All(num_slaves), AlState::PreOperational)
        .unwrap();
    let data = master
        .read_sdo(SlaveAddress::SlavePosition(0), 0x2005, 0x01)
        .unwrap();
    dbg!(data);

    let data2 = [data[0] + 1, data[1]];
    master
        .write_sdo(SlaveAddress::SlavePosition(0), 0x2005, 0x01, &data2)
        .unwrap();

    let data = master
        .read_sdo(SlaveAddress::SlavePosition(0), 0x2005, 0x01)
        .unwrap();
    dbg!(data);

    let data2 = [data[0] - 1, data[1]];
    master
        .write_sdo(SlaveAddress::SlavePosition(0), 0x2005, 0x01, &data2)
        .unwrap();
}
