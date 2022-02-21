use crate::arch::*;
use crate::error::*;
use crate::frame::ethercat_frame::*;
use crate::util::*;

const AL_RECV_TIMEOUT_NS: u64 = 1000_000_000;

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash, Copy)]
pub enum AlState {
    Init = 0x1,
    PreOperational = 0x2,
    Bootstrap = 0x3,
    SafeOperational = 0x4,
    Operational = 0x8,
    Invalid,
}

impl From<u16> for AlState {
    fn from(v: u16) -> Self {
        if v == AlState::Init as u16 {
            AlState::Init
        } else if v == AlState::PreOperational as u16 {
            AlState::PreOperational
        } else if v == AlState::Bootstrap as u16 {
            AlState::Bootstrap
        } else if v == AlState::SafeOperational as u16 {
            AlState::SafeOperational
        } else if v == AlState::PreOperational as u16 {
            AlState::PreOperational
        } else if v == AlState::Operational as u16 {
            AlState::Operational
        } else {
            AlState::Invalid
        }
    }
}

pub(crate) fn change_al_state<
    B: AsRef<[u8]> + AsMut<[u8]>,
    R: RawPacketInterface,
    E: EtherCATSystemTime,
>(
    ethdev: &mut R,
    ec_packet: &mut EtherCATFrame<B>,
    recv_buffer: &mut [u8],
    slave_numbers: &[u16],
    state: AlState,
    timeout_ns: u64,
) -> Result<(), Error> {
    for slave_number in slave_numbers {
        request_al_states::<_, _, E>(ethdev, ec_packet, recv_buffer, *slave_number, state)?;
    }
    for slave_number in slave_numbers {
        wait_al_state_transition::<_, _, E>(
            ethdev,
            ec_packet,
            recv_buffer,
            *slave_number,
            state,
            timeout_ns,
        )?;
    }
    Ok(())
}

fn request_al_states<B: AsRef<[u8]> + AsMut<[u8]>, R: RawPacketInterface, E: EtherCATSystemTime>(
    ethdev: &mut R,
    ec_packet: &mut EtherCATFrame<B>,
    recv_buffer: &mut [u8],
    slave_number: u16,
    state: AlState,
) -> Result<(), Error> {
    let al_control_register = 0x0120;
    //let data = [(state as u8) | 0b1_0000];
    let data = [state as u8];

    init_ec_packet(ec_packet);
    ec_packet.add_apwr(get_ap_adp(slave_number), al_control_register, &data)?;
    send_ec_packet(ethdev, ec_packet)?;

    clear_buffer(recv_buffer);
    receive_packet_with_wkc_check::<_, E>(ethdev, recv_buffer, 1, AL_RECV_TIMEOUT_NS)
}

fn read_al_states<B: AsRef<[u8]> + AsMut<[u8]>, R: RawPacketInterface, E: EtherCATSystemTime>(
    ethdev: &mut R,
    ec_packet: &mut EtherCATFrame<B>,
    recv_buffer: &mut [u8],
    slave_number: u16,
    check_error_code: bool,
) -> Result<AlState, Error> {
    let register = 0x0130;
    let data = [0; 6];

    init_ec_packet(ec_packet);
    ec_packet.add_aprd(get_ap_adp(slave_number), register, &data)?;
    send_ec_packet(ethdev, ec_packet)?;

    clear_buffer(recv_buffer);
    receive_packet_with_wkc_check::<_, E>(ethdev, recv_buffer, 1, AL_RECV_TIMEOUT_NS)?;
    let recieve_packet = EtherCATFrame::new(recv_buffer)?;
    let offset = recieve_packet
        .dlpdu_payload_offsets()
        .next()
        .ok_or(Error::SmallBuffer)?;
    let data = recieve_packet
        .packet()
        .get(offset)
        .ok_or(Error::SmallBuffer)?;
    let slave_state = data & 0b0000_1111;

    if check_error_code && (data & 0b1_0000) != 0 {
        let low = recieve_packet
            .packet()
            .get(offset + 4)
            .ok_or(Error::SmallBuffer)?;
        let high = recieve_packet
            .packet()
            .get(offset + 5)
            .ok_or(Error::SmallBuffer)?;
        let error_code = ((*high as u16) << 8) | (*low as u16);
        return Err(Error::ALStateTransfer(
            error_code,
            AlState::from(slave_state as u16),
        ));
    }

    Ok(AlState::from(slave_state as u16))
}

fn wait_al_state_transition<
    B: AsRef<[u8]> + AsMut<[u8]>,
    R: RawPacketInterface,
    E: EtherCATSystemTime,
>(
    ethdev: &mut R,
    ec_packet: &mut EtherCATFrame<B>,
    recv_buffer: &mut [u8],
    slave_number: u16,
    state: AlState,
    timeout_ns: u64,
) -> Result<(), Error> {
    let start_time = E::system_time_from_2000_1_1_as_nanos();
    while (read_al_states::<_, _, E>(ethdev, ec_packet, recv_buffer, slave_number, false)?) != state
    {
        if E::system_time_from_2000_1_1_as_nanos() - start_time >= timeout_ns {
            if (read_al_states::<_, _, E>(ethdev, ec_packet, recv_buffer, slave_number, true)?)
                == state
            {
                break;
            }
            return Err(Error::ALStateTimeout(timeout_ns, state));
        }
    }
    Ok(())
}
