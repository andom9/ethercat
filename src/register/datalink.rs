use bit_field::*;
use bitfield::*;

bitfield! {
    #[derive(Debug, Clone)]
    pub struct DLInformation([u8]);
    pub u8, ethercat_type, _: 8*1-1, 8*0;
    pub u8, revision, _: 8*2-1, 8*1;
    pub u16, build_number, _: 8*4-1, 8*2;
    /// Number of supported FMMU entities
    pub u8, number_of_supported_fmmu_entities, _: 8*5-1, 8*4;
    /// Number of supported Sync Manager channels
    pub u8, number_of_supported_sm_channels, _: 8*6-1, 8*5;
    pub u8, ram_size, _: 8*7-1, 8*6;
    /// FMMU bit operation not supported
    pub fmmu_bit_operation_not_supported, _: 8*8;
    /// DC supported
    pub dc_supported, _: 8*8+2;
    /// DC range. If true, 64bit.
    pub dc_range, _: 8*8+3;
    /// Low Jitter EBUS
    pub low_jitter_ebus, _: 8*8+4;
    /// Enhanced Link Detection EBUS
    pub enhanced_link_detection_ebus, _: 8*8+5;
    /// Enhances Link Detection MII
    pub enhanced_link_detection_mii, _: 8*8+6;
    /// Separate Handling of FCS errors
    pub separate_handling_of_fcs_errors, _: 8*8+7;
    /// LRW not supported
    pub not_lrw_supported, _: 8*9+1;
    /// BRW, APWR, FPRW not supported
    pub not_bafrw_supported, _: 8*9+2;
    /// Special FMMU Sync Manager configuration.
    /// FMMU0: RxPDO,
    /// FMMU1: TxPDO,
    /// FMMU2: Sync Manager1,
    /// Sync Manager0: Write Mailbox,
    /// Sync Manager1: Read Mailbox,
    /// Sync Manager2: rx data buffer,
    /// Sync Manager3: tx data buffer,
    pub is_special_fmmu_sm_configuration, _: 8*9+3;
}

impl<B: AsRef<[u8]>> DLInformation<B> {
    pub const ADDRESS: u16 = 0x0000;
    pub const SIZE: usize = 10;

    pub fn new(buf: B) -> Option<Self> {
        if buf.as_ref().len() < Self::SIZE.into() {
            None
        } else {
            Some(Self(buf))
        }
    }

    pub fn port0_type(&self) -> Option<PortPhysics> {
        let byte = self.0.as_ref()[7];
        match byte.get_bits(0..2) {
            0b10 => Some(PortPhysics::EBUS),
            0b11 => Some(PortPhysics::MII),
            _ => None,
        }
    }

    pub fn port1_type(&self) -> Option<PortPhysics> {
        let byte = self.0.as_ref()[7];
        match byte.get_bits(2..4) {
            0b10 => Some(PortPhysics::EBUS),
            0b11 => Some(PortPhysics::MII),
            _ => None,
        }
    }

    pub fn port2_type(&self) -> Option<PortPhysics> {
        let byte = self.0.as_ref()[7];
        match byte.get_bits(4..6) {
            0b10 => Some(PortPhysics::EBUS),
            0b11 => Some(PortPhysics::MII),
            _ => None,
        }
    }

