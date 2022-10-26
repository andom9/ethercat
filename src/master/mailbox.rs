use std::process;

use crate::frame::{
    AbortCode, CoeHeader, CoeServiceType, Emmergency, MailboxErrorDetail, MailboxErrorResponse,
    MailboxHeader, MailboxType, SdoHeader,
};
use crate::interface::{PduSocket, RawEthernetDevice, SocketHandle, SocketInterface};
use crate::register::SyncManagerStatus;
use crate::slave::{Network, Slave, SlaveInfo};
use crate::task::{CyclicTask, EtherCatSystemTime, MailboxTask, MailboxTaskError, TaskError};
use crate::{interface::SlaveAddress, slave::Direction};

#[derive(Debug)]
pub(super) struct MailboxManager {
    task: MailboxTask,
    session: MailboxSessionId,
    slave_with_mailbox: Option<SlaveAddress>,
}

impl MailboxManager {
    pub fn new(task: MailboxTask) -> Self {
        Self {
            task,
            session: Default::default(),
            slave_with_mailbox: None,
        }
    }

    pub fn process_one_step(
        &mut self,
        network: &Network,
        mb_socket: &mut PduSocket,
        sys_time: EtherCatSystemTime,
    ) {
        if self.task.is_finished() {
            if let Some(slave_with_mailbox) = self.slave_with_mailbox {
                let (slave, _) = network.slave(slave_with_mailbox).unwrap();
                let tx_sm = slave.info().mailbox_tx_sm().unwrap();
                self.task.start_to_read(slave_with_mailbox, tx_sm, false);
            }
        }
        self.task.process_one_step(mb_socket, sys_time)
    }

    pub fn find_slave_with_mailbox(
        &mut self,
        network: &Network,
        logical_address_offset: u32,
        process_data_image: &[u8],
    ) {
        for (pos, (slave, _)) in network.slaves().enumerate() {
            if let Some(ref fmmu_config) = slave.fmmu_config()[2] {
                let tx_sm_number = slave.info().mailbox_tx_sm().unwrap().number();
                let mb_tx_sm_status = SyncManagerStatus::ADDRESS + 0x08 * tx_sm_number as u16;
                assert_eq!(mb_tx_sm_status, fmmu_config.physical_address());

                let mut buf = [0; SyncManagerStatus::SIZE];
                fmmu_config
                    .read_to_buffer(logical_address_offset, process_data_image, &mut buf)
                    .unwrap();
                let sm_status = SyncManagerStatus(buf);
                if sm_status.is_mailbox_full() {
                    self.slave_with_mailbox = Some(SlaveAddress::SlavePosition(pos as u16));
                }
            }
        }
        self.slave_with_mailbox = None;
    }

    pub fn try_get_mailbox_writer<'a, 'b, 'c>(
        &'a mut self,
        mb_socket: &'b mut PduSocket<'c>,
    ) -> Option<MailboxWriter<'a, 'b, 'c, 'a>> {
        if !self.task.is_finished() {
            return None;
        }

        let Self {
            ref mut task,
            ref mut session,
            ..
        } = self;

        Some(MailboxWriter {
            task,
            socket: mb_socket,
            session,
        })
    }

    pub fn message_from_same_session<'a>(
        &mut self,
        mb_socket: &'a mut PduSocket,
    ) -> Option<Result<(MailboxHeader<&'a [u8]>, &'a [u8]), TaskError<MailboxTaskError>>> {
        if self.session.mailbox_count == 0 || self.task.is_write_mode() {
            return None;
        }

        let _ = self.task.wait()?;
        let (header, data) = MailboxTask::mailbox_data(mb_socket.data_buf());
        if header.count() == self.session.mailbox_count
            && self.task.slave_address() == self.session.slave_address
        {
            Some(Ok((header, data)))
        } else {
            None
        }
    }

    pub fn message<'a>(
        &mut self,
        mb_socket: &'a mut PduSocket,
    ) -> Option<Result<(MailboxHeader<&'a [u8]>, &'a [u8]), TaskError<MailboxTaskError>>> {
        if self.task.is_write_mode() {
            return None;
        }
        let _ = self.task.wait()?;
        Some(Ok(MailboxTask::mailbox_data(mb_socket.data_buf())))
    }
}

#[derive(Debug)]
pub(super) struct MailboxWriter<'a, 'b, 'c, 'd> {
    task: &'a mut MailboxTask,
    socket: &'b mut PduSocket<'c>,
    session: &'d mut MailboxSessionId,
}

