use crate::arch::Device;
use crate::error::CommonError;
use crate::ethercat_frame::*;
use crate::packet::ethercat::*;
use crate::util::*;
use embedded_hal::timer::CountDown;
use fugit::MicrosDurationU32;
use log::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Command {
    c_type: CommandType,
    adp: u16,
    ado: u16,
}

impl Default for Command {
    fn default() -> Self {
        Command::new(CommandType::APRD, 0, 0)
    }
}

impl Command {
    pub fn new(c_type: CommandType, adp: u16, ado: u16) -> Self {
        Command { c_type, adp, ado }
    }

    pub fn new_read(slave_address: SlaveAddress, ado: u16) -> Self {
        let (c_type, adp) = match slave_address {
            SlaveAddress::SlavePosition(adp) => (CommandType::APRD, get_ap_adp(adp)),
            SlaveAddress::StationAddress(adp) => (CommandType::FPRD, adp),
        };
        Command { c_type, adp, ado }
    }

    pub fn new_write(slave_address: SlaveAddress, ado: u16) -> Self {
        let (c_type, adp) = match slave_address {
            SlaveAddress::SlavePosition(adp) => (CommandType::APWR, get_ap_adp(adp)),
            SlaveAddress::StationAddress(adp) => (CommandType::FPWR, adp),
        };
        Command { c_type, adp, ado }
    }

    pub fn new_logical_read_write(logical_address: u32) -> Self {
        let adp = (logical_address & 0x0000_ffff) as u16;
        let ado = (logical_address >> 16) as u16;
        Command {
            c_type: CommandType::LRW,
            adp,
            ado,
        }
    }
}

#[derive(Debug)]
pub struct EtherCatInterface<'a, D, T>
where
    D: Device,
    T: CountDown<Time = MicrosDurationU32>,
{
    ethdev: D,
    buffer: &'a mut [u8],
    data_size: usize,
    buffer_size: usize,
    should_recv_frames: usize,
    timer: T,
}

