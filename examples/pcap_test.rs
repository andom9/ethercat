use ethercat_master::frame::CommandType;
use ethercat_master::interface::pcap_device::PcapDevice;
use ethercat_master::interface::*;
use ethercat_master::register::od::cia402::*;
use ethercat_master::register::sii::ProductCode;
use ethercat_master::register::AlControl;
use ethercat_master::register::DcSystemTime;
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
        //pdu_test(name);
        //read_eeprom_test(name);
        //sdo_test(name);
        //dc_test(name);
        pdo_test(name);
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
    let vender_id = master
        .read_sdo_as_u32(SlaveAddress::SlavePosition(0), 0x1018, 0x01)
        .unwrap();
    println!("vender id: {:x}", vender_id);

    println!("reading gain");
    let gain = master
        .read_sdo_as_u16(SlaveAddress::SlavePosition(0), 0x2005, 0x01)
        .unwrap();
    println!("gain: {:x}", gain);

    println!("writing gain");
    master
        .write_sdo_as_u16(SlaveAddress::SlavePosition(0), 0x2005, 0x01, gain + 1)
        .unwrap();
    let gain = master
        .read_sdo_as_u16(SlaveAddress::SlavePosition(0), 0x2005, 0x01)
        .unwrap();
    println!("gain: {:x}", gain);
    master
        .write_sdo_as_u16(SlaveAddress::SlavePosition(0), 0x2005, 0x01, gain - 1)
        .unwrap();

    println!("sdo_test done");
}

fn dc_test(name: &str) {
    println!("\ndc_test");
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

    let data = master
        .read_register(SlaveAddress::SlavePosition(2).into(), 0x92C, 4)
        .unwrap();
    println!(
        "{:?}",
        u32::from_le_bytes([data[0], data[1], data[2], data[3]]) & 0x7FFF_FFFF
    );

    master.init_dc().unwrap();

    let data = master
        .read_register(SlaveAddress::SlavePosition(0).into(), 0x92C, 4)
        .unwrap();
    println!(
        "{:?}",
        u32::from_le_bytes([data[0], data[1], data[2], data[3]]) & 0x7FFF_FFFF
    );

    let data = master
        .read_register(SlaveAddress::SlavePosition(1).into(), 0x92C, 4)
        .unwrap();
    println!(
        "{:?}",
        u32::from_le_bytes([data[0], data[1], data[2], data[3]]) & 0x7FFF_FFFF
    );

    let data = master
        .read_register(SlaveAddress::SlavePosition(2).into(), 0x92C, 4)
        .unwrap();
    println!(
        "{:?}",
        u32::from_le_bytes([data[0], data[1], data[2], data[3]]) & 0x7FFF_FFFF
    );

    println!("dc_test done");
}

