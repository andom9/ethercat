use bitfield::*;

bitfield! {
    #[derive(Debug, Clone)]
    pub struct OdPdoEntry([u8]);
    pub u8, bit_length, set_bit_length: 8*1-1, 0;
    pub u8, sub_index, set_sub_index: 8*2-1, 8*1;
    pub u16, index, set_index: 8*4-1, 8*2;
}

impl OdPdoEntry<[u8; 4]> {
    pub const SIZE: usize = 4;

    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct DeviceType([u8]);
    pub u16, device_profile_number, set_device_profile_number: 15, 0;
    pub u8, device_type, set_device_type: 23, 16;
    pub u8, mode_bits, set_mode_bits: 31, 24;
}

impl DeviceType<[u8; 4]> {
    pub const INDEX: u16 = 0x1000;
    pub const SUB_INDEX: u8 = 0;
    pub const SIZE: usize = 4;

    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct ErrorRegister([u8]);
    pub generic, set_generic: 0;
    pub current, set_current: 1;
    pub voltage, set_voltage: 2;
    pub temperature, set_temperature: 3;
    pub communication, set_communication: 4;
    pub device_profile, set_device_profile: 5;
    pub manufacture_specific, set_manufacture_specific: 7;
}

impl ErrorRegister<[u8; 1]> {
    pub const INDEX: u16 = 0x1001;
    pub const SUB_INDEX: u8 = 0;
    pub const SIZE: usize = 1;

    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct IdentityObjectEntries([u8]);
    pub u8, number_of_entries, set_number_of_entries: 7, 0;
}

impl IdentityObjectEntries<[u8; 1]> {
    pub const INDEX: u16 = 0x1018;
    pub const SUB_INDEX: u8 = 0;
    pub const SIZE: usize = 1;

    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct IdentityObjectVender([u8]);
    pub u32, vender, set_vender: 31, 0;
}

impl IdentityObjectVender<[u8; 4]> {
    pub const INDEX: u16 = 0x1018;
    pub const SUB_INDEX: u8 = 1;
    pub const SIZE: usize = 4;

    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct IdentityObjectProduct([u8]);
    pub u32, product, set_product: 31, 0;
}

impl IdentityObjectProduct<[u8; 4]> {
    pub const INDEX: u16 = 0x1018;
    pub const SUB_INDEX: u8 = 2;
    pub const SIZE: usize = 4;

    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct IdentityObjectRevision([u8]);
    pub u32, revision, set_revision: 31, 0;
}

impl IdentityObjectRevision<[u8; 4]> {
    pub const INDEX: u16 = 0x1018;
    pub const SUB_INDEX: u8 = 3;
    pub const SIZE: usize = 4;

    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct IdentityObjectSerial([u8]);
    pub u32, serial, set_serial: 31, 0;
}

impl IdentityObjectSerial<[u8; 4]> {
    pub const INDEX: u16 = 0x1018;
    pub const SUB_INDEX: u8 = 4;
    pub const SIZE: usize = 4;

    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}

pub mod cia402 {
    use bitfield::*;
    use core::convert::TryFrom;
    use num_enum::TryFromPrimitive;
    // #[derive(Debug, Clone, Copy)]
    // pub enum ControlWord {
    //     ShutDown = 0b1000_0110,
    //     SwitchOnOrEnableOperation = 0b0000_1111,
    //     DisableVoltage = 0b0000_0000,
    //     QuickStop = 0b0000_0010,
    //     FaultReset = 0b1000_0000,
    // }
    bitfield! {
        #[derive(Debug, Clone)]
        pub struct ControlWord([u8]);
        pub switch_on, set_switch_on: 0;
        pub enable_voltage, set_enable_voltage: 1;
        pub nquick_stop, set_nquick_stop: 2;
        pub enable_operation, set_enable_operation: 3;
        pub operation_mode_specific1, set_operation_mode_specific1: 4;
        pub operation_mode_specific2, set_operation_mode_specific2: 5;
        pub operation_mode_specific3, set_operation_mode_specific3: 6;
        pub fault_reset, set_fault_reset: 7;
        pub halt, set_halt: 8;
        pub operation_mode_specific4, set_operation_mode_specific4: 9;
        pub manufacture_specific1, set_manufacture_specific1: 11;
        pub manufacture_specific2, set_manufacture_specific2: 12;
        pub manufacture_specific3, set_manufacture_specific3: 13;
        pub manufacture_specific4, set_manufacture_specific4: 14;
        pub manufacture_specific5, set_manufacture_specific5: 15;
    }

    impl ControlWord<[u8; 2]> {
        pub const INDEX: u16 = 0x6040;
        pub const SUB_INDEX: u8 = 0;
        pub const SIZE: usize = 2;

