use crate::register::{application::*, datalink::*};
use bitfield::*;

pub const ETHERNET_HEADER_LENGTH: usize = 14;
pub const ETHERCAT_HEADER_LENGTH: usize = 2;
pub const ETHERCATPDU_HEADER_LENGTH: usize = 10;
pub const DST_MAC: u64 = 0x06_06_06_06_06_06;
pub const SRC_MAC: u64 = 0x01_01_01_01_01_01;
pub const MAILBOX_HEADER_LENGTH: usize = 6;
pub const WKC_LENGTH: usize = 2;
pub const ETHERCAT_TYPE: u16 = 0x88A4;

bitfield! {
    pub struct EthernetHeader(MSB0 [u8]);
    u64;
    pub destination, set_destination: 47, 0;
    pub source, set_source: 48+47, 48;
    u16;
    pub ether_type, set_ether_type: 48+47+1+15, 48+47+1;
}

impl<T: AsRef<[u8]>> EthernetHeader<T> {
    pub fn new(buf: T) -> Option<Self> {
        let packet = Self(buf);
        if packet.is_buffer_range_ok() {
            Some(packet)
        } else {
            None
        }
    }

    pub fn new_unchecked(buf: T) -> Self {
        Self(buf)
    }

    pub fn is_buffer_range_ok(&self) -> bool {
        self.0.as_ref().get(ETHERNET_HEADER_LENGTH - 1).is_some()
    }
}

impl<T: AsRef<[u8]> + AsMut<[u8]>> EthernetHeader<T> {
    pub fn set_ethercat_default(&mut self) {
        self.set_destination(0xFF_FF_FF_FF_FF_FF);
        self.set_source(0x01_01_01_01_01_01);
        self.set_ether_type(ETHERCAT_TYPE);
    }
}

bitfield! {
    pub struct EtherCATHeader([u8]);
    u16;
    pub length, set_length: 10, 0;
    u8;
    pub ethercat_type, set_ethercat_type: 15, 12;
}

impl<T: AsRef<[u8]>> EtherCATHeader<T> {
    pub fn new(buf: T) -> Option<Self> {
        let packet = Self(buf);
        if packet.is_buffer_range_ok() {
            Some(packet)
        } else {
            None
        }
    }

    pub fn new_unchecked(buf: T) -> Self {
        Self(buf)
    }

    pub fn is_buffer_range_ok(&self) -> bool {
        self.0.as_ref().get(ETHERCAT_HEADER_LENGTH - 1).is_some()
    }
}

bitfield! {
    pub struct EtherCATPDU([u8]);
    u8;
    pub command_type, set_command_type: 7, 0;
    pub index, set_index: 15, 8;
    u16;
    pub adp, set_adp: 31, 16;
    pub ado, set_ado: 47, 32;
    pub length, set_length: 58, 48;
    u8;
    pub is_circulated, set_is_circulated: 62;
    pub has_next, set_has_next: 63;
    u16;
    pub irq, set_irq: 64+15, 64;
}

impl<T: AsRef<[u8]>> EtherCATPDU<T> {
    pub fn new(buf: T) -> Option<Self> {
        let packet = Self(buf);
        if packet.is_buffer_range_ok() {
            Some(packet)
        } else {
            None
        }
    }

    pub fn new_unchecked(buf: T) -> Self {
        Self(buf)
    }

    pub fn is_buffer_range_ok(&self) -> bool {
        self.0.as_ref().get(ETHERCATPDU_HEADER_LENGTH - 1).is_some()
    }

    pub fn data(&self) -> &[u8] {
        &self.0.as_ref()
            [ETHERCATPDU_HEADER_LENGTH..ETHERCATPDU_HEADER_LENGTH + self.length() as usize]
    }

    pub fn wkc(&self) -> Option<u16> {
        let len = self.length() as usize;
        let low = self.0.as_ref().get(ETHERCATPDU_HEADER_LENGTH + len)?;
        let high = self.0.as_ref().get(ETHERCATPDU_HEADER_LENGTH + len + 1)?;
        Some(((*high as u16) << 8) | (*low as u16))
    }
}

bitfield! {
    pub struct MailboxPDU([u8]);
    u16;
    pub length, set_length: 15, 0;
    pub address, set_address: 31, 16;
    u8;
    pub prioriry, set_prioriry: 39, 38;
    pub mailbox_type, set_mailbox_type: 43, 40;
    pub count, set_count: 46, 44;
}

impl<T: AsRef<[u8]>> MailboxPDU<T> {
    pub fn new(buf: T) -> Option<Self> {
        let packet = Self(buf);
        if packet.is_buffer_range_ok() {
            Some(packet)
        } else {
            None
        }
    }

    //pub fn new_unchecked(buf: T) -> Self {
    //    Self(buf)
    //}

    pub fn is_buffer_range_ok(&self) -> bool {
        self.0.as_ref().get(MAILBOX_HEADER_LENGTH - 1).is_some()
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash, Copy)]
pub enum MailboxType {
    Error = 0,
    CoE = 3,
    FoE = 4,
}

pub const MAILBOX_ERROR_LENGTH: usize = 4;

bitfield! {
    pub struct MailboxError([u8]);
    u16;
    pub service_type, _: 15, 0;
    pub detail, _: 31, 16;
}

impl<T: AsRef<[u8]>> MailboxError<T> {
    pub fn new(buf: T) -> Option<Self> {
        let packet = Self(buf);
        if packet.is_buffer_range_ok() {
            Some(packet)
        } else {
            None
        }
    }

