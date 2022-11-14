use ethercat_master::frame::CommandType;
use ethercat_master::interface::pcap_device::PcapDevice;
use ethercat_master::interface::*;
use ethercat_master::register::od::cia402::*;
use ethercat_master::register::sii::ProductCode;
use ethercat_master::slave::AlState;
use ethercat_master::slave::PdoEntry;
use ethercat_master::slave::PdoMapping;
use ethercat_master::slave::SlaveConfig;
use ethercat_master::slave::SyncMode;
use ethercat_master::EtherCatMaster;
use pcap::Device;
use std::env;
use std::time::Instant;

fn main() {
    env::set_var("RUST_LOG", "warn");
    env_logger::init();
    let args: Vec<String> = env::args().collect();

    if let Some(name) = args.get(1) {
        pdu_test(&name);
        read_eeprom_test(name);
        sdo_test(name);
        //pdo_test(name);
    } else {
        println!("Specify the name of network interface as an argument from the following.");
        for (i, device) in Device::list().unwrap().iter().enumerate() {
            println!("{}:", i);
            println!("\tname: {}", device.name);
            println!("\tdesc: {}", device.desc.as_ref().unwrap());
        }
    }
}

fn new_device(name: &str) -> PcapDevice {
    let device = Device::list()
        .unwrap()
        .into_iter()
        .find(|d| &d.name == name)
        .unwrap();
    PcapDevice::new(device).unwrap()
}

fn pdu_test(name: &str) {
    println!("\npdu_test");
    let dev = new_device(name);
    let mut buf = [0; 1500];
    let iface = PduInterface::new(dev, &mut buf);
    let mut s_buf = [0; 256];
    let mut sif: SocketInterface<_, 1> = SocketInterface::new(iface);
    let handle = sif.add_socket(PduSocket::new(&mut s_buf)).unwrap();
    let socket = sif.get_socket_mut(&handle).unwrap();

    let command = Command::new(CommandType::BRD, 0, 0);
    let data_length = 1;

    socket.set_pdu(|_buf| Some((command, data_length)));
    loop {
        if let Ok(ok) = sif.poll_tx_rx() {
            if ok {
                break;
            }
        }
    }
    let socket = sif.get_socket_mut(&handle).unwrap();
    let recv_pdu = socket.get_recieved_pdu().unwrap();
    assert_eq!(command.c_type, recv_pdu.command.c_type);
    assert_eq!(recv_pdu.command.adp, recv_pdu.wkc);
    dbg!(recv_pdu);
    println!("pdu_test done");
}

fn read_eeprom_test(name: &str) {
    println!("\nread_eeprom_test");
    let dev = new_device(name);
    let mut buf = [0; 1500];
    let iface = PduInterface::new(dev, &mut buf);
    let mut s_buf = [0; 256];
    let mut sif: SocketInterface<_, 1> = SocketInterface::new(iface);
    let handle = sif.add_socket(PduSocket::new(&mut s_buf)).unwrap();
    let (data, size) = sif
        .read_sii(
            &handle,
            SlaveAddress::SlavePosition(0),
            ProductCode::ADDRESS,
        )
        .unwrap();
    println!("ProductCode: {}", data.data(size));
    println!("read_eeprom_test done");
}

fn sdo_test(name: &str) {
    println!("\nsdo_test");
    let dev = new_device(name);
    let mut buf = [0; 1500];
    let iface = PduInterface::new(dev, &mut buf);

    let mut slaves: [_; 10] = Default::default();
    let mut pdu_buffer = vec![0; 1500];
    let mut master = EtherCatMaster::new(&mut slaves, &mut pdu_buffer, iface);
    println!("initializing slaves");
    master.init().unwrap();

    let num_slaves = master.network().num_slaves();
    println!("number of slaves: {}", num_slaves);

    println!("changing al states");
    let al_state = master
        .change_al_state(TargetSlave::All(num_slaves), AlState::PreOperational)
        .unwrap();
    println!("al state: {:?}", al_state);

    println!("reading vender id");
    let vender_id = master.read_sdo(SlaveAddress::SlavePosition(0), 0x1018, 0x01).unwrap();
    println!("vender id: {:?}", vender_id);
    
    println!("sdo_test done");
}

// fn pdo_test(interf_name: &str) {
//     dbg!("prepare resources");
//     let device = PnetDevice::open(interf_name);
//     let mut buf = vec![0; 1500];
//     let iface = CommandInterface::new(device, &mut buf);

//     let mut output_pdo_map = [PdoMapping {
//         is_fixed: false,
//         index: 0x1702,
//         entries: &mut [
//             PdoEntry::new(0x6040, 0x00, 16), // control word
//             PdoEntry::new(0x607A, 0x00, 32), // target position
//             PdoEntry::new(0x200D, 0x01, 16), // misc
//                                              //PdoEntry::new(0x2005, 0x01, 16),
//                                              //PdoEntry::new(0x6300, 1, 16),
//         ],
//     }];

//     let mut input_pdo_map = [PdoMapping {
//         is_fixed: false,
//         index: 0x1B03,
//         entries: &mut [
//             PdoEntry::new(0x603F, 0x00, 16), // error code
//             PdoEntry::new(0x6041, 0x00, 16), // status word
//             PdoEntry::new(0x6064, 0x00, 32), // actual position
//             PdoEntry::new(0x6077, 0x00, 16), // actual torque
//             PdoEntry::new(0x60F4, 0x00, 32), // position error
//             PdoEntry::new(0x200D, 0x01, 16), // misc
//                                              //PdoEntry::new(0x2005, 0x01, 16),
//                                              //PdoEntry::new(0x6100, 1, 16),
//         ],
//     }];

