use bitfield::*;

//pub const COE_HEADER_LENGTH: usize = 2;

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
    TxPDO,
    RxPDO,
    TxPDORemoteReq,
    RxPDORemoteReq,
    SdoInfo,
}

bitfield! {
    #[derive(Debug, Clone)]
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

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash, Copy)]
pub enum AbortCode {
    NoToggleBitChange = 0x05_03_00_00,  //
    Timeout = 0x05_04_00_00,            //
    UnknownClient = 0x05_04_00_01,      //
    OutsideMemoryRange = 0x05_04_00_05, //
    NotSupportedAccess = 0x06_01_00_00, //
    WriteOnly = 0x06_01_00_01,
    ReadOnly = 0x06_01_00_02,
    SubIndexCannotBeWritten = 0x06_01_00_03,
    NotSupportForVariableLength = 0x06_01_00_04,
    LengthExceedsMailboxSize = 0x06_01_00_05,
    ObjectMappedToRxPDO = 0x06_01_00_06,
    DoesNotExistInDict = 0x06_02_00_00,
    UnableToMapToPDO = 0x06_04_00_41,
    PDOLimit = 0x06_04_00_42,
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
    MaxValueIsLessThanMinValue = 0x06_09_00_36,
    GeneralError = 0x08_00_00_00,
    CannotTransfer = 0x08_00_00_20,
    CannotTransferDueToLocalControl = 0x08_00_00_21,
    CannotTransferInCurrentState = 0x08_00_00_22,
    ObjectDictionaryDoesNotExist = 0x08_00_00_23,
    UnknownAbortCode,
}

impl From<u32> for AbortCode {
    fn from(value: u32) -> Self {
        if value == AbortCode::NoToggleBitChange as u32 {
            AbortCode::NoToggleBitChange
        } else if value == AbortCode::Timeout as u32 {
            AbortCode::Timeout
        } else if value == AbortCode::UnknownClient as u32 {
            AbortCode::UnknownClient
        } else if value == AbortCode::OutsideMemoryRange as u32 {
            AbortCode::OutsideMemoryRange
        } else if value == AbortCode::NotSupportedAccess as u32 {
            AbortCode::NotSupportedAccess
        } else if value == AbortCode::ReadOnly as u32 {
            AbortCode::ReadOnly
        } else if value == AbortCode::WriteOnly as u32 {
            AbortCode::WriteOnly
        } else if value == AbortCode::SubIndexCannotBeWritten as u32 {
            AbortCode::SubIndexCannotBeWritten
        } else if value == AbortCode::LengthExceedsMailboxSize as u32 {
            AbortCode::LengthExceedsMailboxSize
        } else if value == AbortCode::ObjectMappedToRxPDO as u32 {
            AbortCode::ObjectMappedToRxPDO
        } else if value == AbortCode::DoesNotExistInDict as u32 {
            AbortCode::DoesNotExistInDict
        } else if value == AbortCode::UnableToMapToPDO as u32 {
            AbortCode::UnableToMapToPDO
        } else if value == AbortCode::PDOLimit as u32 {
            AbortCode::PDOLimit
        } else if value == AbortCode::ParameterIncompatibilities as u32 {
            AbortCode::ParameterIncompatibilities
        } else if value == AbortCode::DeviceIncompatibilities as u32 {
            AbortCode::DeviceIncompatibilities
        } else if value == AbortCode::FailureDueToWriteProtect as u32 {
            AbortCode::FailureDueToWriteProtect
        } else if value == AbortCode::ParameterLengthMismatch as u32 {
            AbortCode::ParameterLengthMismatch
        } else if value == AbortCode::ParameterLengthTooLong as u32 {
            AbortCode::ParameterLengthTooLong
        } else if value == AbortCode::ParameterLengthTooShort as u32 {
            AbortCode::ParameterLengthTooShort
        } else if value == AbortCode::SubIndexDoesNotExist as u32 {
            AbortCode::SubIndexDoesNotExist
        } else if value == AbortCode::ValueRangeExceeded as u32 {
            AbortCode::ValueRangeExceeded
        } else if value == AbortCode::WriteParameterTooLarge as u32 {
            AbortCode::WriteParameterTooLarge
        } else if value == AbortCode::WriteParameterTooSmall as u32 {
            AbortCode::WriteParameterTooSmall
        } else if value == AbortCode::MaxValueIsLessThanMinValue as u32 {
            AbortCode::MaxValueIsLessThanMinValue
        } else if value == AbortCode::GeneralError as u32 {
            AbortCode::GeneralError
        } else if value == AbortCode::CannotTransfer as u32 {
            AbortCode::CannotTransfer
        } else if value == AbortCode::CannotTransferDueToLocalControl as u32 {
            AbortCode::CannotTransferDueToLocalControl
        } else if value == AbortCode::CannotTransferInCurrentState as u32 {
            AbortCode::CannotTransferInCurrentState
        } else if value == AbortCode::ObjectDictionaryDoesNotExist as u32 {
            AbortCode::ObjectDictionaryDoesNotExist
        } else {
            AbortCode::UnknownAbortCode
        }
    }
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
