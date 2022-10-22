use bitfield::*;
use num_enum::FromPrimitive;

bitfield! {
    #[derive(Debug, Clone)]
    pub struct CoeHeader([u8]);
    u16;
    pub number, set_number: 8, 0;
    u8;
    pub service_type, set_service_type: 15, 12;
}

impl CoeHeader<[u8; 2]> {
    pub const SIZE: usize = 2;
    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash, Copy)]
pub enum CoeServiceType {
    Emmergency = 1,
    SdoReq,
    SdoRes,
    TxPdo,
    RxPdo,
    TxPdoRemoteReq,
    RxPdoRemoteReq,
    SdoInfo,
}

bitfield! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SdoHeader([u8]);
    pub size_indicator, set_size_indicator: 0;
    pub transfer_type, set_transfer_type: 1;
    pub u8, data_set_size, set_data_set_size: 3, 2;
    pub complete_access, set_complete_access: 4;
    pub u8, command_specifier, set_command_specifier: 7, 5;
    pub u16, index, set_index: 23, 8;
    pub u8, sub_index, set_sub_index: 31, 24;
}

impl SdoHeader<[u8; 4]> {
    pub const SIZE: usize = 4;
    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct SdoDownloadNormalHeader([u8]);
    pub u32, complete_size, set_complete_size: 31, 0;
}

impl SdoDownloadNormalHeader<[u8; 4]> {
    pub const SIZE: usize = 4;
    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}

#[derive(Debug, Clone, Copy, FromPrimitive, PartialEq, Eq)]
#[repr(u32)]
pub enum AbortCode {
    NoToggleBitChange = 0x05_03_00_00,
    Timeout = 0x05_04_00_00,
    UnknownClient = 0x05_04_00_01,
    OutsideMemoryRange = 0x05_04_00_05,
    NotSupportedAccess = 0x06_01_00_00,
    WriteOnly = 0x06_01_00_01,
    ReadOnly = 0x06_01_00_02,
    SubIndexCannotBeWritten = 0x06_01_00_03,
    NotSupportForVariableLength = 0x06_01_00_04,
    LengthExceedsMailboxSize = 0x06_01_00_05,
    ObjectMappedToRxPdo = 0x06_01_00_06,
    DoesNotExistInDict = 0x06_02_00_00,
    UnableToMapToPdo = 0x06_04_00_41,
    PdoLimit = 0x06_04_00_42,
    ParameterIncompatibilities = 0x06_04_00_43,
    DeviceIncompatibilities = 0x06_04_00_47,
    FailureDueToWriteProtect = 0x06_06_00_00,
    ParameterLengthMismatch = 0x06_07_00_10,
    ParameterLengthTooLong = 0x06_07_00_12,
    ParameterLengthTooShort = 0x06_07_00_13,
    SubIndexDoesNotExist = 0x06_09_00_11,
    ValueRangeExceeded = 0x06_09_00_30,
    WriteParameterTooLarge = 0x06_09_00_31,
    WriteParameterTooSmall = 0x06_09_00_32,
    ConfiguredModuleListDoesNotMatch = 0x06_09_00_33,
    MaxValueIsLessThanMinValue = 0x06_09_00_36,
    GeneralError = 0x08_00_00_00,
    CannotTransfer = 0x08_00_00_20,
    CannotTransferDueToLocalControl = 0x08_00_00_21,
    CannotTransferInCurrentState = 0x08_00_00_22,
    ObjectDictionaryDoesNotExist = 0x08_00_00_23,
    #[num_enum(default)]
    UnknownAbortCode,
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct Emmergency([u8]);
    u16, error_code, _: 15, 0;
    u8, error_register, _: 23, 16;
    u64, data, _: 63, 24;
}

impl Emmergency<[u8; 8]> {
    const SIZE: usize = 8;
    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}
