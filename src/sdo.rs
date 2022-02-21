use crate::arch::*;
use crate::error::*;
use crate::frame::coe::*;
use crate::frame::ethercat::*;
use crate::frame::ethercat_frame::*;
use crate::mailbox::*;
use heapless;

pub const SDO_MAX_DATA_LENGTH: usize =
    512 - MAILBOX_HEADER_LENGTH - COE_HEADER_LENGTH - SDO_HEADER_LENGTH;

pub(crate) fn write_sdo<
    B: AsRef<[u8]> + AsMut<[u8]>,
    R: RawPacketInterface,
    E: EtherCATSystemTime,
>(
    ethdev: &mut R,
    ec_packet: &mut EtherCATFrame<B>,
    recv_buffer: &mut [u8],
    slave_number: u16,
    station_addr: u16,
    send_data: &[u8],
    mailbox_count: u8,
    mailbox_timeout_millis: u64,
    sdo_index: u16,
    sdo_sub_index: u8,
) -> Result<(), Error> {
    const SIZE: usize = 512 - MAILBOX_HEADER_LENGTH;
    let mut buffer = heapless::Vec::<_, SIZE>::from_slice(
        &[0; COE_HEADER_LENGTH + SDO_HEADER_LENGTH + SDO_DATA_LENGTH],
    )
    .unwrap(); //[0; 512 - MAILBOX_HEADER_LENGTH];
    {
        let mut canopen = CANOpenPDU::new(&mut buffer).ok_or(Error::SmallBuffer)?;
        canopen.set_service_type(CANOpenServiceType::SDOReq as u8);
    }

    let data_len = send_data.len();
    {
        let mut sdo = SDO::new(&mut buffer[COE_HEADER_LENGTH..]).unwrap();
        sdo.set_index(sdo_index);
        sdo.set_sub_index(sdo_sub_index);
        if data_len <= 1 {
            sdo.set_command(SDOCommand::DownExpReq1 as u8);
            for i in 0..data_len {
                buffer[COE_HEADER_LENGTH + SDO_HEADER_LENGTH + i] = send_data[i];
            }
        } else if data_len <= 2 {
            sdo.set_command(SDOCommand::DownExpReq2 as u8);
            for i in 0..data_len {
                buffer[COE_HEADER_LENGTH + SDO_HEADER_LENGTH + i] = send_data[i];
            }
        } else if data_len <= 3 {
            sdo.set_command(SDOCommand::DownExpReq3 as u8);
            for i in 0..data_len {
                buffer[COE_HEADER_LENGTH + SDO_HEADER_LENGTH + i] = send_data[i];
            }
        } else if data_len <= 4 {
            sdo.set_command(SDOCommand::DownExpReq4 as u8);
            for i in 0..data_len {
                buffer[COE_HEADER_LENGTH + SDO_HEADER_LENGTH + i] = send_data[i];
            }
        } else {
            sdo.set_command(SDOCommand::DownNormalReq as u8);
            sdo.set_data(data_len as u32);
            for i in 0..data_len {
                buffer.push(send_data[i]).unwrap();
                //buffer[COE_HEADER_LENGTH + SDO_HEADER_LENGTH + SDO_DATA_LENGTH + i] = send_data[i];
            }
        }
    }
    mailbox::<_, _, E>(
        ethdev,
        ec_packet,
        recv_buffer,
        slave_number,
        station_addr,
        MailboxType::CoE,
        &buffer,
        mailbox_count,
        mailbox_timeout_millis,
    )?;
    check_sdo_res(recv_buffer)?;
    Ok(())
}

pub(crate) fn read_sdo<
    B: AsRef<[u8]> + AsMut<[u8]>,
    R: RawPacketInterface,
    E: EtherCATSystemTime,
>(
    ethdev: &mut R,
    ec_packet: &mut EtherCATFrame<B>,
    recv_buffer: &mut [u8],
    slave_number: u16,
    station_addr: u16,
    mailbox_count: u8,
    mailbox_timeout_millis: u64,
    sdo_index: u16,
    sdo_sub_index: u8,
) -> Result<heapless::Vec<u8, SDO_MAX_DATA_LENGTH>, Error> {
    let mut buffer = [0; COE_HEADER_LENGTH + SDO_HEADER_LENGTH + SDO_DATA_LENGTH];
    {
        let mut canopen = CANOpenPDU::new(&mut buffer).ok_or(Error::SmallBuffer)?;
        canopen.set_service_type(CANOpenServiceType::SDOReq as u8);
    }

    {
        let mut sdo = SDO::new(&mut buffer[COE_HEADER_LENGTH..]).unwrap();
        sdo.set_index(sdo_index);
        sdo.set_sub_index(sdo_sub_index);
        sdo.set_command(SDOCommand::UpReq as u8);
    }
    mailbox::<_, _, E>(
        ethdev,
        ec_packet,
        recv_buffer,
        slave_number,
        station_addr,
        MailboxType::CoE,
        &buffer,
        mailbox_count,
        mailbox_timeout_millis,
    )?;
    check_sdo_res(recv_buffer)
}

