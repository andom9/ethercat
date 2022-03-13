use crate::arch::Device;
use crate::error::CommonError;
use crate::ethercat_frame::*;
use crate::packet::ethercat::*;
use crate::register::{application::*, datalink::*};
use crate::util::*;
use log::*;

#[derive(Debug)]
pub struct EtherCATInterface<'a, D: Device> {
    ethdev: D,
    buffer: &'a mut [u8],
    data_size: usize,
    buffer_size: usize,
    should_recv_frames: usize,
}

impl<'a, D: Device> EtherCATInterface<'a, D> {
    pub fn new(ethdev: D, buffer: &'a mut [u8]) -> Self {
        let buffer_size = buffer.len();
        Self {
            ethdev,
            buffer,
            data_size: 0,
            buffer_size,
            should_recv_frames: 0,
        }
    }

    pub fn add_command<F: FnOnce(&mut [u8])>(
        &mut self,
        command: CommandType,
        adp: u16,
        ado: u16,
        data_size: usize,
        data_writer: F,
    ) -> Result<(), CommonError> {
        if self.data_size + ETHERCAT_HEADER_LENGTH + data_size + WKC_LENGTH > self.buffer_size {
            return Err(CommonError::BufferExhausted);
        }

        if data_size
            > self.ethdev.max_transmission_unit()
                - (ETHERNET_HEADER_LENGTH
                    + ETHERCAT_HEADER_LENGTH
                    + ETHERCATPDU_HEADER_LENGTH
                    + WKC_LENGTH)
        {
            return Err(CommonError::BufferExhausted);
        }

        let mut header = [0; ETHERCATPDU_HEADER_LENGTH];
        let mut pdu = EtherCATPDU::new_unchecked(&mut header);
        pdu.set_command_type(command as u8);
        pdu.set_adp(adp);
        pdu.set_ado(ado);
        pdu.set_length(data_size as u16);

        self.buffer[self.data_size..self.data_size + ETHERCATPDU_HEADER_LENGTH]
            .copy_from_slice(&header);
        data_writer(
            &mut self.buffer[self.data_size + ETHERCATPDU_HEADER_LENGTH
                ..self.data_size + ETHERCATPDU_HEADER_LENGTH + data_size],
        );

        // WKC field
        self.buffer[self.data_size + ETHERCATPDU_HEADER_LENGTH + data_size + 1] = 0;
        self.buffer[self.data_size + ETHERCATPDU_HEADER_LENGTH + data_size + 2] = 0;

        self.data_size += ETHERCATPDU_HEADER_LENGTH + data_size + WKC_LENGTH;
        Ok(())
    }

    pub fn consume_command(&mut self) -> EtherCATPDUs {
        let pdus = EtherCATPDUs::new(self.buffer, self.data_size, 0);
        self.data_size = 0;
        pdus
    }

