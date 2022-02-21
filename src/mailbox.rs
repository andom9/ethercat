use crate::arch::*;
use crate::error::*;
use crate::frame::ethercat::*;
use crate::frame::ethercat_frame::*;
use crate::master::*;
use crate::util::*;

//MEMO: Mailbox Counterはどう決めるのがベストか？

const MB_RECV_TIMEOUT_NS: u64 = 1000_000_000;

pub(crate) fn mailbox<
    B: AsRef<[u8]> + AsMut<[u8]>,
    R: RawPacketInterface,
    E: EtherCATSystemTime,
>(
    ethdev: &mut R,
    ec_packet: &mut EtherCATFrame<B>,
    recv_buffer: &mut [u8],
    slave_number: u16,
    station_addr: u16,
    mailbox_type: MailboxType,
    send_data: &[u8],
    mailbox_count: u8,
    mailbox_timeout_millis: u64,
) -> Result<(), Error> {
    while is_sm1_mailbox_full::<_, _, E>(ethdev, ec_packet, recv_buffer, slave_number)? {
        receive_mailbox::<_, _, E>(ethdev, ec_packet, recv_buffer, slave_number)?;
    }

    wait_sm0_mailbox_empty::<_, _, E>(
        ethdev,
        ec_packet,
        recv_buffer,
        slave_number,
        mailbox_timeout_millis,
    )?;
    send_mailbox::<_, _, E>(
        ethdev,
        ec_packet,
        recv_buffer,
        slave_number,
        station_addr,
        mailbox_type,
        send_data,
        mailbox_count,
    )?;

    loop {
        wait_sm1_mailbox_full::<_, R, E>(
            ethdev,
            ec_packet,
            recv_buffer,
            slave_number,
            mailbox_timeout_millis,
        )?;
        receive_mailbox::<_, _, E>(ethdev, ec_packet, recv_buffer, slave_number)?;

        let res_packet = EtherCATFrame::new(&recv_buffer)?;
        let payload_offset = res_packet
            .dlpdu_payload_offsets()
            .next()
            .ok_or(Error::SmallBuffer)?;
        let recv_buffer = res_packet.drop();
        if recv_buffer
            .get(payload_offset + MAILBOX_HEADER_LENGTH)
            .is_none()
        {
            return Err(Error::SmallBuffer);
        }

        let mailbox = MailboxPDU::new(&recv_buffer[payload_offset..]).ok_or(Error::SmallBuffer)?;
        if mailbox.mailbox_type() == MailboxType::Error as u8 {
            let error = MailboxError::new(&recv_buffer[payload_offset + MAILBOX_HEADER_LENGTH..])
                .ok_or(Error::SmallBuffer)?;
            let detail = (error.detail() & 0xFF) as u8;
            return Err(Error::MailboxError(detail.into()));
        }

        if mailbox.count() == mailbox_count {
            break;
        }
    }
    Ok(())
}

fn send_mailbox<B: AsRef<[u8]> + AsMut<[u8]>, R: RawPacketInterface, E: EtherCATSystemTime>(
    ethdev: &mut R,
    ec_packet: &mut EtherCATFrame<B>,
    recv_buffer: &mut [u8],
    slave_number: u16,
    station_addr: u16,
    mailbox_type: MailboxType,
    data: &[u8],
    mailbox_count: u8,
) -> Result<(), Error> {
    assert!((1..=7).contains(&mailbox_count));

    init_ec_packet(ec_packet);

    let mailbox_ado = SM0_START_ADDRESS; //sm0の設定
    let mut mailbox_datagram = [0; 512]; //512であること。
    let data_len = data.len();
    if data_len > 512 - MAILBOX_HEADER_LENGTH {
        return Err(Error::MaxMailboxLength);
    }
    {
        let mut mailbox = MailboxPDU::new(&mut mailbox_datagram).unwrap();
        mailbox.set_length(data_len as u16);
        mailbox.set_address(station_addr);
        mailbox.set_prioriry(0);
        mailbox.set_mailbox_type(mailbox_type as u8);
        mailbox.set_count(mailbox_count);
    }
    for (i, d) in data.iter().enumerate() {
        mailbox_datagram[MAILBOX_HEADER_LENGTH + i] = *d;
    }

    ec_packet.add_apwr(get_ap_adp(slave_number), mailbox_ado, &mailbox_datagram)?;
    send_ec_packet(ethdev, ec_packet)?;

    clear_buffer(recv_buffer);
    receive_packet_with_wkc_check::<R, E>(ethdev, recv_buffer, 1, MB_RECV_TIMEOUT_NS)
}