fn check_sdo_res(sdo_recv_buffer: &[u8]) -> Result<heapless::Vec<u8, SDO_MAX_DATA_LENGTH>, Error> {
    let res_packet = EtherCATFrame::new(sdo_recv_buffer)?;
    let mut buffer: heapless::Vec<u8, SDO_MAX_DATA_LENGTH> = heapless::Vec::new();

    let payload_offset = res_packet
        .dlpd0u_payload_offsets()
        .next()
        .ok_or(Error::SmallBuffer)?;
    //TODO: 初めに全体の長さチェックをする。

    {
        let mailbox =
            MailboxPDU::new(&sdo_recv_buffer[payload_offset..]).ok_or(Error::SmallBuffer)?;
        if mailbox.mailbox_type() != MailboxType::CoE as u8 {
            return Err(Error::UnexpectedMailbox(mailbox.mailbox_type()));
        }
    }

    {
        let sdo = SDO::new(
            &sdo_recv_buffer[payload_offset + MAILBOX_HEADER_LENGTH + COE_HEADER_LENGTH..],
        )
        .ok_or(Error::SmallBuffer)?;
        let command = sdo.command();
        if command == SDOCommand::Abort as u8 {
            let mut abort_code = 0_u32;
            for j in 0..4 {
                if let Some(data) = sdo_recv_buffer.get(
                    payload_offset
                        + MAILBOX_HEADER_LENGTH
                        + COE_HEADER_LENGTH
                        + SDO_HEADER_LENGTH
                        + j,
                ) {
                    abort_code |= (*data as u32) << (j * 8);
                } else {
                    continue;
                }
            }
            Err(Error::MailboxAbort(abort_code.into()))
        } else if command == SDOCommand::DownRes as u8 {
            Ok(buffer)
        } else if command == SDOCommand::UpExpRes1 as u8 {
            let d = sdo_recv_buffer
                .get(payload_offset + MAILBOX_HEADER_LENGTH + COE_HEADER_LENGTH + SDO_HEADER_LENGTH)
                .ok_or(Error::SmallBuffer)?;
            buffer.push(*d).unwrap();
            Ok(buffer)
        } else if command == SDOCommand::UpExpRes2 as u8 {
            for j in 0..2 {
                let d = sdo_recv_buffer
                    .get(
                        payload_offset
                            + MAILBOX_HEADER_LENGTH
                            + COE_HEADER_LENGTH
                            + SDO_HEADER_LENGTH
                            + j,
                    )
                    .ok_or(Error::SmallBuffer)?;
                buffer.push(*d).unwrap();
            }
            Ok(buffer)
        } else if command == SDOCommand::UpExpRes3 as u8 {
            for j in 0..3 {
                let d = sdo_recv_buffer
                    .get(
                        payload_offset
                            + MAILBOX_HEADER_LENGTH
                            + COE_HEADER_LENGTH
                            + SDO_HEADER_LENGTH
                            + j,
                    )
                    .ok_or(Error::SmallBuffer)?;
                buffer.push(*d).unwrap();
            }
            Ok(buffer)
        } else if command == SDOCommand::UpExpRes4 as u8 {
            for j in 0..4 {
                let d = sdo_recv_buffer
                    .get(
                        payload_offset
                            + MAILBOX_HEADER_LENGTH
                            + COE_HEADER_LENGTH
                            + SDO_HEADER_LENGTH
                            + j,
                    )
                    .ok_or(Error::SmallBuffer)?;
                buffer.push(*d).unwrap();
            }
            Ok(buffer)
        } else if command == SDOCommand::UpNormalRes as u8 {
            //TODO:多分間違ってる
            for j in 0..4 {
                let d = sdo_recv_buffer
                    .get(
                        payload_offset
                            + MAILBOX_HEADER_LENGTH
                            + COE_HEADER_LENGTH
                            + SDO_HEADER_LENGTH
                            + j,
                    )
                    .ok_or(Error::SmallBuffer)?;
                buffer.push(*d).unwrap();
            }
            Ok(buffer)
        } else {
            Err(Error::UnexpectedMailbox(command))
        }
    }
}