    //pub fn new_unchecked(buf: T) -> Self {
    //    Self(buf)
    //}

    pub fn is_buffer_range_ok(&self) -> bool {
        self.0.as_ref().get(MAILBOX_ERROR_LENGTH - 1).is_some()
    }
}

pub const FMMU_LENGTH: usize = 16;

bitfield! {
    pub struct FMMU([u8]);
    u32;
    pub logical_start_address, set_logical_start_address: 31, 0;
    u16;
    pub length, set_length: 47, 32;
    u8;
    pub logical_start_bit, set_logical_start_bit: 50, 48;
    pub logical_stop_bit, set_logical_stop_bit: 58, 56;
    u16;
    pub physical_start_address, set_physical_start_address: 79, 64;
    u8;
    pub physical_start_bit, set_physical_start_bit: 82, 80;
    pub read_access, set_read_access: 88;
    pub write_access, set_write_access: 89;
    pub active, set_active: 96;
}

impl<T: AsRef<[u8]>> FMMU<T> {
    pub fn new(buf: T) -> Option<Self> {
        let packet = Self(buf);
        if packet.is_buffer_range_ok() {
            Some(packet)
        } else {
            None
        }
    }

    //pub fn new_unchecked(buf: T) -> Self {
    //    Self(buf)
    //}

    pub fn is_buffer_range_ok(&self) -> bool {
        self.0.as_ref().get(FMMU_LENGTH - 1).is_some()
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash, Copy)]
pub enum CommandType {
    /// No operation
    /// A slave ignores the command.
    NOP = 0x00,
    /// Auto Increment Read
    /// A slave increments the address. A slave writes the data it has read to the EtherCAT datagram if the address received is zero.
    APRD,
    /// Auto Increment Write
    /// A slave increments the address. A slave writes data to a memory area if the address received is zero.
    APWR,
    /// Auto Increment Read Write
    /// A slave increments the address. A slave writes the data it has read to the EtherCAT datagram and writes the newly acquired data to the same memory area if the received address is zero.
    APRW,
    /// Configured Address Read
    /// A slave writes the data it has read to the EtherCAT datagram if its slave address matches one of the addresses configured in the datagram.
    FPRD,
    /// Configured Address Write
    /// A slave writes data to a memory area if its slave address matches one of the addresses configured in the datagram.
    FPWR,
    /// Configured Address Read Write
    /// A slave writes the data it has read to the EtherCAT datagram and writes the newly acquired data to the same memory area if its slave address matches one of the addresses configured in the datagram.
    FPRW,
    /// Broadcast Read
    /// All slaves write a logical OR of the data from the memory area and the data from the EtherCAT datagram to the EtherCAT datagram. All slaves increment the Position field.
    BRD,
    /// Broadcast Write
    /// All slaves write data to a memory area. All slaves increment the Position field.
    BWR,
    /// Broadcast Read Write
    /// All slaves write a logical OR of the data from the memory area and the data from the EtherCAT datagram to the EtherCAT datagram; all slaves write data to the memory area. BRW is typically not used. All slaves increment the Position field.
    BRW,
    /// Logical Memory Read
    /// A slave writes data it has read to the EtherCAT datagram if the address received matches one of the FMMU areas configured for reading.
    LRD,
    /// Logical Memory Write
    /// Slaves write data to their memory area if the address received matches one of the FMMU areas configured for writing.
    LWR,
    /// Logical Memory Read Write
    /// A slave writes data it has read to the EtherCAT datagram if the address received matches one of the FMMU areas configured for reading. Slaves write data to their memory area if the address received matches one of the FMMU areas configured for writing.
    LRW,
    /// Auto Increment Read Multiple Write
    /// A slave increments the Address field. A slave writes data it has read to the EtherCAT datagram when the address received is zero, otherwise it writes data to the memory area.
    ARMW,
    FRMW,
    Invalid,
}

impl CommandType {
    pub fn new(value: u8) -> Self {
        match value {
            0 => Self::NOP,
            1 => Self::APRD,
            2 => Self::APWR,
            3 => Self::APRW,
            4 => Self::FPRD,
            5 => Self::FPWR,
            6 => Self::FPRW,
            7 => Self::BRD,
            8 => Self::BWR,
            9 => Self::BRW,
            10 => Self::LRD,
            11 => Self::LWR,
            12 => Self::LRW,
            13 => Self::ARMW,
            14 => Self::FRMW,
            _ => Self::Invalid,
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash, Copy)]
pub enum MailboxErrorDetail {
    Syntax = 0x01,
    UnsupportedProtocol = 0x02,
    InvalidChannel = 0x03,
    ServiceNotSupported = 0x04,
    InvalidHeader = 0x05,
    SizeTooShort = 0x06,
    NoMoreMemory = 0x07,
    InvalidSize = 0x08,
    Unknown = 0x00,
}

impl From<u8> for MailboxErrorDetail {
    fn from(value: u8) -> Self {
        match value {
            1 => Self::Syntax,
            2 => Self::UnsupportedProtocol,
            3 => Self::InvalidChannel,
            4 => Self::ServiceNotSupported,
            5 => Self::InvalidHeader,
            6 => Self::SizeTooShort,
            7 => Self::NoMoreMemory,
            8 => Self::InvalidSize,
            _ => Self::Unknown,
        }
    }
}
