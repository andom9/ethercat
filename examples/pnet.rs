use ethercat_master::cyclic_task::{*, tasks::*};
use ethercat_master::hal::*;
use ethercat_master::master::CyclicTaskType;
use ethercat_master::master::EtherCatMaster;
use ethercat_master::slave_network::NetworkDescription;
use ethercat_master::register::sii::ProductCode;
use ethercat_master::slave_network::AlState;
use ethercat_master::slave_network::Slave;
use ethercat_master::slave_network::SyncManager;
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

    fn max_transmission_unit(&self) -> usize {
        1500
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

    let device_size = core::mem::size_of::<PnetDevice>();
    println!("device size {}", device_size);

    let timer_size = core::mem::size_of::<Timer>();
    println!("timer size {}", timer_size);

    let size = core::mem::size_of::<EtherCatMaster<PnetDevice, Timer>>();
    println!("EtherCatMaster size {}", size);

    let size = core::mem::size_of::<CyclicTaskType>();
    println!("CyclicTaskTypes size {}", size);
    let size = core::mem::size_of::<SiiReader>();
    println!("SiiReader size {}", size);
    let size = core::mem::size_of::<NetworkInitializer>();
    println!("NetworkInitializer size {}", size);
    let size = core::mem::size_of::<SlaveInitializer>();
    println!("SlaveInitializer size {}", size);
    let size = core::mem::size_of::<MailboxTask>();
    println!("MailboxTask size {}", size);
    let size = core::mem::size_of::<SdoTask>();
    println!("SdoTask size {}", size);
    let size = core::mem::size_of::<SdoUploader>();
    println!("SdoUploader size {}", size);
    let size = core::mem::size_of::<SdoDownloader>();
    println!("SdoDownloader size {}", size);
    let size = core::mem::size_of::<RamAccessTask>();
    println!("RamAccessTask size {}", size);
    let size = core::mem::size_of::<AlStateTransfer>();
    println!("AlStateTransfer size {}", size);
    let size = core::mem::size_of::<NetworkDescription>();
    println!("net size {}", size);

    let size = core::mem::size_of::<SyncManager>();
    println!("SyncManager size {}", size);
    //panic!();

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
    let mut pdu_buf = vec![0; device.max_transmission_unit()];
    let mut mb_buf = vec![0; 1488];
    let mut tasks_buf: Box<[TaskOption<CyclicTaskType>; 10]> = Default::default();
    let mut slave_buf: Box<[Option<Slave>; 10]> = Default::default();
    let iface = CommandInterface::new(device, timer, &mut pdu_buf);

    dbg!("crate master");
    let mut master =
        EtherCatMaster::initilize(iface, slave_buf.as_mut(), tasks_buf.as_mut()).unwrap();
    dbg!("done");

    let slave_count = master.network().len();

    let sdo_task_handle = master.add_sdo_task(SdoTask::new(&mut mb_buf)).unwrap();

    let (eeprom_data, size) = master
        .read_sii(SlaveAddress::SlavePosition(0), ProductCode::ADDRESS)
        .unwrap();
    println!("product code: {:x}", eeprom_data.sii_data());
    println!("read_size: {}", size);

    let alstate = master
        .transfer_al_state(
            TargetSlave::All(slave_count as u16),
            AlState::PreOperational,
        )
        .unwrap();
    println!("al_state: {:?}", alstate);

    let alstate = master
        .read_al_state(TargetSlave::All(slave_count as u16))
        .unwrap();
    println!("al_state: {:?}", alstate);

    master
        .read_sdo(
            &sdo_task_handle,
            SlaveAddress::SlavePosition(0),
            0x6072,
            0x0,
        )
        .unwrap();
    let sdo_task = master.get_sdo_task(&sdo_task_handle).unwrap();
    println!("sdo data: {:x?}", sdo_task.sdo_data());

    master
        .write_sdo(
            &sdo_task_handle,
            SlaveAddress::SlavePosition(0),
            0x6072,
            0x0,
            &0x1388_u16.to_le_bytes(),
        )
        .unwrap();

    master
        .read_sdo(
            &sdo_task_handle,
            SlaveAddress::SlavePosition(0),
            0x6072,
            0x0,
        )
        .unwrap();
    let sdo_task = master.get_sdo_task(&sdo_task_handle).unwrap();
    let data = sdo_task.sdo_data();
    println!("sdo data: 0x{:x?}", u16::from_le_bytes([data[0], data[1]]));
}
