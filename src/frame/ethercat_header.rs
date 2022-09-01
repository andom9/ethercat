use bitfield::*;

const DST_MAC: u64 = 0x02_02_02_02_02_02;
pub(crate) const SRC_MAC: u64 = 0x01_01_01_01_01_01;
pub(crate) const WKC_LENGTH: usize = 2;
pub(crate) const ETHERCAT_TYPE: u16 = 0x88A4;

bitfield! {
    #[derive(Debug, Clone)]
    pub struct EthernetHeader(MSB0 [u8]);
    u64;
    pub destination, set_destination: 47, 0;
    pub source, set_source: 48+47, 48;
    u16;
    pub ether_type, set_ether_type: 48+47+1+15, 48+47+1;
}

impl EthernetHeader<[u8; 14]> {
    pub const SIZE: usize = 14;
    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}

impl<T: AsRef<[u8]> + AsMut<[u8]>> EthernetHeader<T> {
    pub fn set_ethercat_default(&mut self) {
        self.set_destination(DST_MAC);
        self.set_source(SRC_MAC);
        self.set_ether_type(ETHERCAT_TYPE);
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct EtherCatHeader([u8]);
    u16;
    pub length, set_length: 10, 0;
    u8;
    pub ethercat_type, set_ethercat_type: 15, 12;
}

impl EtherCatHeader<[u8; 2]> {
    pub const SIZE: usize = 2;
    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct EtherCatPduHeader([u8]);
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

impl EtherCatPduHeader<[u8; 10]> {
    pub const SIZE: usize = 10;
    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}

impl<T: AsRef<[u8]>> EtherCatPduHeader<T> {
    pub fn data(&self) -> &[u8] {
        &self.0.as_ref()[EtherCatPduHeader::SIZE..EtherCatPduHeader::SIZE + self.length() as usize]
    }

    pub fn wkc(&self) -> Option<u16> {
        let len = self.length() as usize;
        let low = self.0.as_ref().get(EtherCatPduHeader::SIZE + len)?;
        let high = self.0.as_ref().get(EtherCatPduHeader::SIZE + len + 1)?;
        Some(((*high as u16) << 8) | (*low as u16))
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct MailboxHeader([u8]);
    u16;
    pub length, set_length: 15, 0;
    pub address, set_address: 31, 16;
    u8;
    pub prioriry, set_prioriry: 39, 38;
    pub mailbox_type, set_mailbox_type: 43, 40;
    pub count, set_count: 46, 44;
}

impl MailboxHeader<[u8; 6]> {
    pub const SIZE: usize = 6;
    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash, Copy)]
pub enum MailboxType {
    Error = 0,
    AoE = 1,
    EoE = 2,
    CoE = 3,
    FoE = 4,
    SoE = 5,
    VoE = 0xf,
}

//pub const MAILBOX_ERROR_LENGTH: usize = 4;

bitfield! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct MailboxErrorResponse([u8]);
    u16;
    pub service_type, _: 15, 0;
    pub detail, _: 31, 16;
}

impl MailboxErrorResponse<[u8; 4]> {
    pub const SIZE: usize = 4;
    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}

impl<T: AsRef<[u8]>> MailboxErrorResponse<T> {
    pub fn error_detail(&self) -> MailboxErrorDetail {
        MailboxErrorDetail::from(self.detail())
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash, Copy)]
pub enum CommandType {
    /// No operation
    /// A slave ignores the command.
    NOP = 0x00,
    /// Auto Increment Read
    /// A slave increments the address. A slave writes the data it has read to the EtherCat datagram if the address received is zero.
    APRD,
    /// Auto Increment Write
    /// A slave increments the address. A slave writes data to a memory area if the address received is zero.
    APWR,
    /// Auto Increment Read Write
    /// A slave increments the address. A slave writes the data it has read to the EtherCat datagram and writes the newly acquired data to the same memory area if the received address is zero.
    APRW,
    /// Configured Address Read
    /// A slave writes the data it has read to the EtherCat datagram if its slave address matches one of the addresses configured in the datagram.
    FPRD,
    /// Configured Address Write
    /// A slave writes data to a memory area if its slave address matches one of the addresses configured in the datagram.
    FPWR,
    /// Configured Address Read Write
    /// A slave writes the data it has read to the EtherCat datagram and writes the newly acquired data to the same memory area if its slave address matches one of the addresses configured in the datagram.
    FPRW,
    /// Broadcast Read
    /// All slaves write a logical OR of the data from the memory area and the data from the EtherCat datagram to the EtherCat datagram. All slaves increment the Position field.
    BRD,
    /// Broadcast Write
    /// All slaves write data to a memory area. All slaves increment the Position field.
    BWR,
    /// Broadcast Read Write
    /// All slaves write a logical OR of the data from the memory area and the data from the EtherCat datagram to the EtherCat datagram; all slaves write data to the memory area. BRW is typically not used. All slaves increment the Position field.
    BRW,
    /// Logical Memory Read
    /// A slave writes data it has read to the EtherCat datagram if the address received matches one of the Fmmu areas configured for reading.
    LRD,
    /// Logical Memory Write
    /// Slaves write data to their memory area if the address received matches one of the Fmmu areas configured for writing.
    LWR,
    /// Logical Memory Read Write
    /// A slave writes data it has read to the EtherCat datagram if the address received matches one of the Fmmu areas configured for reading. Slaves write data to their memory area if the address received matches one of the Fmmu areas configured for writing.
    LRW,
    /// Auto Increment Read Multiple Write
    /// A slave increments the Address field. A slave writes data it has read to the EtherCat datagram when the address received is zero, otherwise it writes data to the memory area.
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
    Unspecified = 0x00,
}

impl From<u16> for MailboxErrorDetail {
    fn from(value: u16) -> Self {
        match value {
            1 => Self::Syntax,
            2 => Self::UnsupportedProtocol,
            3 => Self::InvalidChannel,
            4 => Self::ServiceNotSupported,
            5 => Self::InvalidHeader,
            6 => Self::SizeTooShort,
            7 => Self::NoMoreMemory,
            8 => Self::InvalidSize,
            _ => Self::Unspecified,
        }
    }
}