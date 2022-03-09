use crate::arch::Device;
use crate::error::*;
use crate::ethercat_frame::*;
use crate::packet::ethercat::*;
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

    pub fn add_command(
        &mut self,
        command: CommandType,
        adp: u16,
        ado: u16,
        data: &[u8],
    ) -> Result<(), BufferExhausted> {
        let pdu_data_size = data.len();
        if self.data_size + ETHERCAT_HEADER_LENGTH + pdu_data_size + WKC_LENGTH > self.buffer_size {
            return Err(BufferExhausted);
        }

        if pdu_data_size
            > self.ethdev.max_transmission_unit()
                - (ETHERNET_HEADER_LENGTH
                    + ETHERCAT_HEADER_LENGTH
                    + ETHERCATPDU_HEADER_LENGTH
                    + WKC_LENGTH)
        {
            return Err(BufferExhausted);
        }

        let mut header = [0; ETHERCATPDU_HEADER_LENGTH];
        let mut pdu = EtherCATPDU::new_unchecked(&mut header);
        pdu.set_command_type(command as u8);
        pdu.set_adp(adp);
        pdu.set_ado(ado);
        pdu.set_length(pdu_data_size as u16);

        self.buffer[self.data_size..self.data_size + ETHERCATPDU_HEADER_LENGTH]
            .copy_from_slice(&header);
        self.buffer[self.data_size + ETHERCATPDU_HEADER_LENGTH
            ..self.data_size + ETHERCATPDU_HEADER_LENGTH + pdu_data_size]
            .copy_from_slice(&data);
        self.buffer[self.data_size + ETHERCATPDU_HEADER_LENGTH + pdu_data_size + 1] = 0;
        self.buffer[self.data_size + ETHERCATPDU_HEADER_LENGTH + pdu_data_size + 2] = 0;

        self.data_size += ETHERCATPDU_HEADER_LENGTH + pdu_data_size + WKC_LENGTH;
        Ok(())
    }

    pub fn consume_command(&mut self) -> EtherCATPDUs {
        let pdus = EtherCATPDUs::new(self.buffer, self.data_size, 0);
        self.data_size = 0;
        pdus
    }

    pub fn poll(&mut self) -> Result<(), DeviceFailure> {
        if !self.transmit() {
            return Err(DeviceFailure::Tx);
        }
        if !self.receive() {
            return Err(DeviceFailure::Rx);
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

pub struct WKC(pub u16);

#[derive(Debug, Clone, Copy)]
pub struct BufferExhausted;

impl From<BufferExhausted> for Error {
    fn from(_error: BufferExhausted) -> Self {
        Error::BufferExhausted
    }
}

#[derive(Debug, Clone, Copy)]
pub enum DeviceFailure {
    Tx,
    Rx,
}

impl From<DeviceFailure> for Error {
    fn from(error: DeviceFailure) -> Self {
        match error {
            DeviceFailure::Rx => Error::RxDeviceFailed,
            DeviceFailure::Tx => Error::TxDeviceFailed,
        }
    }
}