impl<'a, D, T> EtherCatInterface<'a, D, T>
where
    D: Device,
    T: CountDown<Time = MicrosDurationU32>,
{
    pub fn new(ethdev: D, timer: T, buffer: &'a mut [u8]) -> Self {
        let buffer_size = buffer.len();
        Self {
            ethdev,
            buffer,
            data_size: 0,
            buffer_size,
            should_recv_frames: 0,
            timer,
        }
    }

    pub fn remaing_capacity(&self) -> usize {
        self.buffer_size - self.data_size - EtherCatHeader::SIZE - WKC_LENGTH
    }

    pub fn add_command<F: FnOnce(&mut [u8])>(
        &mut self,
        pdu_index: u8,
        command: Command,
        data_size: usize,
        data_writer: F,
    ) -> Result<(), CommonError> {
        if self.data_size + EtherCatHeader::SIZE + data_size + WKC_LENGTH > self.buffer_size {
            return Err(CommonError::BufferExhausted);
        }

        if data_size
            > self.ethdev.max_transmission_unit()
                - (EthernetHeader::SIZE
                    + EtherCatHeader::SIZE
                    + EtherCatPduHeader::SIZE
                    + WKC_LENGTH)
        {
            return Err(CommonError::BufferExhausted);
        }

        let mut header = [0; EtherCatPduHeader::SIZE];
        let mut pdu = EtherCatPduHeader(&mut header);
        pdu.set_index(pdu_index);
        pdu.set_command_type(command.c_type as u8);
        pdu.set_adp(command.adp);
        pdu.set_ado(command.ado);
        pdu.set_length(data_size as u16);

        self.buffer[self.data_size..self.data_size + EtherCatPduHeader::SIZE]
            .copy_from_slice(&header);
        data_writer(
            &mut self.buffer[self.data_size + EtherCatPduHeader::SIZE
                ..self.data_size + EtherCatPduHeader::SIZE + data_size],
        );

        // WKC field
        self.buffer[self.data_size + EtherCatPduHeader::SIZE + data_size + 1] = 0;
        self.buffer[self.data_size + EtherCatPduHeader::SIZE + data_size + 2] = 0;

        self.data_size += EtherCatPduHeader::SIZE + data_size + WKC_LENGTH;
        Ok(())
    }

    pub fn consume_command(&mut self) -> EtherCatPdus {
        let pdus = EtherCatPdus::new(self.buffer, self.data_size, 0);
        self.data_size = 0;
        pdus
    }

    pub fn poll<I: Into<MicrosDurationU32>>(&mut self, recv_timeout: I) -> Result<(), CommonError> {
        if !self.transmit() {
            return Err(CommonError::DeviceErrorTx);
        }
        match self.receive(recv_timeout) {
            RxRes::Ok => (),
            RxRes::DeviceError => return Err(CommonError::DeviceErrorRx),
            //RxRes::TimerError => return Err(CommonError::UnspcifiedTimerError),
            //RxRes::Timeout => return Err(CommonError::ReceiveTimeout),
            RxRes::Timeout => (),
        }
        Ok(())
    }

    fn transmit(&mut self) -> bool {
        let Self {
            ethdev,
            buffer,
            data_size,
            should_recv_frames,
            ..
        } = self;
        let buffer = &buffer[0..*data_size];
        let mtu = ethdev.max_transmission_unit();
        let max_send_count = EtherCatPdus::new(buffer, *data_size, 0).count();
        let mut actual_send_count = 0;

        while actual_send_count < max_send_count {
            let pdus = EtherCatPdus::new(buffer, *data_size, 0);
            let mut send_size = 0;
            let mut send_count = actual_send_count;
            for pdu in pdus {
                let pdu_length = pdu.length() as usize + EtherCatPduHeader::SIZE + WKC_LENGTH;
                if mtu > send_size + pdu_length {
                    send_size += pdu_length;
                    send_count += 1;
                } else {
                    break;
                }
            }

            if ethdev.send(
                EthernetHeader::SIZE + EtherCatHeader::SIZE + send_size,
                |tx_buffer| {
                    let mut ec_frame = EtherCatFrame::new_unchecked(tx_buffer);
                    ec_frame.init();
                    let pdus = EtherCatPdus::new(buffer, *data_size, 0);
                    for (i, pdu) in pdus.into_iter().enumerate().skip(actual_send_count) {
                        if i >= send_count {
                            break;
                        }
                        let index = pdu.index();
                        let command = CommandType::new(pdu.command_type());
                        let adp = pdu.adp();
                        let ado = pdu.ado();
                        let data = pdu.data();
                        if !ec_frame.add_command(command, adp, ado, data, Some(index)) {
                            error!("Failed to add command");
                            panic!();
                        }
                        actual_send_count += 1;
                    }
                    *should_recv_frames += 1;
                    Some(())
                },
            ).is_none() {
                error!("Failed to consume TX token");
                return false;
            }
        }
        true
    }

    fn receive<I: Into<MicrosDurationU32>>(&mut self, timeout: I) -> RxRes {
        let Self {
            ethdev,
            buffer,
            should_recv_frames,
            ..
        } = self;
        let mut data_size = 0;
        self.timer.start(timeout);
        while *should_recv_frames > 0 {
            if ethdev.recv(|frame| {
                info!("something receive");
                let eth = EthernetHeader(&frame);
                if eth.source() == SRC_MAC || eth.ether_type() != ETHERCat_TYPE {
                    return Some(());
                }
                let ec_frame = EtherCatFrame::new_unchecked(frame);
                for pdu in ec_frame.iter_dlpdu() {
                    let pdu_size = EtherCatPduHeader::SIZE + pdu.length() as usize + WKC_LENGTH;
                    buffer[data_size..data_size + pdu_size].copy_from_slice(pdu.0);
                    data_size += pdu_size;
                }
                *should_recv_frames -= 1;
                Some(())
            }).is_none() {
                return RxRes::DeviceError;
            }
            match self.timer.wait() {
                Ok(_) => return RxRes::Timeout,
                Err(nb::Error::Other(_void)) => unreachable!(),
                Err(nb::Error::WouldBlock) => (),
            }
        }
        assert_eq!(data_size, self.data_size);
        RxRes::Ok
    }
}

enum RxRes {
    Ok,
    DeviceError,
    Timeout,
    //TimerError,
}

#[derive(Debug, Clone, Copy)]
pub enum SlaveAddress {
    StationAddress(u16),
    SlavePosition(u16),
}

impl SlaveAddress {
    pub fn get_ado(&self) -> u16 {
        match self {
            Self::StationAddress(addr) => *addr,
            Self::SlavePosition(pos) => *pos,
        }
    }
}

impl Default for SlaveAddress {
    fn default() -> Self {
        SlaveAddress::SlavePosition(0)
    }
}