        pub fn new() -> Self {
            Self([0; Self::SIZE])
        }
        pub fn new_disable_voltage() -> Self {
            Self::new()
        }
        pub fn new_switch_on_and_enable_operation() -> Self {
            let mut this = Self::new();
            this.set_nquick_stop(true);
            this.set_switch_on(true);
            this.set_enable_voltage(true);
            this.set_enable_operation(true);
            this
        }
        pub fn new_fault_reset() -> Self {
            let mut this = Self::new();
            this.set_fault_reset(true);
            this
        }
        pub fn new_quick_stop() -> Self {
            let mut this = Self::new();
            this.set_enable_voltage(true);
            this
        }
    }

    bitfield! {
        #[derive(Debug, Clone)]
        pub struct StatusWord([u8]);
        pub ready_to_switch_on, set_ready_to_switch_on: 0;
        pub switched_on, set_switched_on: 1;
        pub operation_enabled, set_operation_enabled: 2;
        pub fault, set_fault: 3;
        pub voltage_enabled, set_voltage_enabled: 4;
        pub nquick_stop, set_nquick_stop: 5;
        pub switch_on_disabled, set_switch_on_disabled: 6;
        pub warning, set_warning: 7;
        pub manufacture_specific1, set_manufacture_specific1: 8;
        pub remote, set_remote: 9;
        pub operation_mode_specific1, set_operation_mode_specific1: 10;
        pub internal_limit_active, set_internal_limit_active: 11;
        pub operation_mode_specific2, set_operation_mode_specific2: 12;
        pub operation_mode_specific3, set_operation_mode_specific3: 13;
        pub manufacture_specific2, set_manufacture_specific2: 14;
        pub manufacture_specific3, set_manufacture_specific3: 15;
    }

    impl StatusWord<[u8; 2]> {
        pub const INDEX: u16 = 0x6041;
        pub const SUB_INDEX: u8 = 0;
        pub const SIZE: usize = 2;

        pub fn new() -> Self {
            Self([0; Self::SIZE])
        }
    }

    bitfield! {
        #[derive(Debug, Clone)]
        pub struct OperationMode([u8]);
        pub i8, modes_of_operation, set_modes_of_operation: 7, 0;
    }

    impl OperationMode<[u8; 1]> {
        pub const INDEX: u16 = 0x6060;
        pub const SUB_INDEX: u8 = 0;
        pub const SIZE: usize = 1;

        pub fn new() -> Self {
            Self([0; Self::SIZE])
        }
    }

    impl<B: AsRef<[u8]>> OperationMode<B> {
        pub fn kind(&self) -> OperationModeKind {
            OperationModeKind::try_from(self.modes_of_operation())
                .unwrap_or(OperationModeKind::Other)
        }
    }

    #[derive(Debug, Eq, PartialEq, TryFromPrimitive)]
    #[repr(i8)]
    /// Operation mode for servo drives
    pub enum OperationModeKind {
        /// Profile position mode
        PP = 1,
        /// Velocity mode
        VL = 2,
        /// Profile velocity mode
        PV = 3,
        /// Torque profile mode
        TQ = 4,
        /// Homing mode
        HM = 6,
        /// Interpolated position mode
        IP = 7,
        /// Cyclic synchronous position mode
        CSP = 8,
        /// Cyclic synchronous velocity mode
        CSV = 9,
        /// Cyclic synchronous torque mode
        CST = 10,
        /// Cyclic synchronous torque mode with commutation angle
        CSTCA = 11,
        Other,
    }

    bitfield! {
        #[derive(Debug, Clone)]
        pub struct OperationModeDisplay([u8]);
        pub i8, modes_of_operation, set_modes_of_operation: 7, 0;
    }

    impl OperationModeDisplay<[u8; 1]> {
        pub const INDEX: u16 = 0x6061;
        pub const SUB_INDEX: u8 = 0;
        pub const SIZE: usize = 1;

        pub fn new() -> Self {
            Self([0; Self::SIZE])
        }
    }

    impl<B: AsRef<[u8]>> OperationModeDisplay<B> {
        pub fn kind(&self) -> OperationModeKind {
            OperationModeKind::try_from(self.modes_of_operation())
                .unwrap_or(OperationModeKind::Other)
        }
    }

    bitfield! {
        #[derive(Debug, Clone)]
        pub struct SupportedDriveModes([u8]);
        pub pp, set_pp: 0;
        pub vl, set_vl: 1;
        pub pv, set_pv: 2;
        pub tq, set_tq: 3;
        pub hm, set_hm: 5;
        pub ip, set_ip: 6;
        pub csp, set_csp: 7;
        pub csv, set_csv: 8;
        pub cst, set_cst: 9;
        pub cstca, set_cstca: 10;
    }

    impl SupportedDriveModes<[u8; 4]> {
        pub const INDEX: u16 = 0x6502;
        pub const SUB_INDEX: u8 = 0;
        pub const SIZE: usize = 4;

        pub fn new() -> Self {
            Self([0; Self::SIZE])
        }
    }
}
