use crate::arch::*;
use crate::error::*;
use crate::frame::ethercat_frame::*;
use crate::master::*;
use crate::util::*;

const DC_RECV_TIMEOUT_NS: u64 = 1000_000;

// NOTE: ポート0がIN、ポート1がOUTとし、ライントポロジーとする。
// NOTE: すべてのスレーブがDCに対応しているとする。
// TODO: 上記を一般化する。
pub(crate) fn config_dc<
    B: AsRef<[u8]> + AsMut<[u8]>,
    R: RawPacketInterface,
    E: EtherCATSystemTime,
>(
    ethdev: &mut R,
    ec_packet: &mut EtherCATFrame<B>,
    recv_buffer: &mut [u8],
    slave_count: u16,
    num_drift_iter: usize,
) -> Result<(), Error> {
    set_dc_master_control::<_, _, E>(ethdev, ec_packet, recv_buffer, slave_count)?;

    set_dc_cycle_deactivation::<_, _, E>(ethdev, ec_packet, recv_buffer, slave_count)?;

    clear_dc::<_, _, E>(ethdev, ec_packet, recv_buffer, slave_count)?;

    set_dc_offset_and_delay::<_, _, E>(ethdev, ec_packet, recv_buffer, slave_count)?;

    set_dc_drift::<_, _, E>(ethdev, ec_packet, recv_buffer, slave_count, num_drift_iter)?;

    Ok(())
}

fn clear_dc<B: AsRef<[u8]> + AsMut<[u8]>, R: RawPacketInterface, E: EtherCATSystemTime>(
    ethdev: &mut R,
    ec_packet: &mut EtherCATFrame<B>,
    recv_buffer: &mut [u8],
    slave_count: u16,
) -> Result<(), Error> {
    let register = 0x0910;
    init_ec_packet(ec_packet);
    ec_packet.add_bwr(0, register, &[0; 32])?;
    send_ec_packet(ethdev, ec_packet)?;

    clear_buffer(recv_buffer);
    receive_packet_with_wkc_check::<_, E>(ethdev, recv_buffer, slave_count, DC_RECV_TIMEOUT_NS)?;
    Ok(())
}

fn set_dc_cycle_deactivation<
    B: AsRef<[u8]> + AsMut<[u8]>,
    R: RawPacketInterface,
    E: EtherCATSystemTime,
>(
    ethdev: &mut R,
    ec_packet: &mut EtherCATFrame<B>,
    recv_buffer: &mut [u8],
    slave_count: u16,
) -> Result<(), Error> {
    let register = 0x0981;
    init_ec_packet(ec_packet);
    ec_packet.add_bwr(0, register, &[0])?;
    send_ec_packet(ethdev, ec_packet)?;

    clear_buffer(recv_buffer);
    receive_packet_with_wkc_check::<_, E>(ethdev, recv_buffer, slave_count, DC_RECV_TIMEOUT_NS)?;
    Ok(())
}

fn set_dc_master_control<
    B: AsRef<[u8]> + AsMut<[u8]>,
    R: RawPacketInterface,
    E: EtherCATSystemTime,
>(
    ethdev: &mut R,
    ec_packet: &mut EtherCATFrame<B>,
    recv_buffer: &mut [u8],
    slave_count: u16,
) -> Result<(), Error> {
    let register = 0x0980;
    init_ec_packet(ec_packet);
    ec_packet.add_bwr(0, register, &[0])?;
    send_ec_packet(ethdev, ec_packet)?;

    clear_buffer(recv_buffer);
    receive_packet_with_wkc_check::<_, E>(ethdev, recv_buffer, slave_count, DC_RECV_TIMEOUT_NS)?;
    Ok(())
}

fn set_dc_offset_and_delay<
    B: AsRef<[u8]> + AsMut<[u8]>,
    R: RawPacketInterface,
    E: EtherCATSystemTime,
