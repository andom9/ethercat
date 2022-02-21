use core::panic;

use ethercat_master::arch::*;
use ethercat_master::master::*;
use ethercat_master::slave_device::*;
use ethercat_master::{AlState, Error};
use pnet::datalink::{self, Channel::Ethernet, DataLinkReceiver, DataLinkSender, NetworkInterface};
use std::env;

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
    let raw_socket = RawSocket::open(interf_name);
    let mut master = EtherCATMaster::<_, Clock>::new(raw_socket);
    master.init_slaves().unwrap();
    dbg!("{:?}", master.slaves());

    for index in 0..master.slave_count() as usize {
        let product_code = master.read_sdo(index as u16, 0x1018, 2).unwrap();
        println!("{:?}", product_code);
        let name = master.read_sdo(index as u16, 0x6041, 0).unwrap();
        dbg!(name);

        //RxPDOの登録
        //master
        //.slave_mut(index)
        //.unwrap()
        //.push_rx_pd0_entry(PDOEntry::new(0, 8).unwrap())
        //.unwrap();
        //master
        //    .slave_mut(index)
        //    .unwrap()
        //    .push_rx_pd0_entry(PDOEntry::new(0x6040, 16).unwrap())
        //    .unwrap();
        //*master
        //    .slave_mut(index)
        //    .unwrap()
        //    .rx_pd0_entry_mut(1)
        //    .unwrap()
        //    .data_mut() = ControlWord::ShutDown.as_le_bytes();

        master
            .slave_mut(index)
            .unwrap()
            .push_rx_pd0_entry(PDOEntry::new(0x6060, 8).unwrap())
            .unwrap();
        *master
            .slave_mut(index)
            .unwrap()
            .rx_pd0_entry_mut(0)
            .unwrap()
            .data_mut() = [8, 0, 0, 0];

        //TxPDOの登録
        //master
        //.slave_mut(index)
        //.unwrap()
        //.push_tx_pd0_entry(PDOEntry::new(0, 32).unwrap())
        //.unwrap();
        //master
        //    .slave_mut(index)
        //    .unwrap()
        //    .push_tx_pd0_entry(PDOEntry::new(0x6040, 16).unwrap())
        //    .unwrap();
        master
            .slave_mut(index)
            .unwrap()
            .push_tx_pd0_entry(PDOEntry::new(0x6060, 8).unwrap())
            .unwrap();

        //軌跡制御モードにする
        //master.write_sdo(index as u16, 0x6060, 0, &[0x08]).unwrap();
        //let op = master.read_sdo(index as u16, 0x6060, 0).unwrap();
        //dbg!(op);
        //master.write_sdo(index as u16, 0x6040, 0, &ControlWord::ShutDown.as_le_bytes()).unwrap();
        //dbg!(ControlWord::ShutDown.as_le_bytes());
    }

    let cycle_time = 2_000_000;
    //マスターの起動
    let sync0_pulse_start_time = master.start_safe_operation(cycle_time).unwrap();
    master.change_al_states(AlState::Operational).unwrap();

    let mut max_sleep = 0;
    let mut min_sleep = std::u128::MAX;
    let mut now_time = 0;
    let mut time_old = 0;
    let mut max_interval = 0;
    for i in 0..10000 {
        let time0 = Clock::system_time_from_2000_1_1_as_nanos();
        now_time = match master.send_pdo(1_000_000) {
            Ok(new_now_time) => new_now_time,
            Err(e) => {
                println!("{:?}", e);
                if let Error::WkcNeq(_, _) = e {
                    panic!();
                }
                now_time
            }
        };
        if i % 2 == 0 {
            *master
                .slave_mut(0)
                .unwrap()
                .rx_pd0_entry_mut(0)
                .unwrap()
                .data_mut() = [8, 0, 0, 0];
        } else {
            *master
                .slave_mut(0)
                .unwrap()
                .rx_pd0_entry_mut(0)
                .unwrap()
                .data_mut() = [0, 0, 0, 0];
        }

        let now = std::time::Instant::now();
        let tmp = (now_time - sync0_pulse_start_time) % (cycle_time as u64);
        let sleeptime = tmp.max(cycle_time as u64 - tmp);
        let sleeptime = sleeptime as u128;

        println!("interval {:0.4}ms", (time0 - time_old) as f64 / 1_000_000.0);
        if i > 0 {
            max_sleep = max_sleep.max(sleeptime);
            min_sleep = min_sleep.min(sleeptime);
            max_interval = max_interval.max(time0 - time_old);
        }

        for index in 0..master.slave_count() {
            dbg!(index);
            let op = master.read_sdo(index, 0x6060, 0).unwrap();
            dbg!(op);
        }

        while now.elapsed().as_nanos() < (sleeptime) {}
        time_old = time0;
    }
    println!("max_sleep {:0.4}", max_sleep as f64 / 1_000_000.0);
    println!("min_sleep {:0.4}", min_sleep as f64 / 1_000_000.0);
    println!("jitter {:0.4}ms", max_interval as f64 / 1_000_000.0);
}

struct Clock {}

impl EtherCATSystemTime for Clock {
    fn system_time_from_2000_1_1_as_nanos() -> u64 {
        let systemtime = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap();
        (systemtime.as_nanos() - 946_684_800_000_000_000) as u64
    }
}

struct RawSocket {
    tx: Box<dyn DataLinkSender + 'static>,
    rx: Box<dyn DataLinkReceiver + 'static>,
}

impl RawSocket {
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
        Self { tx, rx }
    }
}

impl RawPacketInterface for RawSocket {
    fn send(&mut self, packet: &[u8]) -> bool {
        self.tx
            .send_to(packet, None)
            .map(|res| res.is_ok())
            .unwrap_or(false)
    }
    fn recv(&mut self, rx_buffer: &mut [u8]) -> Option<usize> {
        match self.rx.next() {
            Err(_e) => None,
            Ok(packet) => {
                let mut len = 0;
                for (buf, recv) in rx_buffer.iter_mut().zip(packet.into_iter()) {
                    *buf = *recv;
                    len += 1;
                }
                Some(len)
            }
        }
    }
}