//     let mut slaves: Box<[(_, SlaveConfig); 10]> = Box::new(Default::default());
//     slaves[0].1.set_tx_pdo_mappings(&mut input_pdo_map);
//     slaves[0].1.set_rx_pdo_mappings(&mut output_pdo_map);
//     slaves[0].1.operation_mode = SyncMode::FreeRun;
//     let mut socket_buffer = vec![0; 1500];
//     let mut pdo_buffer = vec![0; 1500];
//     let mut master = EtherCatMaster::new(slaves.as_mut(), &mut socket_buffer, iface);
//     dbg!("init");
//     master.initilize_slaves().unwrap();
//     dbg!("init end");
//     let num_slaves = master.network().num_slaves();
//     dbg!(num_slaves);
//     for (slave, config) in master.network().slaves() {
//         dbg!(&slave.info().id);
//         dbg!(&config);
//     }
//     master
//         .change_al_state(TargetSlave::All(num_slaves), AlState::PreOperational)
//         .unwrap();
//     //let t_lim = master.read_sdo(SlaveAddress::SlavePosition(0), 0x6072, 0).unwrap();
//     //dbg!(t_lim);

//     master
//         .write_sdo(
//             SlaveAddress::SlavePosition(0),
//             0x6072,
//             0,
//             &40_u16.to_le_bytes(),
//         )
//         .unwrap();
//     //let t_lim = master.read_sdo(SlaveAddress::SlavePosition(0), 0x6072, 0).unwrap();
//     //dbg!(t_lim);
//     //panic!();
//     master
//         .write_sdo(
//             SlaveAddress::SlavePosition(0),
//             0x20F8,
//             7,
//             &0x1_u8.to_le_bytes(),
//         )
//         .unwrap();
//     //master.synchronize_dc().unwrap();
//     master.configure_slave_settings(&mut pdo_buffer).unwrap();

//     //panic!();
//     let ret = master
//         .change_al_state(TargetSlave::All(num_slaves), AlState::SafeOperational)
//         .unwrap();
//     let ret = master
//         .change_al_state(TargetSlave::All(num_slaves), AlState::Operational)
//         .unwrap();
//     dbg!(ret);

//     // master
//     //     .write_pdo_u16(
//     //         SlaveAddress::SlavePosition(0),
//     //         0,
//     //         0,
//     //         u16::from_le_bytes(ControlWord::new_switch_on_and_enable_operation().0),
//     //     )
//     //     .unwrap();

//     let mut pre_cycle_count = 0;
//     let instance = Instant::now();
//     let mut count = 0;
//     let mut count2 = 0;
//     for i in 0..100 {
//         loop {
//             count += 1;
//             let cycle_count = master.process_one_cycle(instance.elapsed().into()).unwrap();
//             if pre_cycle_count != cycle_count {
//                 pre_cycle_count = cycle_count;
//                 break;
//             }
//         }
//         let error_code = master
//             .read_pdo_u16(SlaveAddress::SlavePosition(0), 0, 0)
//             .unwrap();
//         let status_word = master
//             .read_pdo_u16(SlaveAddress::SlavePosition(0), 0, 1)
//             .unwrap();
//         let actual_position = master
//             .read_pdo_u32(SlaveAddress::SlavePosition(0), 0, 2)
//             .unwrap();
//         let actual_torque = master
//             .read_pdo_u16(SlaveAddress::SlavePosition(0), 0, 3)
//             .unwrap();
//         let position_error = master
//             .read_pdo_u32(SlaveAddress::SlavePosition(0), 0, 4)
//             .unwrap();
//         let misc = master
//             .read_pdo_u16(SlaveAddress::SlavePosition(0), 0, 5)
//             .unwrap();

//         let status_word = StatusWord(status_word.to_le_bytes());
//         if !status_word.switched_on() {
//             dbg!(status_word.nquick_stop());
//             dbg!(status_word.internal_limit_active());
//             dbg!(status_word.fault());
//             dbg!(status_word.ready_to_switch_on());
//             dbg!(status_word.operation_enabled());
//             dbg!(status_word.ready_to_switch_on());
//             dbg!(status_word.switch_on_disabled());
//             dbg!(status_word.switched_on());
//             dbg!(status_word.voltage_enabled());
//             let mut c_word = ControlWord::new_switch_on_and_enable_operation();
//             if !status_word.nquick_stop() {
//                 c_word = ControlWord::new();
//                 c_word.0[0] = 0b0000_0110;
//             }
//             master
//                 .write_pdo_u16(
//                     SlaveAddress::SlavePosition(0),
//                     0,
//                     0,
//                     u16::from_le_bytes(c_word.0),
//                 )
//                 .unwrap();
//         } else {
//             dbg!(error_code);
//             dbg!(status_word);
//             dbg!(actual_position);
//             dbg!(actual_torque);
//             dbg!(position_error);
//             dbg!(misc);
//             master
//                 .write_pdo_u32(SlaveAddress::SlavePosition(0), 0, 1, count2 * 500)
//                 .unwrap();
//             count2 += 1;
//         }
//     }
//     dbg!(count);
//     let t_lim = master
//         .read_sdo(SlaveAddress::SlavePosition(0), 0x6072, 0)
//         .unwrap();
//     dbg!(t_lim);
// }