fn pdo_test(name: &str) {
    println!("\npdo_test");
    let dev = new_device(name);
    let mut buf = [0; 1500];
    let iface = PduInterface::new(dev, &mut buf);

    let mut output_pdo_map0 = [PdoMapping {
        is_fixed: false,
        index: 0x1702,
        entries: &mut [
            PdoEntry::new(0x6040, 0x00, 16), // control word
            PdoEntry::new(0x607A, 0x00, 32), // target position
        ],
    }];

    let mut input_pdo_map0 = [PdoMapping {
        is_fixed: false,
        index: 0x1B03,
        entries: &mut [
            PdoEntry::new(0x603F, 0x00, 16), // error code
            PdoEntry::new(0x6041, 0x00, 16), // status word
            PdoEntry::new(0x6064, 0x00, 32), // actual position
            PdoEntry::new(0x6077, 0x00, 16), // actual torque
            PdoEntry::new(0x60F4, 0x00, 32), // position error
        ],
    }];

    let mut output_pdo_map1 = [PdoMapping {
        is_fixed: false,
        index: 0x1702,
        entries: &mut [
            PdoEntry::new(0x6040, 0x00, 16), // control word
            PdoEntry::new(0x607A, 0x00, 32), // target position
        ],
    }];

    let mut input_pdo_map1 = [PdoMapping {
        is_fixed: false,
        index: 0x1B03,
        entries: &mut [
            PdoEntry::new(0x603F, 0x00, 16), // error code
            PdoEntry::new(0x6041, 0x00, 16), // status word
            PdoEntry::new(0x6064, 0x00, 32), // actual position
            PdoEntry::new(0x6077, 0x00, 16), // actual torque
            PdoEntry::new(0x60F4, 0x00, 32), // position error
        ],
    }];

    let mut output_pdo_map2 = [PdoMapping {
        is_fixed: false,
        index: 0x1702,
        entries: &mut [
            PdoEntry::new(0x6040, 0x00, 16), // control word
            PdoEntry::new(0x607A, 0x00, 32), // target position
        ],
    }];

    let mut input_pdo_map2 = [PdoMapping {
        is_fixed: false,
        index: 0x1B03,
        entries: &mut [
            PdoEntry::new(0x603F, 0x00, 16), // error code
            PdoEntry::new(0x6041, 0x00, 16), // status word
            PdoEntry::new(0x6064, 0x00, 32), // actual position
            PdoEntry::new(0x6077, 0x00, 16), // actual torque
            PdoEntry::new(0x60F4, 0x00, 32), // position error
        ],
    }];

    let mut slaves: Box<[(_, SlaveConfig); 10]> = Box::new(Default::default());
    slaves[0]
        .1
        .set_input_process_data_mappings(&mut input_pdo_map0);
    slaves[0]
        .1
        .set_output_process_data_mappings(&mut output_pdo_map0);
    slaves[0].1.sync_mode = SyncMode::Sync0Event;
    slaves[0].1.cycle_time_ns = 16_000_000.into();

    // slaves[1]
    //     .1
    //     .set_input_process_data_mappings(&mut input_pdo_map1);
    // slaves[1]
    //     .1
    //     .set_output_process_data_mappings(&mut output_pdo_map1);
    // slaves[1].1.sync_mode = SyncMode::FreeRun;
    // slaves[1].1.cycle_time_ns = 500_000;

    // slaves[2]
    //     .1
    //     .set_input_process_data_mappings(&mut input_pdo_map2);
    // slaves[2]
    //     .1
    //     .set_output_process_data_mappings(&mut output_pdo_map2);
    // slaves[2].1.sync_mode = SyncMode::FreeRun;
    // slaves[2].1.cycle_time_ns = 500_000;

    let mut socket_buffer = vec![0; 1500];
    let mut pdo_buffer = vec![0; 1500];
    let mut master = EtherCatMaster::new(slaves.as_mut(), &mut socket_buffer, iface);
    master.register_process_data_buffer(&mut pdo_buffer);
    dbg!("initilizeing");
    master.init().unwrap();
    dbg!("init end");
    let num_slaves = master.network().num_slaves();
    dbg!(num_slaves);
    for (slave, config) in master.network().slaves() {
        dbg!(&slave.info().id());
        dbg!(&config);
    }

    master.init_dc().unwrap();

    master
        .change_al_state(TargetSlave::All(num_slaves), AlState::PreOperational)
        .unwrap();

    master.configure_slaves_for_operation().unwrap();

    let ret = master
        .change_al_state(TargetSlave::All(num_slaves), AlState::SafeOperational)
        .unwrap();
    dbg!(ret);

    let c_word = ControlWord::new_switch_on_and_enable_operation();
    master
        .write_pdo_as_u16(
            SlaveAddress::SlavePosition(0),
            0,
            0,
            u16::from_le_bytes(c_word.0),
        )
        .unwrap();

    let mut pre_cycle_count = 0;
    let instance = Instant::now();
    let mut count = 0;
    let mut count2 = 0;
    loop {
        loop {
            count += 1;
            if let Ok(cycle_count) = master.process(instance.elapsed().into()) {
                if pre_cycle_count != cycle_count {
                    pre_cycle_count = cycle_count;
                    break;
                }
            }
        }
        master.request_al_state(AlState::Operational);
        std::thread::sleep(std::time::Duration::from_micros(16000));

        let err_count = master
            .read_sdo_as_u16(SlaveAddress::SlavePosition(0), 0x1C32, 0x0C)
            .unwrap();
        println!("err_count: {:x}", err_count);
        if let (Some(AlState::Operational), _) = master.al_state() {
            break;
        }
    }

    for i in 0..100 {
        loop {
            count += 1;
            if let Ok(cycle_count) = master.process(instance.elapsed().into()) {
                if pre_cycle_count != cycle_count {
                    pre_cycle_count = cycle_count;
                    break;
                }
            }
        }

        if let (Some(AlState::Operational), _) = master.al_state() {
            let error_code = master
                .read_pdo_as_u16(SlaveAddress::SlavePosition(0), 0, 0)
                .unwrap();
            let status_word = master
                .read_pdo_as_u16(SlaveAddress::SlavePosition(0), 0, 1)
                .unwrap();
            let actual_position = master
                .read_pdo_as_u32(SlaveAddress::SlavePosition(0), 0, 2)
                .unwrap();
            let actual_torque = master
                .read_pdo_as_u16(SlaveAddress::SlavePosition(0), 0, 3)
                .unwrap();
            let position_error = master
                .read_pdo_as_u32(SlaveAddress::SlavePosition(0), 0, 4)
                .unwrap();

            let status_word = StatusWord(status_word.to_le_bytes());
            if !status_word.switched_on() {
                dbg!(status_word.nquick_stop());
                // dbg!(status_word.internal_limit_active());
                // dbg!(status_word.fault());
                // dbg!(status_word.ready_to_switch_on());
                // dbg!(status_word.operation_enabled());
                // dbg!(status_word.ready_to_switch_on());
                // dbg!(status_word.switch_on_disabled());
                // dbg!(status_word.switched_on());
                // dbg!(status_word.voltage_enabled());
                let mut c_word = ControlWord::new_switch_on_and_enable_operation();
                if !status_word.nquick_stop() {
                    c_word.0[1] = 0;
                    c_word.0[0] = 0b0000_0110;
                }
                master
                    .write_pdo_as_u16(
                        SlaveAddress::SlavePosition(0),
                        0,
                        0,
                        u16::from_le_bytes(c_word.0),
                    )
                    .unwrap();
            } else {
                //dbg!(error_code);
                //dbg!(status_word);
                //dbg!(actual_position);
                //dbg!(actual_torque);
                //dbg!(position_error);
                master
                    .write_pdo_as_u32(SlaveAddress::SlavePosition(0), 0, 1, count2 * 500)
                    .unwrap();
                count2 += 1;
                dbg!(count2);
                let err_count = master
                .read_sdo_as_u16(SlaveAddress::SlavePosition(0), 0x1C32, 0x0C)
                .unwrap();
                println!("err_count: {:x}", err_count);
            }
        }else{
            dbg!(master.al_state());
            panic!()
        }
        std::thread::sleep(std::time::Duration::from_micros(100));
    }
    dbg!(count);
    let ret = master
        .change_al_state(TargetSlave::All(num_slaves), AlState::Init)
        .unwrap();
}