fn receive_mailbox<B: AsRef<[u8]> + AsMut<[u8]>, R: RawPacketInterface, E: EtherCATSystemTime>(
    ethdev: &mut R,
    ec_packet: &mut EtherCATFrame<B>,
    recv_buffer: &mut [u8],
    slave_number: u16,
) -> Result<(), Error> {
    let mailbox_ado = SM1_START_ADDRESS;
    init_ec_packet(ec_packet);
    ec_packet.add_aprd(get_ap_adp(slave_number), mailbox_ado, &[0; 512])?;
    send_ec_packet(ethdev, ec_packet)?;

    clear_buffer(recv_buffer);
    receive_packet_with_wkc_check::<R, E>(ethdev, recv_buffer, 1, MB_RECV_TIMEOUT_NS)
}

fn is_sm0_mailbox_empty<
    B: AsRef<[u8]> + AsMut<[u8]>,
    R: RawPacketInterface,
    E: EtherCATSystemTime,
>(
    ethdev: &mut R,
    ec_packet: &mut EtherCATFrame<B>,
    recv_buffer: &mut [u8],
    slave_number: u16,
) -> Result<bool, Error> {
    let sm0_register = 0x0805;
    init_ec_packet(ec_packet);
    ec_packet.add_aprd(get_ap_adp(slave_number), sm0_register, &[0; 2])?;
    send_ec_packet(ethdev, ec_packet)?;

    clear_buffer(recv_buffer);
    receive_packet::<R, E>(ethdev, recv_buffer, MB_RECV_TIMEOUT_NS)?;
    check_wkc(recv_buffer, 1)?;

    let recieve_packet = EtherCATFrame::new(recv_buffer)?;
    let payload_offset = recieve_packet
        .dlpdu_payload_offsets()
        .next()
        .ok_or(Error::SmallBuffer)?;
    let mut data = [0; 2];
    for j in 0..2 {
        data[j] = *recieve_packet
            .packet()
            .as_ref()
            .get(payload_offset + j)
            .ok_or(Error::SmallBuffer)?;
    }
    let sm_enable = data[1] & 1;
    if sm_enable == 0 {
        return Err(Error::MailboxDisable);
    }

    Ok((data[0] & 0b1000) == 0)
}

fn is_sm1_mailbox_full<
    B: AsRef<[u8]> + AsMut<[u8]>,
    R: RawPacketInterface,
    E: EtherCATSystemTime,
>(
    ethdev: &mut R,
    ec_packet: &mut EtherCATFrame<B>,
    recv_buffer: &mut [u8],
    slave_number: u16,
) -> Result<bool, Error> {
    let sm1_register = 0x080D;
    init_ec_packet(ec_packet);
    ec_packet.add_aprd(get_ap_adp(slave_number), sm1_register, &[0; 2])?;
    send_ec_packet(ethdev, ec_packet)?;

    clear_buffer(recv_buffer);
    receive_packet::<R, E>(ethdev, recv_buffer, MB_RECV_TIMEOUT_NS)?;
    check_wkc(recv_buffer, 1)?;
    let recieve_packet = EtherCATFrame::new(recv_buffer)?;
    let payload_offset = recieve_packet
        .dlpdu_payload_offsets()
        .next()
        .ok_or(Error::SmallBuffer)?;
    let mut data = [0; 2];
    for j in 0..2 {
        data[j] = *recieve_packet
            .packet()
            .as_ref()
            .get(payload_offset + j)
            .ok_or(Error::SmallBuffer)?;
    }
    let sm_enable = data[1] & 1;
    if sm_enable == 0 {
        return Err(Error::MailboxDisable);
    }

    Ok((data[0] & 0b1000) != 0)
}

fn wait_sm0_mailbox_empty<
    B: AsRef<[u8]> + AsMut<[u8]>,
    R: RawPacketInterface,
    E: EtherCATSystemTime,
>(
    ethdev: &mut R,
    ec_packet: &mut EtherCATFrame<B>,
    recv_buffer: &mut [u8],
    slave_number: u16,
    max_attempt: u64,
) -> Result<(), Error> {
    let mut iter = 0;
    while !is_sm0_mailbox_empty::<_, _, E>(ethdev, ec_packet, recv_buffer, slave_number)? {
        if iter >= max_attempt {
            return Err(Error::MailboxTimeout(max_attempt));
        }
        iter += 1;
    }
    Ok(())
}

fn wait_sm1_mailbox_full<
    B: AsRef<[u8]> + AsMut<[u8]>,
    R: RawPacketInterface,
    E: EtherCATSystemTime,
>(
    ethdev: &mut R,
    ec_packet: &mut EtherCATFrame<B>,
    recv_buffer: &mut [u8],
    slave_number: u16,
    max_attempt: u64,
) -> Result<(), Error> {
    let mut iter = 0;
    while !is_sm1_mailbox_full::<_, _, E>(ethdev, ec_packet, recv_buffer, slave_number)? {
        if iter >= max_attempt {
            return Err(Error::MailboxTimeout(max_attempt));
        }
        iter += 1;
    }
    Ok(())
}