//impl<'a, D, T> EtherCatInterface<'a, D, T>
//where
//    D: Device,
//    T: CountDown<Time = MicrosDurationU32>,
//{
//    pub fn read_register(
//        &mut self,
//        slave_address: SlaveAddress,
//        register_address: u16,
//        size: usize,
//        //timeout: I,
//    ) -> Result<EtherCatPdu<&[u8]>, CommonError> {
//        match slave_address {
//            SlaveAddress::StationAddress(adr) => self.add_command(
//                u8::MAX,
//                Command::new(CommandType::FPRD, adr, register_address),
//                size,
//                |buf| buf.iter_mut().for_each(|b| *b = 0),
//            )?,
//            SlaveAddress::SlavePosition(adr) => self.add_command(
//                u8::MAX,
//                Command::new(CommandType::APRD, get_ap_adp(adr), register_address),
//                size,
//                |buf| buf.iter_mut().for_each(|b| *b = 0),
//            )?,
//        };
//        self.poll(MicrosDurationU32::from_ticks(1000))?;
//        let pdu = self
//            .consume_command()
//            .last()
//            .ok_or(CommonError::BadPacket)?;
//        check_wkc(&pdu, 1)?;
//        Ok(pdu)
//    }
//
//    pub fn write_register<F: FnOnce(&mut [u8])>(
//        &mut self,
//        slave_address: SlaveAddress,
//        register_address: u16,
//        size: usize,
//        //timeout: I,
//        buffer_writer: F,
//    ) -> Result<EtherCatPdu<&[u8]>, CommonError> {
//        match slave_address {
//            SlaveAddress::StationAddress(adr) => self.add_command(
//                u8::MAX,
//                Command::new(CommandType::FPWR, adr, register_address),
//                size,
//                buffer_writer,
//            )?,
//            SlaveAddress::SlavePosition(adr) => self.add_command(
//                u8::MAX,
//                Command::new(CommandType::APWR, get_ap_adp(adr), register_address),
//                size,
//                buffer_writer,
//            )?,
//        }
//        self.poll(MicrosDurationU32::from_ticks(1000))?;
//        let pdu = self
//            .consume_command()
//            .last()
//            .ok_or(CommonError::BadPacket)?;
//        check_wkc(&pdu, 1)?;
//        Ok(pdu)
//    }
//}
//
//macro_rules! define_read_specific_register {
//    ($($func: ident, $reg: ident, $address: ident;)*) =>{
//        impl<'a, D: Device, T> EtherCatInterface<'a, D, T>
//        where
//            D: Device,
//            T: CountDown<Time = MicrosDurationU32>,
//        {
//            $(pub fn $func(
//                &mut self,
//                slave_address: SlaveAddress,
//            ) -> Result<$reg<[u8; $reg::SIZE]>, CommonError> {
//                self.read_register(slave_address, $reg::$address, $reg::SIZE)
//                .map(|pdu| {
//                    let mut copied = [0; $reg::SIZE];
//                    copied.copy_from_slice(&pdu.0[EtherCatPduHeader::SIZE..EtherCatPduHeader::SIZE + $reg::SIZE]);
//                    $reg(copied)}
//                )
//            })*
//        }
//    };
//}
//
//macro_rules! define_write_specific_register {
//    ($($func: ident, $reg: ident, $address: ident;)*) =>{
//        impl<'a, D, T> EtherCatInterface<'a, D, T>
//        where
//            D: Device,
//            T: CountDown<Time = MicrosDurationU32>,
//        {
//            //$(pub fn $func<F: FnOnce(&mut $reg<[u8; $reg::SIZE]>)>(
//            $(pub fn $func(
//                &mut self,
//                slave_address: SlaveAddress,
//                initial_value: Option<$reg::<[u8; $reg::SIZE]>>,
//                //data_writer: F,
//            ) -> Result<$reg<&[u8]>, CommonError> {
//                self.write_register(slave_address, $reg::$address, $reg::SIZE,
//                    |buf|{
//                    let initial_value = initial_value.unwrap_or($reg([0;$reg::SIZE]));
//                    //data_writer(&mut initial_value);
//                    buf.copy_from_slice(&initial_value.0);
//                })
//                .map(|pdu| $reg(&pdu.0[EtherCatPduHeader::SIZE..EtherCatPduHeader::SIZE + $reg::SIZE]))
//            })*
//        }
//    };
//}
//
//define_read_specific_register! {
//    read_dl_information, DLInformation, ADDRESS;
//    read_fixed_station_address, FixedStationAddress, ADDRESS;
//    read_dl_control, DLControl, ADDRESS;
//    read_dl_status, DLStatus, ADDRESS;
//    read_rx_error_counter, RxErrorCounter, ADDRESS;
//    read_watch_dog_divider, WatchDogDivider, ADDRESS;
//    read_dl_user_watch_dog, DLUserWatchDog, ADDRESS;
//    read_sm_watch_dog, SyncManagerChannelWatchDog, ADDRESS;
//    read_sm_watch_dog_status, SyncManagerChannelWDStatus, ADDRESS;
//    read_sii_access, SIIAccess, ADDRESS;
//    read_sii_control, SIIControl, ADDRESS;
//    read_sii_address, SIIAddress, ADDRESS;
//    read_sii_data, SIIData, ADDRESS;
//    read_fmmu0, FMMURegister, ADDRESS0;
//    read_fmmu1, FMMURegister, ADDRESS1;
//    read_fmmu2, FMMURegister, ADDRESS2;
//    read_sm0, SyncManagerRegister, ADDRESS0;
//    read_sm1, SyncManagerRegister, ADDRESS1;
//    read_sm2, SyncManagerRegister, ADDRESS2;
//    read_sm3, SyncManagerRegister, ADDRESS3;
//    read_dc_recieve_time, DcRecieveTime, ADDRESS;
//    read_dc_system_time, DcSystemTime, ADDRESS;
//    read_al_control, AlControl, ADDRESS;
//    read_al_status, AlStatus, ADDRESS;
//    read_pdi_control, PdiControl, ADDRESS;
//    read_pdi_config, PdiConfig, ADDRESS;
//    read_sync_config, SyncConfig, ADDRESS;
//    read_dc_activation, DcActivation, ADDRESS;
//    read_sync_pulse, SyncPulse, ADDRESS;
//    read_interrupt_status, InterruptStatus, ADDRESS;
//    read_cyclic_operation_start_time, CyclicOperationStartTime, ADDRESS;
//    read_sync0_cycle_time, Sync0CycleTime, ADDRESS;
//    read_sync1_cycle_time, Sync1CycleTime, ADDRESS;
//    read_latch_edge, LatchEdge, ADDRESS;
//    read_latch_event, LatchEvent, ADDRESS;
//    read_latch0_positive_edge_value, Latch0PositiveEdgeValue, ADDRESS;
//    read_latch0_negative_edge_value, Latch0NegativeEdgeValue, ADDRESS;
//    read_latch1_positive_edge_value, Latch1PositiveEdgeValue, ADDRESS;
//    read_latch1_negative_edge_value, Latch1NegativeEdgeValue, ADDRESS;
//}
//
//define_write_specific_register! {
//    write_fixed_station_address, FixedStationAddress, ADDRESS;
//    write_dl_control, DLControl, ADDRESS;
//    write_rx_error_counter, RxErrorCounter, ADDRESS;
//    write_watch_dog_divider, WatchDogDivider, ADDRESS;
//    write_dl_user_watch_dog, DLUserWatchDog, ADDRESS;
//    write_sm_watch_dog, SyncManagerChannelWatchDog, ADDRESS;
//    write_sii_access, SIIAccess, ADDRESS;
//    write_sii_control, SIIControl, ADDRESS;
//    write_sii_address, SIIAddress, ADDRESS;
//    write_sii_data, SIIData, ADDRESS;
//    write_fmmu0, FMMURegister, ADDRESS0;
//    write_fmmu1, FMMURegister, ADDRESS1;
//    write_fmmu2, FMMURegister, ADDRESS2;
//    write_sm0, SyncManagerRegister, ADDRESS0;
//    write_sm1, SyncManagerRegister, ADDRESS1;
//    write_sm2, SyncManagerRegister, ADDRESS2;
//    write_sm3, SyncManagerRegister, ADDRESS3;
//    write_dc_recieve_time, DcRecieveTime, ADDRESS;
//    write_dc_system_time, DcSystemTime, ADDRESS;
//    write_al_control, AlControl, ADDRESS;
//    write_dc_activation, DcActivation, ADDRESS;
//    write_cyclic_operation_start_time, CyclicOperationStartTime, ADDRESS;
//    write_sync0_cycle_time, Sync0CycleTime, ADDRESS;
//    write_sync1_cycle_time, Sync1CycleTime, ADDRESS;
//    write_latch_edge, LatchEdge, ADDRESS;
//    write_latch_event, LatchEvent, ADDRESS;
//    write_latch0_positive_edge_value, Latch0PositiveEdgeValue, ADDRESS;
//    write_latch0_negative_edge_value, Latch0NegativeEdgeValue, ADDRESS;
//    write_latch1_positive_edge_value, Latch1PositiveEdgeValue, ADDRESS;
//    write_latch1_negative_edge_value, Latch1NegativeEdgeValue, ADDRESS;
//}
//