impl<'a, 'b, 'c, 'd> MailboxWriter<'a, 'b, 'c, 'd> {
    pub fn start_to_write(
        &mut self,
        slave_info: &SlaveInfo,
        mb_header: &MailboxHeader<[u8; MailboxHeader::SIZE]>,
        mb_data: &[u8],
    ) {
        self.session.mailbox_count = mb_header.count();
        MailboxTask::set_mailbox_data(mb_header, mb_data, self.socket.data_buf_mut());
        let addr = slave_info.slave_address();
        let rx_sm = slave_info.mailbox_rx_sm().unwrap();
        self.session.slave_address = addr;
        self.task.start_to_write(addr, rx_sm, false);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct MailboxSessionId {
    slave_address: SlaveAddress,
    mailbox_count: u8,
}

#[derive(Debug, Clone, Copy)]
pub struct LengthError;

pub fn dispatch_mailbox<'a>(
    mb_frame: &MailboxHeader<&'a [u8]>,
) -> Result<Mailbox<'a>, LengthError> {
    let coe_frame = mb_frame.data().ok_or(LengthError)?;
    let address = mb_frame.address();
    let mb_count = mb_frame.count();
    match mb_frame.mb_type() {
        MailboxType::Error => {
            let detail = MailboxErrorResponse(&coe_frame);
            detail
                .0
                .get(MailboxErrorResponse::SIZE - 1)
                .ok_or(LengthError)?;
            Ok(Mailbox::new(
                address,
                mb_count,
                Message::Error(detail.error_detail()),
            ))
        }
        MailboxType::AoE => Ok(Mailbox::new(address, mb_count, Message::Unsupported)),
        MailboxType::EoE => Ok(Mailbox::new(address, mb_count, Message::Unsupported)),
        MailboxType::CoE => {
            let sdo_frame = CoeHeader(coe_frame).data().ok_or(LengthError)?;
            let sdo_payload = SdoHeader(sdo_frame).data().ok_or(LengthError)?;
            let coe = match CoeHeader(coe_frame).coe_service_type() {
                CoeServiceType::Emmergency => {
                    sdo_payload.get(Emmergency::SIZE - 1).ok_or(LengthError)?;
                    CoE::Emmergency(Emmergency(&sdo_payload))
                }
                CoeServiceType::SdoReq => CoE::SdoReq,
                CoeServiceType::SdoRes => {
                    let sdo_header = SdoHeader(sdo_frame);

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

                                CoE::SdoRes(Sdo::Upload(&sdo_payload[..size]))

                            // normal
                            } else {
                                sdo_payload.get(4 - 1).ok_or(LengthError)?;

                                let mut complete_size = [0; 4];
                                let buf = &sdo_payload[..4];
                                complete_size.iter_mut().zip(buf).for_each(|(s, b)| *s = *b);
                                let size = u32::from_le_bytes(complete_size) as usize;

                                sdo_payload.get(size + 4 - 1).ok_or(LengthError)?;

                                CoE::SdoRes(Sdo::Upload(&sdo_payload[4..size + 4]))
                            }
                        }
                        // Download Response
                        3 => CoE::SdoRes(Sdo::DownLoad),
                        // Abort
                        4 => {
                            let mut abort_code = [0; 4];
                            sdo_header
                                .0
                                .get(SdoHeader::SIZE + 4 - 1)
                                .ok_or(LengthError)?;
                            abort_code
                                .iter_mut()
                                .zip(sdo_header.0.iter().skip(SdoHeader::SIZE))
                                .for_each(|(a_code, data)| *a_code = *data);
                            let abort_code = AbortCode::from(u32::from_le_bytes(abort_code));
                            CoE::SdoRes(Sdo::Abort(abort_code))
                        }
                        _ => CoE::SdoRes(Sdo::Other),
                    }
                }
                CoeServiceType::TxPdo => CoE::Unsupported,
                CoeServiceType::RxPdo => CoE::Unsupported,
                CoeServiceType::TxPdoRemoteReq => CoE::Unsupported,
                CoeServiceType::RxPdoRemoteReq => CoE::Unsupported,
                CoeServiceType::SdoInfo => CoE::Unsupported,
                CoeServiceType::Other => CoE::Unsupported,
            };
            Ok(Mailbox::new(address, mb_count, Message::CoE(coe)))
        }
        MailboxType::FoE => Ok(Mailbox::new(address, mb_count, Message::Unsupported)),
        MailboxType::SoE => Ok(Mailbox::new(address, mb_count, Message::Unsupported)),
        MailboxType::VoE => Ok(Mailbox::new(address, mb_count, Message::Unsupported)),
        MailboxType::Other => Ok(Mailbox::new(address, mb_count, Message::Unsupported)),
    }
}

#[derive(Debug)]
pub struct Mailbox<'a> {
    address: u16,
    mb_count: u8,
    message: Message<'a>,
}

impl<'a> Mailbox<'a> {
    fn new(address: u16, mb_count: u8, message: Message<'a>) -> Self {
        Self {
            address,
            mb_count,
            message,
        }
    }

    pub fn address(&self) -> u16 {
        self.address
    }

    pub fn mailbox_count(&self) -> u8 {
        self.mb_count
    }

    pub fn message(&self) -> &Message<'a> {
        &self.message
    }
}

#[derive(Debug)]
pub enum Message<'a> {
    Error(MailboxErrorDetail),
    CoE(CoE<'a>),
    Unsupported,
}

#[derive(Debug)]
pub enum CoE<'a> {
    Emmergency(Emmergency<&'a [u8]>),
    SdoReq,
    SdoRes(Sdo<'a>),
    Unsupported,
}

#[derive(Debug)]
pub enum Sdo<'a> {
    DownLoad,
    Upload(&'a [u8]),
    Abort(AbortCode),
    Other,
}