>(
    ethdev: &mut R,
    ec_packet: &mut EtherCATFrame<B>,
    recv_buffer: &mut [u8],
    slave_count: u16,
) -> Result<(), Error> {
    // フレームの到着したローカルタイムをラッチする。
    init_ec_packet(ec_packet);
    ec_packet.add_bwr(0, 0x0900, &[0])?;
    send_ec_packet(ethdev, ec_packet)?;
    let master_systime = E::system_time_from_2000_1_1_as_nanos();
    clear_buffer(recv_buffer);
    receive_packet_with_wkc_check::<_, E>(ethdev, recv_buffer, slave_count, DC_RECV_TIMEOUT_NS)?;

    let mut port0_local_times = [0; SLAVE_MAX];
    let mut port1_local_times = [0; SLAVE_MAX];
    let mut esc_local_time = [0; SLAVE_MAX];
    for _ in 0..16 {
        for i in 0..slave_count {
            {
                //Port0とPort1のラッチしたローカルタイムを取得する
                init_ec_packet(ec_packet);
                ec_packet.add_aprd(get_ap_adp(i), 0x0900, &[0; 32])?;
                send_ec_packet(ethdev, ec_packet)?;
                clear_buffer(recv_buffer);
                receive_packet_with_wkc_check::<_, E>(ethdev, recv_buffer, 1, DC_RECV_TIMEOUT_NS)?;
                let packet = EtherCATFrame::new(&recv_buffer)?;
                let offset = packet
                    .dlpdu_payload_offsets()
                    .next()
                    .ok_or(Error::SmallBuffer)?;
                //port0
                let local_time_port0 = u32::from_le_bytes([
                    packet.packet()[offset],
                    packet.packet()[offset + 1],
                    packet.packet()[offset + 2],
                    packet.packet()[offset + 3],
                ]);
                port0_local_times[i as usize] += local_time_port0 as u64;
                //port1
                let local_time_port0 = u32::from_le_bytes([
                    packet.packet()[offset + 4],
                    packet.packet()[offset + 5],
                    packet.packet()[offset + 6],
                    packet.packet()[offset + 7],
                ]);
                port1_local_times[i as usize] += local_time_port0 as u64;
            }

            {
                //ESCのラッチしたローカルタイムを取得する
                init_ec_packet(ec_packet);
                ec_packet.add_aprd(get_ap_adp(i), 0x0918, &[0; 8])?;
                send_ec_packet(ethdev, ec_packet)?;
                clear_buffer(recv_buffer);
                receive_packet_with_wkc_check::<_, E>(ethdev, recv_buffer, 1, DC_RECV_TIMEOUT_NS)?;
                let packet = EtherCATFrame::new(&recv_buffer)?;
                let offset = packet
                    .dlpdu_payload_offsets()
                    .next()
                    .ok_or(Error::SmallBuffer)?;
                let local_time_esc = u64::from_le_bytes([
                    packet.packet()[offset],
                    packet.packet()[offset + 1],
                    packet.packet()[offset + 2],
                    packet.packet()[offset + 3],
                    packet.packet()[offset + 4],
                    packet.packet()[offset + 5],
                    packet.packet()[offset + 6],
                    packet.packet()[offset + 7],
                ]);
                esc_local_time[i as usize] += local_time_esc as u128;
            }
        }
    }

    // 16回平均をとる。
    for i in 0..slave_count as usize {
        port0_local_times[i] /= 16;
        port1_local_times[i] /= 16;
        esc_local_time[i] /= 16;
    }

    // オフセットを設定する。
    for i in 0..slave_count {
        let offset = master_systime - esc_local_time[i as usize] as u64;
        init_ec_packet(ec_packet);
        ec_packet.add_apwr(get_ap_adp(i), 0x0920, &u64::to_le_bytes(offset as u64))?;
        send_ec_packet(ethdev, ec_packet)?;
        clear_buffer(recv_buffer);
        receive_packet_with_wkc_check::<_, E>(ethdev, recv_buffer, 1, DC_RECV_TIMEOUT_NS)?;
    }

    // 基準クロックからのディレイを設定する。
    let mut delay_sum = 0;
    for i in (0..slave_count).skip(1) {
        let parent_port0_time = port0_local_times[(i - 1) as usize];
        let parent_port1_time = port1_local_times[(i - 1) as usize];
        let port0_time = port0_local_times[i as usize];
        let port1_time = port1_local_times[i as usize];
        let parent_loop_delay = parent_port1_time - parent_port0_time;
        let mut loop_delay = 0;
        let mut compensation_value = 0;
        //中間のスレーブの場合
        if i + 1 != slave_count {
            loop_delay = port1_time - port0_time;
            compensation_value = 40;
        }
        let delay_from_parent = if loop_delay < parent_loop_delay {
            (parent_loop_delay - loop_delay + compensation_value) / 2
        } else {
            (loop_delay - parent_loop_delay + compensation_value) / 2
        };

        delay_sum += delay_from_parent;

        init_ec_packet(ec_packet);
        ec_packet.add_apwr(get_ap_adp(i), 0x0928, &u32::to_le_bytes(delay_sum as u32))?;
        send_ec_packet(ethdev, ec_packet)?;
        clear_buffer(recv_buffer);
        receive_packet_with_wkc_check::<_, E>(ethdev, recv_buffer, 1, DC_RECV_TIMEOUT_NS)?;
    }

    Ok(())
}

fn set_dc_drift<B: AsRef<[u8]> + AsMut<[u8]>, R: RawPacketInterface, E: EtherCATSystemTime>(
    ethdev: &mut R,
    ec_packet: &mut EtherCATFrame<B>,
    recv_buffer: &mut [u8],
    slave_count: u16,
    num_iter: usize,
) -> Result<(), Error> {
    for _ in 0..num_iter {
        init_ec_packet(ec_packet);
        ec_packet.add_armw(0, 0x0910, &[0; 8])?;
        send_ec_packet(ethdev, ec_packet)?;
        clear_buffer(recv_buffer);
        receive_packet_with_wkc_check::<_, E>(
            ethdev,
            recv_buffer,
            slave_count,
            DC_RECV_TIMEOUT_NS,
        )?;
    }
    Ok(())
}
