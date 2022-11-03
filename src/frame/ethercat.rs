use bitfield::*;
use core::ops::Range;
use num_enum::FromPrimitive;

use super::{
    AbortCode, CoeFrame, CoeServiceType, EmmergencyFrame, SdoDownloadNormalRequestFrame, SdoFrame,
};

const DST_MAC: u64 = 0xFF_FF_FF_FF_FF_FF;
pub(crate) const SRC_MAC: u64 = 0x05_05_05_05_05_05;
pub(crate) const WKC_LENGTH: usize = 2;
pub(crate) const ETHERCAT_TYPE: u16 = 0x88A4;

pub(crate) const ETHERNET_FRAME_SIZE_WITHOUT_FCS: usize = 1514;
pub(crate) const MAX_ETHERCAT_DATAGRAM: usize =
    ETHERNET_FRAME_SIZE_WITHOUT_FCS - EthernetFrame::HEADER_SIZE - EtherCatFrame::HEADER_SIZE;
pub(crate) const MAX_PDU_DATAGRAM: usize =
    MAX_ETHERCAT_DATAGRAM - EtherCatPdu::HEADER_SIZE - WKC_LENGTH;

bitfield! {
    #[derive(Debug, Clone)]
    pub struct EthernetFrame(MSB0 [u8]);
    u64;
    pub destination, set_destination: 47, 0;
    pub source, set_source: 48+47, 48;
    u16;
    pub ether_type, set_ether_type: 48+47+1+15, 48+47+1;
}

impl EthernetFrame<[u8; 14]> {
    pub const HEADER_SIZE: usize = 14;
    pub fn new() -> Self {
        Self([0; Self::HEADER_SIZE])
    }
}

impl<T: AsRef<[u8]> + AsMut<[u8]>> EthernetFrame<T> {
    pub fn set_ethercat_default(&mut self) {
        self.set_destination(DST_MAC);
        self.set_source(SRC_MAC);
        self.set_ether_type(ETHERCAT_TYPE);
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct EtherCatFrame([u8]);
    u16;
    pub length, set_length: 10, 0;
    u8;
    pub ethercat_type, set_ethercat_type: 15, 12;
}

impl EtherCatFrame<[u8; 2]> {
    pub const HEADER_SIZE: usize = 2;
    pub fn new() -> Self {
        Self([0; Self::HEADER_SIZE])
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct EtherCatPdu([u8]);
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

impl EtherCatPdu<[u8; 10]> {
    pub const HEADER_SIZE: usize = 10;
    pub fn new() -> Self {
        Self([0; Self::HEADER_SIZE])
    }
}

impl<T: AsRef<[u8]>> EtherCatPdu<T> {
    pub fn wkc(&self) -> Option<u16> {
        let len = self.length() as usize;
        let low = self.0.as_ref().get(EtherCatPdu::HEADER_SIZE + len)?;
        let high = self.0.as_ref().get(EtherCatPdu::HEADER_SIZE + len + 1)?;
        Some(((*high as u16) << 8) | (*low as u16))
    }
}

impl<'a> EtherCatPdu<&'a [u8]> {
    pub fn without_header(&self) -> &'a [u8] {
        &self.0[EtherCatPdu::HEADER_SIZE..]
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct MailboxFrame([u8]);
    u16;
    pub length, set_length: 15, 0;
    pub address, set_address: 31, 16;
    u8;
    pub prioriry, set_prioriry: 39, 38;
    pub mailbox_type, set_mailbox_type: 43, 40;
    pub count, set_count: 46, 44;
}

impl MailboxFrame<[u8; 6]> {
    pub const HEADER_SIZE: usize = 6;
    pub fn new() -> Self {
        Self([0; Self::HEADER_SIZE])
    }
}

impl<B: AsRef<[u8]>> MailboxFrame<B> {
    pub fn mb_type(&self) -> MailboxType {
        self.mailbox_type().into()
    }
}

impl<B: AsMut<[u8]>> MailboxFrame<B> {
    pub fn set_mb_type(&mut self, mb_type: MailboxType) {
        self.set_mailbox_type(mb_type as u8)
    }
}

impl<'a> MailboxFrame<&'a [u8]> {
    pub fn without_header(&self) -> &'a [u8] {
        &self.0[MailboxFrame::HEADER_SIZE..]
    }
}

#[derive(Debug, Clone, Copy, FromPrimitive, PartialEq, Eq)]
#[repr(u8)]
pub enum MailboxType {
    Error = 0,
    AoE = 1,
    EoE = 2,
    CoE = 3,
    FoE = 4,
    SoE = 5,
    VoE = 0xf,
    #[num_enum(default)]
    Other,
}

bitfield! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct MailboxErrorFrame([u8]);
    u16;
    pub service_type, _: 15, 0;
    pub detail, _: 31, 16;
}

impl MailboxErrorFrame<[u8; 4]> {
    pub const SIZE: usize = 4;
    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}

impl<T: AsRef<[u8]>> MailboxErrorFrame<T> {
    pub fn error_detail(&self) -> MailboxErrorDetail {
        MailboxErrorDetail::from(self.detail())
    }
}

#[derive(Debug, Clone, Copy, FromPrimitive, PartialEq, Eq)]
#[repr(u8)]
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
    #[num_enum(default)]
    Invalid,
}