    pub fn port3_type(&self) -> Option<PortPhysics> {
        let byte = self.0.as_ref()[7];
        match byte.get_bits(6..8) {
            0b10 => Some(PortPhysics::EBUS),
            0b11 => Some(PortPhysics::MII),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum PortPhysics {
    MII,
    EBUS,
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct FixedStationAddress([u8]);
    pub u8, configured_station_address, set_configured_station_address: 8*1-1, 8*0;
    pub u8, configured_station_alias, set_configured_station_alias: 8*2-1, 8*1;
}

impl<B: AsRef<[u8]>> FixedStationAddress<B> {
    pub const ADDRESS: u16 = 0x0010;
    pub const SIZE: usize = 2;

    pub fn new(buf: B) -> Option<Self> {
        if buf.as_ref().len() < Self::SIZE.into() {
            None
        } else {
            Some(Self(buf))
        }
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct DLControl([u8]);
    pub forwarding_rule, set_forwarding_rule: 0;
    pub u8, loop_control_port0, set_loop_control_port0: 8*1+1, 8*1;
    pub u8, loop_control_port1, set_loop_control_port1: 8*1+3, 8*1+2;
    pub u8, loop_control_port2, set_loop_control_port2: 8*1+5, 8*1+4;
    pub u8, loop_control_port3, set_loop_control_port3: 8*1+7, 8*1+6;
    pub u8, tx_buffer_size, set_tx_buffer_size: 8*2+2, 8*2;
    pub enable_alias_address, set_enable_alias_address: 8*3;
}

impl<B: AsRef<[u8]>> DLControl<B> {
    pub const ADDRESS: u16 = 0x0100;
    pub const SIZE: usize = 4;

    pub fn new(buf: B) -> Option<Self> {
        if buf.as_ref().len() < Self::SIZE.into() {
            None
        } else {
            Some(Self(buf))
        }
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct DLStatus([u8]);
    pub pdi_operational, _: 0;
    pub dls_user_watch_dog_status, _: 1;
    pub extended_link_detection, _: 2;
    pub link_status_port0, _: 4;
    pub link_status_port1, _: 5;
    pub link_status_port2, _: 6;
    pub link_status_port3, _: 7;
    pub loop_status_port0, _: 8*1;
    pub signal_detection_port0, _: 8*1 + 1;
    pub loop_status_port1, _: 8*1+2;
    pub signal_detection_port1, _: 8*1 + 3;
    pub loop_status_port2, _: 8*1+4;
    pub signal_detection_port2, _: 8*1 + 5;
    pub loop_status_port3, _: 8*1+6;
    pub signal_detection_port3, _: 8*1 + 7;
}

impl<B: AsRef<[u8]>> DLStatus<B> {
    pub const ADDRESS: u16 = 0x0110;
    pub const SIZE: usize = 2;

    pub fn new(buf: B) -> Option<Self> {
        if buf.as_ref().len() < Self::SIZE.into() {
            None
        } else {
            Some(Self(buf))
        }
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct RxErrorCounter([u8]);
    pub u8, frame_error_count_port0, set_frame_error_count_port0: 8*1-1, 8*0;
    pub u8, phy_error_count_port0, set_phy_error_count_port0: 8*2-1, 8*1;
    pub u8, frame_error_count_port1, set_frame_error_count_port1: 8*3-1, 8*2;
    pub u8, phy_error_count_port1, set_phy_error_count_port1: 8*4-1, 8*3;
    pub u8, frame_error_count_port2, set_frame_error_count_port2: 8*5-1, 8*4;
    pub u8, phy_error_count_port2, set_phy_error_count_port2: 8*6-1, 8*5;
    pub u8, frame_error_count_port3, set_frame_error_count_port3: 8*7-1, 8*6;
    pub u8, phy_error_count_port3, set_phy_error_count_port3: 8*8-1, 8*7;
}

impl<B: AsRef<[u8]>> RxErrorCounter<B> {
    pub const ADDRESS: u16 = 0x0300;
    pub const SIZE: usize = 8;

    pub fn new(buf: B) -> Option<Self> {
        if buf.as_ref().len() < Self::SIZE.into() {
            None
        } else {
            Some(Self(buf))
        }
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct WatchDogDivider([u8]);
    pub u8, watch_dog_divider, set_watch_dog_divider: 8*2-1, 8*0;
}

impl<B: AsRef<[u8]>> WatchDogDivider<B> {
    pub const ADDRESS: u16 = 0x0400;
    pub const SIZE: usize = 2;

    pub fn new(buf: B) -> Option<Self> {
        if buf.as_ref().len() < Self::SIZE.into() {
            None
        } else {
            Some(Self(buf))
        }
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct DLUserWatchDog([u8]);
    pub u8, dls_user_watch_dog, set_dls_user_watch_dog: 8*2-1, 8*0;
}

impl<B: AsRef<[u8]>> DLUserWatchDog<B> {
    pub const ADDRESS: u16 = 0x0410;
    pub const SIZE: usize = 2;

    pub fn new(buf: B) -> Option<Self> {
        if buf.as_ref().len() < Self::SIZE.into() {
            None
        } else {
            Some(Self(buf))
        }
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct SyncManagerChannelWatchDog([u8]);
    pub u8, sm_channel_watch_dog, set_sm_channel_watch_dog: 8*2-1, 8*0;
}

impl<B: AsRef<[u8]>> SyncManagerChannelWatchDog<B> {
    pub const ADDRESS: u16 = 0x0420;
    pub const SIZE: usize = 2;

    pub fn new(buf: B) -> Option<Self> {
        if buf.as_ref().len() < Self::SIZE.into() {
            None
        } else {
            Some(Self(buf))
        }
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct SyncManagerChannelWDStatus([u8]);
    pub sm_channel_wd_status, set_sm_channel_wd_status: 0;
}

impl<B: AsRef<[u8]>> SyncManagerChannelWDStatus<B> {
    pub const ADDRESS: u16 = 0x0440;
    pub const SIZE: usize = 2;

    pub fn new(buf: B) -> Option<Self> {
        if buf.as_ref().len() < Self::SIZE.into() {
            None
        } else {
            Some(Self(buf))
        }
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct SIIAccess([u8]);
    pub owner, set_owner: 0;
    pub reset_access, set_reset_access: 1;
    pub pdi_accessed, _: 8*1;
}

impl<B: AsRef<[u8]>> SIIAccess<B> {
    pub const ADDRESS: u16 = 0x0500;
    pub const SIZE: usize = 2;

    pub fn new(buf: B) -> Option<Self> {
        if buf.as_ref().len() < Self::SIZE.into() {
            None
        } else {
            Some(Self(buf))
        }
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct SIIControl([u8]);
    pub enable_write_access, set_enable_write_access: 0;
    pub read_size, _: 6;
    pub address_algorithm, _: 7;
    pub read_operation, set_read_operation: 8;
    pub write_operation, set_write_operation: 8+1;
    pub reload_operation, set_reload_operation: 8+2;
    pub check_sum_error, _: 8+3;
    pub device_info_error, _: 8+4;
    pub command_error, _: 8+5;
    pub busy, _: 8+7;
}

impl<B: AsRef<[u8]>> SIIControl<B> {
    pub const ADDRESS: u16 = 0x0502;
    pub const SIZE: usize = 2;

    pub fn new(buf: B) -> Option<Self> {
        if buf.as_ref().len() < Self::SIZE.into() {
            None
        } else {
            Some(Self(buf))
        }
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct SIIAddress([u8]);
    pub u32, sii_address, set_sii_address: 8*4-1, 0;
}

impl<B: AsRef<[u8]>> SIIAddress<B> {
    pub const ADDRESS: u16 = 0x0504;
    pub const SIZE: usize = 4;

    pub fn new(buf: B) -> Option<Self> {
        if buf.as_ref().len() < Self::SIZE.into() {
            None
        } else {
            Some(Self(buf))
        }
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct SIIData([u8]);
    pub u64, sii_data, set_sii_data: 8*8-1, 0;
}

impl<B: AsRef<[u8]>> SIIData<B> {
    pub const ADDRESS: u16 = 0x0508;
    pub const SIZE: usize = 8;

    pub fn new(buf: B) -> Option<Self> {
        if buf.as_ref().len() < Self::SIZE.into() {
            None
        } else {
            Some(Self(buf))
        }
    }
}

// NOTE: MII register is not inplemented

bitfield! {
    #[derive(Debug, Clone)]
    pub struct FMMURegister([u8]);
    pub u32, logical_start_address, set_logical_start_address: 8*4-1, 8*0;
    pub u16, length, set_length: 8*6-1, 8*4;
    pub u8, logical_start_bit, set_logical_start_bit: 8*6+2, 8*6;
    pub u8, logical_end_bit, set_logical_end_bit: 8*7+2, 8*7;
    pub u16, physical_start_address, set_physical_start_address: 8*10-1, 8*8;
    pub u8, physical_start_bit, set_physical_start_bit: 8*10+2, 8*10;
    pub read_enable, set_read_enable: 8*11;
    pub write_enable, set_write_enable: 8*11;
    pub enable, set_enable: 8*12;
}

impl<B: AsRef<[u8]>> FMMURegister<B> {
    pub const ADDRESS0: u16 = 0x0600;
    pub const ADDRESS1: u16 = 0x0610;
    pub const ADDRESS2: u16 = 0x0620;
    pub const SIZE: usize = 16;

    pub fn new(buf: B) -> Option<Self> {
        if buf.as_ref().len() < Self::SIZE.into() {
            None
        } else {
            Some(Self(buf))
        }
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct SyncManagerRegister([u8]);
    pub u16, physical_start_address, set_physical_start_address: 8*2-1, 8*0;
    pub u16, length, set_length: 8*4-1, 8*2;
    pub u8, buffer_type, set_buffer_type: 8*4+1, 8*4;
    pub u8, direction, set_direction: 8*4+3, 8*4+2;
    pub dls_user_event_enable, set_dls_user_event_enable: 8*4+5;
    pub watchdog_enable, set_watchdog_enable: 8*4+6;
    pub write_event, _: 8*5;
    pub read_event, _: 8*5+1;
    pub mailbox_state, _: 8*5+3;
    pub u8, bufferd_state, _: 8*5+5, 8*5+4;
    pub channel_enable, set_channel_enable: 8*6;
    pub repeat, set_repeat: 8*6+1;
    pub dc_event_w_bus_w, set_dc_event_w_bus_w: 8*6+6;
    pub dc_event_w_loc_w, set_dc_event_w_loc_w: 8*6+7;
    pub channel_enable_pdi, _: 8*7;
    pub repeat_ack, _: 8*7+1;
}

impl<B: AsRef<[u8]>> SyncManagerRegister<B> {
    pub const ADDRESS0: u16 = 0x0800;
    pub const ADDRESS1: u16 = 0x0808;
    pub const ADDRESS2: u16 = 0x0810;
    pub const ADDRESS3: u16 = 0x0818;
    pub const SIZE: usize = 8;

    pub fn new(buf: B) -> Option<Self> {
        if buf.as_ref().len() < Self::SIZE.into() {
            None
        } else {
            Some(Self(buf))
        }
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct DCTransmission([u8]);
    pub u32, receive_time_port0, set_receive_time_port0: 8*4-1, 8*0;
    pub u32, receive_time_port1, set_receive_time_port1: 8*8-1, 8*4;
    pub u32, receive_time_port2, set_receive_time_port2: 8*12-1, 8*8;
    pub u32, receive_time_port3, set_receive_time_port3: 8*16-1, 8*12;
    pub u64, local_system_time, set_local_system_time: 8*24-1, 8*16;
    pub u64, system_time_offset, set_system_time_offset: 8*32-1, 8*24;
    pub u32, system_time_transmission_delay, set_system_time_transmission_delay: 8*40-1, 8*32;
}

impl<B: AsRef<[u8]>> DCTransmission<B> {
    pub const ADDRESS: u16 = 0x0900;
    pub const SIZE: usize = 4;

    pub fn new(buf: B) -> Option<Self> {
        if buf.as_ref().len() < Self::SIZE.into() {
            None
        } else {
            Some(Self(buf))
        }
    }
}
