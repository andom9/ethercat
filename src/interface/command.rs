use crate::frame::*;
use crate::hal::{RawEthernetDevice, RxToken, TxToken};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandInterfaceError {
    TxError,
    RxError,
    Busy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
pub struct CommandInterface<'a, D>
where
    D: for<'d> RawEthernetDevice<'d>,
{
    ethdev: D,
    buffer: &'a mut [u8],
    pdus_total_size: usize,
    pdu_count: usize,
    tx_count: usize,
    capacity: usize,
}

impl<'a, D> CommandInterface<'a, D>
where
    D: for<'d> RawEthernetDevice<'d>,
{
    pub fn new(ethdev: D, buffer: &'a mut [u8]) -> Self {
        let capacity = buffer.len().min(MAX_ETHERCAT_DATAGRAM);
        Self {
            ethdev,
            buffer,
            pdus_total_size: 0,
            pdu_count: 0,
            tx_count: 0,
            capacity,
        }
    }

    pub fn remainig_pdu_data_capacity(&self) -> usize {
        self.capacity - self.pdus_total_size - EtherCatPduHeader::SIZE - WKC_LENGTH
    }

    pub fn add_command<F: FnOnce(&mut [u8])>(
        &mut self,
        pdu_index: u8,
        command: Command,
        data_size: usize,
        data_writer: F,
    ) -> Result<(), Command> {
        if self.pdus_total_size + EtherCatPduHeader::SIZE + data_size + WKC_LENGTH > self.capacity {
            //return Err(CommandInterfaceError::NotEnoughCapacityLeft);
            return Err(command);
        }

        if data_size > MAX_PDU_DATAGRAM {
            //return Err(CommandInterfaceError::TooLargeData);
            return Err(command);
        }

        let mut header = [0; EtherCatPduHeader::SIZE];
        let mut pdu = EtherCatPduHeader(&mut header);
        pdu.set_index(pdu_index);
        pdu.set_command_type(command.c_type as u8);
        pdu.set_adp(command.adp);
        pdu.set_ado(command.ado);
        pdu.set_length(data_size as u16);

        self.buffer[self.pdus_total_size..self.pdus_total_size + EtherCatPduHeader::SIZE]
            .copy_from_slice(&header);
        data_writer(
            &mut self.buffer[self.pdus_total_size + EtherCatPduHeader::SIZE
                ..self.pdus_total_size + EtherCatPduHeader::SIZE + data_size],
        );

        // Wkc field
        self.buffer[self.pdus_total_size + EtherCatPduHeader::SIZE + data_size + 1] = 0;
        self.buffer[self.pdus_total_size + EtherCatPduHeader::SIZE + data_size + 2] = 0;

        self.pdus_total_size += EtherCatPduHeader::SIZE + data_size + WKC_LENGTH;
        Ok(())
    }

    pub fn consume_commands(&mut self) -> EtherCatPdus {
        let pdus = EtherCatPdus::new(self.buffer, self.pdus_total_size, 0);
        self.pdus_total_size = 0;
        self.pdu_count = 0;
        pdus
    }

    /// If true, all PDUs are transmitted
    /// If None, Phy is not ready
    pub fn transmit_one_frame(&mut self) -> Result<bool, CommandInterfaceError> {
        let Self {
            ethdev,
            buffer,
            pdus_total_size,
            tx_count,
            ..
        } = self;
        if self.pdu_count == 0 {
            return Ok(true);
        }
        let buffer = &buffer[0..*pdus_total_size];
        if let Some(tx_token) = ethdev.transmit() {
            let len = EthernetHeader::SIZE + EtherCatHeader::SIZE + *pdus_total_size;
            let tx_result = tx_token.consume(len, |tx_buffer| {
                let mut ec_frame = EtherCatFrame::new_unchecked(tx_buffer);
                ec_frame.init();
                let pdus = EtherCatPdus::new(buffer, *pdus_total_size, 0);
                for pdu in pdus {
                    let index = pdu.index();
                    let command = CommandType::new(pdu.command_type());
                    let adp = pdu.adp();
                    let ado = pdu.ado();
                    let data = pdu.data();
                    if !ec_frame.add_command(command, adp, ado, data, Some(index)) {
                        log::error!("Failed to add command");
                        panic!();
                    }
                    *tx_count += 1;
                }
                Ok(())
            });
            if tx_result.is_err() {
                log::error!("Failed to consume TX token");
                return Err(CommandInterfaceError::TxError);
            }
        } else {
            return Err(CommandInterfaceError::Busy);
        }

        if *tx_count < self.pdu_count {
            Ok(false)
        } else {
            Ok(true)
        }
    }

    /// If true, all PDUs are received
    /// If None, Phy is not ready
    pub fn receive_one_frame(&mut self) -> Result<bool, CommandInterfaceError> {
        let Self {
            ethdev,
            buffer,
            tx_count,
            ..
        } = self;
        let mut data_size = 0;
        if *tx_count == 0 {
            return Ok(true);
        }
        loop {
            if let Some(rx_token) = ethdev.receive() {
                let rx_result = rx_token.consume(|frame| {
                    log::info!("something receive");
                    let eth = EthernetHeader(&frame);
                    if eth.source() == SRC_MAC || eth.ether_type() != ETHERCAT_TYPE {
                        return Ok(()); //continue
                    }
                    let ec_frame = EtherCatFrame::new_unchecked(frame);
                    for pdu in ec_frame.dlpdus() {
                        let pdu_size = EtherCatPduHeader::SIZE + pdu.length() as usize + WKC_LENGTH;
                        buffer[data_size..data_size + pdu_size].copy_from_slice(pdu.0);
                        data_size += pdu_size;
                        *tx_count -= 0;
                    }
                    Ok(())
                });
                if rx_result.is_err() {
                    return Err(CommandInterfaceError::RxError);
                } else {
                    break;
                }
            } else {
                return Err(CommandInterfaceError::Busy);
            }
        }
        assert_eq!(data_size, self.pdus_total_size);
        if 0 < *tx_count {
            Ok(false)
        } else {
            Ok(true)
        }
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

impl TargetSlave {
    pub fn num_targets(&self) -> u16 {
        if let Self::All(num) = self {
            *num
        } else {
            1
        }
    }
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

fn get_ap_adp(slave_number: u16) -> u16 {
    if slave_number == 0 {
        0
    } else {
        0xFFFF - (slave_number - 1)
    }
}