#[derive(Debug, Clone, Copy, FromPrimitive, PartialEq, Eq)]
#[repr(u16)]
pub enum MailboxErrorDetail {
    #[num_enum(default)]
    Unspecified,
    Syntax = 0x01,
    UnsupportedProtocol = 0x02,
    InvalidChannel = 0x03,
    ServiceNotSupported = 0x04,
    InvalidHeader = 0x05,
    SizeTooShort = 0x06,
    NoMoreMemory = 0x07,
    InvalidSize = 0x08,
}

impl<'a> MailboxFrame<&'a [u8]> {
    pub fn mailbox(&self) -> Result<Mailbox<'a>, LengthError> {
        let len = self.0.len();
        if len < MailboxFrame::HEADER_SIZE {
            return Err(LengthError);
        }
        let coe_frame = self.without_header();
        match self.mb_type() {
            MailboxType::Error => {
                let detail = MailboxErrorFrame(&coe_frame);
                detail
                    .0
                    .get(MailboxErrorFrame::SIZE - 1)
                    .ok_or(LengthError)?;
                Ok(
                    //Mailbox::new(
                    //address,
                    //mb_count,
                    Mailbox::Error(detail.error_detail()),
                ) //)
            }
            MailboxType::AoE => Ok(
                //Mailbox::new(
                //address,
                //mb_count,
                Mailbox::UnsupportedProtocol(MailboxType::AoE),
            ), //),
            MailboxType::EoE => Ok(
                //Mailbox::new(
                //address,
                //mb_count,
                Mailbox::UnsupportedProtocol(MailboxType::EoE),
            ), //),
            MailboxType::CoE => {
                if len < MailboxFrame::HEADER_SIZE + CoeFrame::HEADER_SIZE + SdoFrame::HEADER_SIZE {
                    return Err(LengthError);
                }
                let sdo_frame = CoeFrame(coe_frame).without_header();
                let sdo_payload = SdoFrame(sdo_frame).without_header();
                let index = SdoFrame(sdo_frame).index();
                let sub_index = SdoFrame(sdo_frame).sub_index();
                let coe = match CoeFrame(coe_frame).coe_service_type() {
                    CoeServiceType::Emmergency => {
                        sdo_payload
                            .get(EmmergencyFrame::SIZE - 1)
                            .ok_or(LengthError)?;
                        CoE::Emmergency(EmmergencyFrame(&sdo_payload))
                    }
                    CoeServiceType::SdoReq => {
                        let sdo_header = SdoFrame(sdo_frame);

                        match sdo_header.command_specifier() {
                            // Download Request
                            1 => {
                                // expedited
                                if sdo_header.transfer_type() {
                                    let size = match sdo_header.data_set_size() {
                                        0 => 4,
                                        1 => 3,
                                        2 => 2,
                                        3 => 1,
                                        _ => 0,
                                    };
                                    sdo_payload.get(size - 1).ok_or(LengthError)?;

                                    CoE::SdoReq(
                                        //SdoReq::new(
                                        //index,
                                        //sub_index,
                                        SdoReq::DownLoad(&sdo_payload[..size]),
                                    ) //)
                                      // normal
                                } else {
                                    sdo_payload.get(4 - 1).ok_or(LengthError)?;

                                    let mut complete_size = [0; 4];
                                    let buf = &sdo_payload[..4];
                                    complete_size.iter_mut().zip(buf).for_each(|(s, b)| *s = *b);
                                    let size = u32::from_le_bytes(complete_size) as usize;

                                    sdo_payload.get(size + 4 - 1).ok_or(LengthError)?;

                                    CoE::SdoReq(
                                        //SdoReq::new(
                                        //index,
                                        //sub_index,
                                        SdoReq::DownLoad(&sdo_payload[4..size + 4]),
                                    ) //)
                                }
                            }
                            // Upload Request
                            2 => CoE::SdoReq(SdoReq::Upload),
                            // Abort
                            4 => {
                                let mut abort_code = [0; 4];
                                sdo_header
                                    .0
                                    .get(SdoFrame::HEADER_SIZE + 4 - 1)
                                    .ok_or(LengthError)?;
                                abort_code
                                    .iter_mut()
                                    .zip(sdo_header.0.iter().skip(SdoFrame::HEADER_SIZE))
                                    .for_each(|(a_code, data)| *a_code = *data);
                                let abort_code = AbortCode::from(u32::from_le_bytes(abort_code));
                                CoE::SdoReq(
                                    //SdoReq::new(
                                    //index,
                                    //sub_index,
                                    SdoReq::Abort(abort_code),
                                ) //)
                            }
                            _ => CoE::SdoReq(
                                //SdoReq::new(
                                //index,
                                //sub_index,
                                SdoReq::Other(CommandSpecifier(sdo_header.command_specifier())),
                            ), //),
                        }
                    }
                    CoeServiceType::SdoRes => {
                        let sdo_header = SdoFrame(sdo_frame);
                        match sdo_header.command_specifier() {
                            // Upload Response
                            2 => {
                                // expedited
                                if sdo_header.transfer_type() {
                                    let size = match sdo_header.data_set_size() {
                                        0 => 4,
                                        1 => 3,
                                        2 => 2,
                                        3 => 1,
                                        _ => 0,
                                    };
                                    sdo_payload.get(size - 1).ok_or(LengthError)?;

                                    CoE::SdoRes(
                                        //SdoRes::new(
                                        //index,
                                        //sub_index,
                                        SdoRes::Upload(&sdo_payload[..size]),
                                    ) //)

                                // normal
                                } else {
                                    sdo_payload.get(4 - 1).ok_or(LengthError)?;

                                    let mut complete_size = [0; 4];
                                    let buf = &sdo_payload[..4];
                                    complete_size.iter_mut().zip(buf).for_each(|(s, b)| *s = *b);
                                    let size = u32::from_le_bytes(complete_size) as usize;

                                    sdo_payload.get(size + 4 - 1).ok_or(LengthError)?;

                                    CoE::SdoRes(
                                        //SdoRes::new(
                                        //index,
                                        //sub_index,
                                        SdoRes::Upload(&sdo_payload[4..size + 4]),
                                    ) //)
                                }
                            }
                            // Download Response
                            3 => CoE::SdoRes(SdoRes::DownLoad),
                            _ => CoE::SdoRes(
                                //SdoRes::new(
                                //index,
                                //sub_index,
                                SdoRes::Other(CommandSpecifier(sdo_header.command_specifier())),
                            ), //),
                        }
                    }
                    CoeServiceType::TxPdo => CoE::UnsupportedType(CoeServiceType::TxPdo),
                    CoeServiceType::RxPdo => CoE::UnsupportedType(CoeServiceType::RxPdo),
                    CoeServiceType::TxPdoRemoteReq => {
                        CoE::UnsupportedType(CoeServiceType::TxPdoRemoteReq)
                    }
                    CoeServiceType::RxPdoRemoteReq => {
                        CoE::UnsupportedType(CoeServiceType::RxPdoRemoteReq)
                    }
                    CoeServiceType::SdoInfo => CoE::UnsupportedType(CoeServiceType::SdoInfo),
                    CoeServiceType::Other => CoE::UnsupportedType(CoeServiceType::Other),
                };
                Ok(
                    //Mailbox::new(address, mb_count,
                    Mailbox::CoE((CoeIndex { index, sub_index }, coe)),
                ) //)
            }
            MailboxType::FoE => Ok(
                //Mailbox::new(
                //address,
                //mb_count,
                Mailbox::UnsupportedProtocol(MailboxType::FoE),
            ), //),
            MailboxType::SoE => Ok(
                //Mailbox::new(
                //address,
                //mb_count,
                Mailbox::UnsupportedProtocol(MailboxType::SoE),
            ), //),
            MailboxType::VoE => Ok(
                //Mailbox::new(
                //address,
                //mb_count,
                Mailbox::UnsupportedProtocol(MailboxType::VoE),
            ), //),
            MailboxType::Other => Ok(
                //Mailbox::new(
                //address,
                //mb_count,
                Mailbox::UnsupportedProtocol(MailboxType::Other),
            ), //),
        }
    }
}