    pub fn poll(&mut self) -> Result<(), CommonError> {
        if !self.transmit() {
            return Err(CommonError::DeviceErrorTx);
        }
        if !self.receive() {
            return Err(CommonError::DeviceErrorRx);
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
        let max_send_count = EtherCATPDUs::new(buffer, *data_size, 0).count();
        let mut actual_send_count = 0;

        while actual_send_count < max_send_count {
            let pdus = EtherCATPDUs::new(buffer, *data_size, 0);
            let mut send_size = 0;
            let mut send_count = actual_send_count;
            for pdu in pdus {
                let pdu_length = pdu.length() as usize + ETHERCATPDU_HEADER_LENGTH + WKC_LENGTH;
                if mtu > send_size + pdu_length {
                    send_size += pdu_length;
                    send_count += 1;
                } else {
                    break;
                }
            }

            if let None = ethdev.send(
                ETHERNET_HEADER_LENGTH + ETHERCAT_HEADER_LENGTH + send_size,
                |tx_buffer| {
                    let mut ec_frame = EtherCATFrame::new_unchecked(tx_buffer);
                    ec_frame.init();
                    let pdus = EtherCATPDUs::new(buffer, *data_size, 0);
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
            ) {
                error!("Failed to consume TX token");
                return false;
            }
        }
        true
    }

    // TODO: timeout
    fn receive(&mut self) -> bool {
        let Self {
            ethdev,
            buffer,
            should_recv_frames,
            ..
        } = self;
        let mut data_size = 0;
        while *should_recv_frames > 0 {
            if let None = ethdev.recv(|frame| {
                info!("something receive");
                let eth = EthernetHeader::new_unchecked(&frame);
                if eth.source() == SRC_MAC || eth.ether_type() != ETHERCAT_TYPE {
                    return Some(());
                }
                let ec_frame = EtherCATFrame::new_unchecked(frame);
                for pdu in ec_frame.iter_dlpdu() {
                    let pdu_size = ETHERCATPDU_HEADER_LENGTH + pdu.length() as usize + WKC_LENGTH;
                    buffer[data_size..data_size + pdu_size].copy_from_slice(&pdu.0);
                    data_size += pdu_size;
                }
                *should_recv_frames -= 1;
                Some(())
            }) {}
        }
        assert_eq!(data_size, self.data_size);
        true
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SlaveAddress {
    StationAddress(u16),
    SlaveNumber(u16),
}

impl<'a, D: Device> EtherCATInterface<'a, D> {
    pub fn read_register(
        &mut self,
        slave_address: SlaveAddress,
        register_address: u16,
        size: usize,
    ) -> Result<EtherCATPDU<&[u8]>, CommonError> {
        match slave_address {
            SlaveAddress::StationAddress(adr) => {
                self.add_command(CommandType::FPRD, adr, register_address, size, |buf| {
                    buf.iter_mut().for_each(|b| *b = 0)
                })?
            }
            SlaveAddress::SlaveNumber(adr) => self.add_command(
                CommandType::APRD,
                get_ap_adp(adr),
                register_address,
                size,
                |buf| buf.iter_mut().for_each(|b| *b = 0),
            )?,
        };
        self.poll()?;
        let pdu = self.consume_command().last().ok_or(CommonError::Dropped)?;
        check_wkc(&pdu, 1)?;
        Ok(pdu)
    }

    pub fn write_register<F: FnOnce(&mut [u8])>(
        &mut self,
        slave_address: SlaveAddress,
        register_address: u16,
        size: usize,
        buffer_writer: F,
    ) -> Result<EtherCATPDU<&[u8]>, CommonError> {
        match slave_address {
            SlaveAddress::StationAddress(adr) => self.add_command(
                CommandType::FPWR,
                adr,
                register_address,
                size,
                buffer_writer,
            )?,
            SlaveAddress::SlaveNumber(adr) => self.add_command(
                CommandType::APWR,
                get_ap_adp(adr),
                register_address,
                size,
                buffer_writer,
            )?,
        }
        self.poll()?;
        let pdu = self.consume_command().last().ok_or(CommonError::Dropped)?;
        check_wkc(&pdu, 1)?;
        Ok(pdu)
    }
}

macro_rules! define_read_specific_register {
    ($($func: ident, $reg: ident, $address: ident;)*) =>{
        impl<'a, D: Device> EtherCATInterface<'a, D> {
            $(pub fn $func(
                &mut self,
                slave_address: SlaveAddress
            ) -> Result<$reg<[u8; $reg::<&[u8]>::SIZE]>, CommonError> {
                self.read_register(slave_address, $reg::<&[u8]>::$address, $reg::<&[u8]>::SIZE)
                .map(|pdu| {
                    let mut copied = [0; $reg::<&[u8]>::SIZE];
                    copied.copy_from_slice(&pdu.0[ETHERCATPDU_HEADER_LENGTH..ETHERCATPDU_HEADER_LENGTH + $reg::<&[u8]>::SIZE]);
                    $reg(copied)}
                )
            })*
        }
    };
}

macro_rules! define_write_specific_register {
    ($($func: ident, $reg: ident, $address: ident;)*) =>{
        impl<'a, D: Device> EtherCATInterface<'a, D> {
            $(pub fn $func<F: FnOnce(&mut $reg<[u8; $reg::<&[u8]>::SIZE]>)>(
                &mut self,
                slave_address: SlaveAddress,
                initial_value: Option<$reg::<[u8; $reg::<&[u8]>::SIZE]>>,
                data_writer: F,
            ) -> Result<$reg<&[u8]>, CommonError> {
                self.write_register(slave_address, $reg::<&[u8]>::$address, $reg::<&[u8]>::SIZE, |buf|{
                    let mut initial_value = initial_value.unwrap_or($reg([0;$reg::<&[u8]>::SIZE]));
                    data_writer(&mut initial_value);
                    buf.copy_from_slice(&initial_value.0);
                })
                .map(|pdu| $reg(&pdu.0[ETHERCATPDU_HEADER_LENGTH..ETHERCATPDU_HEADER_LENGTH + $reg::<&[u8]>::SIZE]))
            })*
        }
    };
}

define_read_specific_register! {
    read_dl_information, DLInformation, ADDRESS;
    read_fixed_station_address, FixedStationAddress, ADDRESS;
    read_dl_control, DLControl, ADDRESS;
    read_dl_status, DLStatus, ADDRESS;
    read_rx_error_counter, RxErrorCounter, ADDRESS;
    read_watch_dog_divider, WatchDogDivider, ADDRESS;
    read_dl_user_watch_dog, DLUserWatchDog, ADDRESS;
    read_sm_watch_dog, SyncManagerChannelWatchDog, ADDRESS;
    read_sm_watch_dog_status, SyncManagerChannelWDStatus, ADDRESS;
    read_sii_access, SIIAccess, ADDRESS;
    read_sii_control, SIIControl, ADDRESS;
    read_sii_address, SIIAddress, ADDRESS;
    read_sii_data, SIIData, ADDRESS;
    read_fmmu0, FMMURegister, ADDRESS0;
    read_fmmu1, FMMURegister, ADDRESS1;
    read_fmmu2, FMMURegister, ADDRESS2;
    read_sm0, SyncManagerRegister, ADDRESS0;
    read_sm1, SyncManagerRegister, ADDRESS1;
    read_sm2, SyncManagerRegister, ADDRESS2;
    read_sm3, SyncManagerRegister, ADDRESS3;
    read_dc_transmission, DCTransmission, ADDRESS;
    read_al_control, ALControl, ADDRESS;
    read_al_status, ALStatus, ADDRESS;
    read_pdi_control, PDIControl, ADDRESS;
    read_pdi_config, PDIConfig, ADDRESS;
    read_sync_config, SyncConfig, ADDRESS;
}

define_write_specific_register! {
    write_fixed_station_address, FixedStationAddress, ADDRESS;
    write_dl_control, DLControl, ADDRESS;
    write_rx_error_counter, RxErrorCounter, ADDRESS;
    write_watch_dog_divider, WatchDogDivider, ADDRESS;
    write_dl_user_watch_dog, DLUserWatchDog, ADDRESS;
    write_sm_watch_dog, SyncManagerChannelWatchDog, ADDRESS;
    write_sm_watch_dog_status, SyncManagerChannelWDStatus, ADDRESS;
    write_sii_access, SIIAccess, ADDRESS;
    write_sii_control, SIIControl, ADDRESS;
    write_sii_address, SIIAddress, ADDRESS;
    write_sii_data, SIIData, ADDRESS;
    write_fmmu0, FMMURegister, ADDRESS0;
    write_fmmu1, FMMURegister, ADDRESS1;
    write_fmmu2, FMMURegister, ADDRESS2;
    write_sm0, SyncManagerRegister, ADDRESS0;
    write_sm1, SyncManagerRegister, ADDRESS1;
    write_sm2, SyncManagerRegister, ADDRESS2;
    write_sm3, SyncManagerRegister, ADDRESS3;
    write_dc_transmission, DCTransmission, ADDRESS;
    write_al_control, ALControl, ADDRESS;
}
