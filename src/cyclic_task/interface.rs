use crate::frame::*;
use crate::hal::{CountDown, Device, RxToken, TxToken};

use crate::util::*;
use core::time::Duration;
use log::*;

#[derive(Debug, Clone)]
pub enum CommandInterfaceError {
    TxError,
    RxError,
    TooLargeData,
    NotEnoughCapacityLeft,
    TxTimeout,
    RxTimeout,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Command {
    pub c_type: CommandType,
    pub adp: u16,
    pub ado: u16,
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

    pub fn new_read(slave_address: TargetSlave, ado: u16) -> Self {
        let (c_type, adp) = match slave_address {
            TargetSlave::Single(addr) => match addr {
                SlaveAddress::SlavePosition(adp) => (CommandType::APRD, get_ap_adp(adp)),
                SlaveAddress::StationAddress(adp) => (CommandType::FPRD, adp),
            },
            TargetSlave::All(_) => (CommandType::BRD, 0),
        };
        Command { c_type, adp, ado }
    }

    pub fn new_write(slave_address: TargetSlave, ado: u16) -> Self {
        let (c_type, adp) = match slave_address {
            TargetSlave::Single(addr) => match addr {
                SlaveAddress::SlavePosition(adp) => (CommandType::APWR, get_ap_adp(adp)),
                SlaveAddress::StationAddress(adp) => (CommandType::FPWR, adp),
            },
            TargetSlave::All(_) => (CommandType::BWR, 0),
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
pub struct CommandInterface<'a, D, T>
where
    D: for<'d> Device<'d>,
    T: CountDown,
{
    ethdev: D,
    timer: T,
    buffer: &'a mut [u8],
    data_size: usize,
    capacity: usize,
    should_recv_frames: usize,
}

impl<'a, D, T> CommandInterface<'a, D, T>
where
    D: for<'d> Device<'d>,
    T: CountDown,
{
    pub fn new(ethdev: D, timer: T, buffer: &'a mut [u8]) -> Self {
        let capacity = buffer.len().min(ethdev.max_transmission_unit());
        Self {
            ethdev,
            timer,
            buffer,
            data_size: 0,
            capacity,
            should_recv_frames: 0,
        }
    }

    pub fn remainig_capacity(&self) -> usize {
        self.capacity - self.data_size - EtherCatHeader::SIZE - WKC_LENGTH
    }

    pub fn add_command<F: FnOnce(&mut [u8])>(
        &mut self,
        pdu_index: u8,
        command: Command,
        data_size: usize,
        data_writer: F,
    ) -> Result<(), CommandInterfaceError> {
        if self.data_size + EtherCatHeader::SIZE + data_size + WKC_LENGTH > self.capacity {
            return Err(CommandInterfaceError::NotEnoughCapacityLeft);
        }

        if data_size
            > self.ethdev.max_transmission_unit()
                - (EthernetHeader::SIZE
                    + EtherCatHeader::SIZE
                    + EtherCatPduHeader::SIZE
                    + WKC_LENGTH)
        {
            return Err(CommandInterfaceError::TooLargeData);
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

        // Wkc field
        self.buffer[self.data_size + EtherCatPduHeader::SIZE + data_size + 1] = 0;
        self.buffer[self.data_size + EtherCatPduHeader::SIZE + data_size + 2] = 0;

        self.data_size += EtherCatPduHeader::SIZE + data_size + WKC_LENGTH;
        Ok(())
    }

    pub fn consume_commands(&mut self) -> EtherCatPdus {
        let pdus = EtherCatPdus::new(self.buffer, self.data_size, 0);
        self.data_size = 0;
        pdus
    }

    pub fn poll<I: Into<Duration> + Clone>(&mut self, timeout: I) -> Result<(), CommandInterfaceError> {
        self.transmit(timeout.clone())?;
        self.receive(timeout)
    }

    fn transmit<I: Into<Duration>>(&mut self, timeout: I) -> Result<(), CommandInterfaceError> {
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
        *should_recv_frames = 0;
        self.timer.start(timeout);

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
            loop {
                if let Some(tx_token) = ethdev.transmit() {
                    let len = EthernetHeader::SIZE + EtherCatHeader::SIZE + send_size;
                    let tx_result = tx_token.consume(len, |tx_buffer| {
                        //info!("something send");
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
                        Ok(())
                    });
                    if tx_result.is_err() {
                        error!("Failed to consume TX token");
                        return Err(CommandInterfaceError::TxError);
                    } else {
                        break;
                    }
                }
                match self.timer.wait() {
                    Ok(_) => return Err(CommandInterfaceError::TxTimeout),
                    Err(nb::Error::WouldBlock) => {}
                    Err(nb::Error::Other(_)) => unreachable!(),
                }
            }
        }
        Ok(())
    }

    fn receive<I: Into<Duration>>(&mut self, timeout: I) -> Result<(), CommandInterfaceError> {
        let Self {
            ethdev,
            buffer,
            should_recv_frames,
            ..
        } = self;
        let mut data_size = 0;
        self.timer.start(timeout);
        while 0 < *should_recv_frames {
            loop {
                if let Some(rx_token) = ethdev.receive() {
                    let rx_result = rx_token.consume(|frame| {
                        info!("something receive");
                        let eth = EthernetHeader(&frame);
                        if eth.source() == SRC_MAC || eth.ether_type() != ETHERCAT_TYPE {
                            //info!("{} {}", eth.source(), SRC_MAC);
                            //info!("{} {}", eth.ether_type(), ETHERCAT_TYPE);

                            return Ok(());
                        }
                        let ec_frame = EtherCatFrame::new_unchecked(frame);
                        for pdu in ec_frame.dlpdus() {
                            let pdu_size =
                                EtherCatPduHeader::SIZE + pdu.length() as usize + WKC_LENGTH;
                            buffer[data_size..data_size + pdu_size].copy_from_slice(pdu.0);
                            data_size += pdu_size;
                        }
                        *should_recv_frames -= 1;
                        Ok(())
                    });
                    if rx_result.is_err() {
                        return Err(CommandInterfaceError::RxError);
                    } else {
                        break;
                    }
                }
                match self.timer.wait() {
                    Ok(_) => return Err(CommandInterfaceError::RxTimeout),
                    Err(nb::Error::WouldBlock) => {}
                    Err(nb::Error::Other(_)) => unreachable!(),
                }
            }
        }
        assert_eq!(data_size, self.data_size);
        Ok(())
    }
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

#[derive(Debug, Clone, Copy)]
pub enum TargetSlave {
    Single(SlaveAddress),
    All(u16),
}

impl From<SlaveAddress> for TargetSlave {
    fn from(address: SlaveAddress) -> Self {
        Self::Single(address)
    }
}

impl Default for TargetSlave {
    fn default() -> Self {
        Self::Single(SlaveAddress::default())
    }
}
