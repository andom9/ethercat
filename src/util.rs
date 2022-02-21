use crate::arch::{EtherCATSystemTime, RawPacketInterface};
use crate::error::*;
use crate::frame::ethercat::*;
use crate::frame::ethercat_frame::*;

#[inline]
pub(crate) fn get_ap_adp(slave_number: u16) -> u16 {
    if slave_number == 0 {
        0
    } else {
        0xFFFF - (slave_number - 1)
    }
}

pub(crate) fn send_ec_packet<B: AsRef<[u8]> + AsMut<[u8]>, R: RawPacketInterface>(
    ethdec: &mut R,
    packet: &mut EtherCATFrame<B>,
) -> Result<(), Error> {
    let index = if packet.index == 0xFF { 0 } else { packet.index + 1 };
    if !ethdec.send(packet.packet()) {
        return Err(Error::UnableToSendPacket);
    }
    packet.init();
    packet.index = index;
    Ok(())
}

pub(crate) fn receive_packet<'a, R: RawPacketInterface, E: EtherCATSystemTime>(
    ethdev: &'a mut R,
    buffer: &mut [u8],
    timeout_ns: u64,
) -> Result<(), Error> {
    let start_time = E::system_time_from_2000_1_1_as_nanos();
    loop {
        if let Some(len) = ethdev.recv(buffer) {
            if let Some(packet) = EthernetHeader::new(&buffer[..len]) {
                //EtherCATパケットのみ受信
                if packet.ether_type() != ETHERCAT_TYPE {
                    continue;
                }
                //自分が送ったパケット以外を受信する。
                if packet.source() != 0x01_01_01_01_01_01 {
                    return Ok(());
                }
            } else {
                continue;
            }
        }
        if E::system_time_from_2000_1_1_as_nanos() - start_time > timeout_ns {
            return Err(Error::UnableToRecievePacket);
        }
    }
}

#[inline]
pub(crate) fn receive_packet_with_wkc_check<'a, R: RawPacketInterface, E: EtherCATSystemTime>(
    ethdev: &'a mut R,
    buffer: &mut [u8],
    num_slaves: u16,
    timeout_ns: u64,
) -> Result<(), Error> {
    receive_packet::<R, E>(ethdev, buffer, timeout_ns)?;
    check_wkc(buffer, num_slaves)
}

#[inline]
pub(crate) fn init_ec_packet<B: AsRef<[u8]> + AsMut<[u8]>>(ec_packet: &mut EtherCATFrame<B>) {
    let index = ec_packet.index;
    ec_packet.init();
    ec_packet.index = index;
}

// wkcを使ってスレーブの数を数える。
// エラーチェックができないので、複数回する方が良い。
pub(crate) fn slave_count<
    B: AsRef<[u8]> + AsMut<[u8]>,
    R: RawPacketInterface,
    E: EtherCATSystemTime,
>(
    ethdev: &mut R,
    ec_packet: &mut EtherCATFrame<B>,
    recv_buffer: &mut [u8],
    timeout_ns: u64,
) -> Result<u16, Error> {
    let mut slave_count = 0;
    for _ in 0..2 {
        init_ec_packet(ec_packet);
        ec_packet.add_brd(0, 0x0000, &[0])?;

        send_ec_packet(ethdev, ec_packet)?;

        clear_buffer(recv_buffer);
        if receive_packet::<R, E>(ethdev, recv_buffer, timeout_ns).is_err() {
            continue;
        }
        let ec_frame_recv = EtherCATFrame::new(recv_buffer.as_mut())?;
        let slave_count_i = {
            let dlpd0u = EtherCATPDU::new(
                &ec_frame_recv.packet()[(ETHERCAT_HEADER_LENGTH + ETHERNET_HEADER_LENGTH)..],
            )
            .ok_or(Error::SmallBuffer)?;
            dlpd0u.wkc().ok_or(Error::SmallBuffer)?
        };
        slave_count = slave_count.max(slave_count_i);
    }
    if slave_count == 0 {
        Err(Error::NotFoundSlaves)
    } else {
        Ok(slave_count)
    }
}

// wkcが正しいかどうか確認する。ただし、送信時のwkcは0であること。
pub(crate) fn check_wkc(recv_packet: &[u8], num_slaves: u16) -> Result<(), Error> {
    let recv_packet = EtherCATFrame::new(recv_packet)?;
    for offset in recv_packet.dlpd0u_header_offsets() {
        let dlpd0u_packet = EtherCATPDU::new_unchecked(&recv_packet.packet()[offset..]);
        let command = CommandType::new(dlpd0u_packet.command_type()).ok_or(Error::InvalidCommand)?;
        let wkc = dlpd0u_packet.wkc().ok_or(Error::SmallBuffer)?;
        match command {
            CommandType::NOP => continue,
            CommandType::APRD | CommandType::APWR | CommandType::FPRD | CommandType::FPWR => {
                if wkc == 1 {
                    continue;
                } else {
                    return Err(Error::WkcNeq(wkc, 1));
                }
            }
            CommandType::BRD
            | CommandType::BWR
            | CommandType::ARMW
            | CommandType::FRMW
            | CommandType::LRD
            | CommandType::LWR => {
                if wkc == num_slaves {
                    continue;
                } else {
                    return Err(Error::WkcNeq(wkc, num_slaves));
                }
            }
            CommandType::LRW => {
                if wkc == num_slaves * 3 {
                    continue;
                } else {
                    return Err(Error::WkcNeq(wkc, num_slaves));
                }
            }
            _ => {
                unimplemented!()
            }
        }
    }
    Ok(())
}

#[inline]
pub(crate) fn clear_buffer(buffer: &mut [u8]) {
    buffer.iter_mut().for_each(|d| *d = 0);
}