impl<'a> MailboxFrame<&'a mut [u8]> {
    pub fn set_mailbox(&mut self, mailbox: &Mailbox) -> Result<(), LengthError> {
        match mailbox {
            Mailbox::Error(_) => {
                unimplemented!()
            }
            Mailbox::CoE((coe_index, coe)) => {
                self.set_mb_type(MailboxType::CoE);
                match coe {
                    CoE::Emmergency(_) => unimplemented!(),
                    CoE::SdoReq(sdo_req) => {
                        let mut coe_frame = CoeFrame(&mut self.0[MailboxFrame::HEADER_SIZE..]);
                        coe_frame.set_number(0);
                        coe_frame.set_coe_service_type(CoeServiceType::SdoReq);
                        let mut sdo_frame = SdoFrame(&mut coe_frame.0[CoeFrame::HEADER_SIZE..]);
                        let mailbox_payload_length = match sdo_req {
                            SdoReq::DownLoad(data) => {
                                // Download normal request
                                sdo_frame.set_complete_access(false);
                                sdo_frame.set_data_set_size(0);
                                sdo_frame.set_command_specifier(1); // download request
                                sdo_frame.set_transfer_type(false); // normal transfer
                                sdo_frame.set_size_indicator(true);
                                sdo_frame.set_index(coe_index.index);
                                sdo_frame.set_sub_index(coe_index.sub_index);
                                let mut download_frame = SdoDownloadNormalRequestFrame(
                                    &mut sdo_frame.0[SdoFrame::HEADER_SIZE..],
                                );
                                download_frame.set_complete_size(data.len() as u32);
                                let payload_length = CoeFrame::HEADER_SIZE
                                    + SdoFrame::HEADER_SIZE
                                    + SdoDownloadNormalRequestFrame::HEADER_SIZE
                                    + data.len();
                                assert!(payload_length <= u16::MAX as usize);
                                payload_length as u16
                            }
                            SdoReq::Upload => {
                                // Upload request
                                sdo_frame.set_complete_access(false);
                                sdo_frame.set_data_set_size(0);
                                sdo_frame.set_command_specifier(2); // upload request
                                sdo_frame.set_transfer_type(false);
                                sdo_frame.set_size_indicator(false);
                                sdo_frame.set_index(coe_index.index);
                                sdo_frame.set_sub_index(coe_index.sub_index);
                                let payload_length =
                                    CoeFrame::HEADER_SIZE + SdoFrame::HEADER_SIZE + 4;
                                payload_length as u16
                            }
                            SdoReq::Abort(_) => unimplemented!(),
                            SdoReq::Other(_) => unimplemented!(),
                        };
                        self.set_length(mailbox_payload_length);
                    }
                    CoE::SdoRes(_) => unimplemented!(),
                    CoE::UnsupportedType(_) => unimplemented!("Unsupported CoE service type"),
                }
            }
            Mailbox::UnsupportedProtocol(_) => unimplemented!("Unsupported mailbox protocol"),
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum Mailbox<'a> {
    Error(MailboxErrorDetail),
    CoE((CoeIndex, CoE<'a>)),
    UnsupportedProtocol(MailboxType),
}

impl<'a> Mailbox<'a> {
    pub fn new_sdo_download_request(index: u16, sub_index: u8, data: &'a [u8]) -> Self {
        let sdo_req = SdoReq::DownLoad(data);
        Self::CoE((CoeIndex { index, sub_index }, CoE::SdoReq(sdo_req)))
    }

    pub fn new_sdo_upload_request(index: u16, sub_index: u8) -> Self {
        let sdo_req = SdoReq::Upload;
        Self::CoE((CoeIndex { index, sub_index }, CoE::SdoReq(sdo_req)))
    }

    pub fn sdo_upload_response(&self) -> Option<&[u8]> {
        match self {
            Mailbox::CoE((_, coe)) => match coe {
                CoE::SdoRes(sdo_res) => match sdo_res {
                    SdoRes::Upload(data) => Some(data),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        }
    }

    pub fn is_sdo_download_response(&self) -> bool {
        match self {
            Mailbox::CoE((_, coe)) => match coe {
                CoE::SdoRes(sdo_res) => match sdo_res {
                    SdoRes::DownLoad => true,
                    _ => false,
                },
                _ => false,
            },
            _ => false,
        }
    }
}

#[derive(Debug)]
pub enum CoE<'a> {
    Emmergency(EmmergencyFrame<&'a [u8]>),
    SdoReq(SdoReq<'a>),
    SdoRes(SdoRes<'a>),
    UnsupportedType(CoeServiceType),
}

#[derive(Debug)]
pub enum SdoRes<'a> {
    DownLoad,
    Upload(&'a [u8]),
    Other(CommandSpecifier),
}

#[derive(Debug)]
pub enum SdoReq<'a> {
    DownLoad(&'a [u8]),
    Upload,
    Abort(AbortCode),
    Other(CommandSpecifier),
}

#[derive(Debug, Clone, Copy)]
pub struct LengthError;

#[derive(Debug, Clone, Copy)]
pub struct CommandSpecifier(u8);

#[derive(Debug, Clone, Copy)]
pub struct CoeIndex {
    pub index: u16,
    pub sub_index: u8,
}
