use bitfield::*;
use num_enum::FromPrimitive;

bitfield! {
    #[derive(Debug, Clone)]
    pub struct CoeFrame([u8]);
    u16;
    pub number, set_number: 8, 0;
    u8;
    pub service_type, set_service_type: 15, 12;
}

impl CoeFrame<[u8; 2]> {
    pub const HEADER_SIZE: usize = 2;
    pub fn new() -> Self {
        Self([0; Self::HEADER_SIZE])
    }
}

impl<B: AsRef<[u8]>> CoeFrame<B> {
    pub fn coe_service_type(&self) -> CoeServiceType {
        self.service_type().into()
    }
}

impl<B: AsMut<[u8]>> CoeFrame<B> {
    pub fn set_coe_service_type(&mut self, coe_type: CoeServiceType) {
        self.set_service_type(coe_type as u8)
    }
}

impl<'a> CoeFrame<&'a [u8]> {
    pub fn without_header(&self) -> &'a [u8] {
        &self.0.as_ref()[CoeFrame::HEADER_SIZE..]
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash, Copy, FromPrimitive)]
#[repr(u8)]
pub enum CoeServiceType {
    Emmergency = 1,
    SdoReq,
    SdoRes,
    TxPdo,
    RxPdo,
    TxPdoRemoteReq,
    RxPdoRemoteReq,
    SdoInfo,
    #[num_enum(default)]
    Other,
}

bitfield! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SdoFrame([u8]);
    pub size_indicator, set_size_indicator: 0;
    pub transfer_type, set_transfer_type: 1;
    pub u8, data_set_size, set_data_set_size: 3, 2;
    pub complete_access, set_complete_access: 4;
    pub u8, command_specifier, set_command_specifier: 7, 5;
    pub u16, index, set_index: 23, 8;
    pub u8, sub_index, set_sub_index: 31, 24;
}

impl SdoFrame<[u8; 4]> {
    pub const HEADER_SIZE: usize = 4;
    pub fn new() -> Self {
        Self([0; Self::HEADER_SIZE])
    }
}

impl<'a> SdoFrame<&'a [u8]> {
    pub fn without_header(&self) -> &'a [u8] {
        &self.0[SdoFrame::HEADER_SIZE..]
    }
}

bitfield! {
    #[derive(Debug, Clone)]
    pub struct SdoDownloadNormalRequestFrame([u8]);
    pub u32, complete_size, set_complete_size: 31, 0;
}

impl SdoDownloadNormalRequestFrame<[u8; 4]> {
    pub const HEADER_SIZE: usize = 4;
    pub fn new() -> Self {
        Self([0; Self::HEADER_SIZE])
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
    pub struct EmmergencyFrame([u8]);
    u16, error_code, _: 15, 0;
    u8, error_register, _: 23, 16;
    u64, data, _: 63, 24;
}

impl EmmergencyFrame<[u8; 8]> {
    pub const SIZE: usize = 8;
    pub fn new() -> Self {
        Self([0; Self::SIZE])
    }
}

impl<B: AsRef<[u8]>> EmmergencyFrame<B> {
    pub fn emmergency_error_code(&self) -> EmmergencyErrorCode {
        let code = self.error_code();
        let bytes = code.to_be_bytes();
        let code_h = bytes[1];
        let code_l = bytes[0];
        match code_h {
            0x00 => EmmergencyErrorCode::ErrorResetOrNoError(code),
            0x10 => EmmergencyErrorCode::GenericError(code),
            0x20 => EmmergencyErrorCode::Current(code),
            0x21 => EmmergencyErrorCode::CurrentInputSide(code),
            0x22 => EmmergencyErrorCode::CurrentInside(code),
            0x23 => EmmergencyErrorCode::CurrentOutputSide(code),
            0x30 => EmmergencyErrorCode::Voltage(code),
            0x31 => EmmergencyErrorCode::MainsVoltage(code),
            0x32 => EmmergencyErrorCode::VoltageInside(code),
            0x33 => EmmergencyErrorCode::OutputVoltage(code),
            0x40 => EmmergencyErrorCode::Temperature(code),
            0x42 => EmmergencyErrorCode::AmbientTemperature(code),
            0x41 => EmmergencyErrorCode::DeviceTempreture(code),
            0x50 => EmmergencyErrorCode::DeviceHardware(code),
            0x60 => EmmergencyErrorCode::DeviceSoftware(code),
            0x61 => EmmergencyErrorCode::IntarnalSoftware(code),
            0x62 => EmmergencyErrorCode::UserSoftware(code),
            0x63 => EmmergencyErrorCode::DateSet(code),
            0x70 => EmmergencyErrorCode::AdditionalModules(code),
            0x80 => EmmergencyErrorCode::Monitoring(code),
            0x81 => EmmergencyErrorCode::Communication(code),
            0x82 => match code_l {
                0x10 => EmmergencyErrorCode::PdoNotProcessedDueToLengthError(code),
                0x20 => EmmergencyErrorCode::PdoLengthExceeded(code),
                _ => EmmergencyErrorCode::ProtocolError(code),
            },
            0x90 => EmmergencyErrorCode::ExternalError(code),
            0xA0 => EmmergencyErrorCode::EsmTransitionError(code),
            0xF0 => EmmergencyErrorCode::AdditionalFunctions(code),
            0xFF => EmmergencyErrorCode::DeviceSpecific(code),
            _ => EmmergencyErrorCode::Other(code),
        }
    }
}

#[derive(Debug, Clone)]
pub enum EmmergencyErrorCode {
    ErrorResetOrNoError(u16),
    GenericError(u16),
    Current(u16),
    CurrentInputSide(u16),
    CurrentInside(u16),
    CurrentOutputSide(u16),
    Voltage(u16),
    MainsVoltage(u16),
    VoltageInside(u16),
    OutputVoltage(u16),
    Temperature(u16),
    AmbientTemperature(u16),
    DeviceTempreture(u16),
    DeviceHardware(u16),
    DeviceSoftware(u16),
    IntarnalSoftware(u16),
    UserSoftware(u16),
    DateSet(u16),
    AdditionalModules(u16),
    Monitoring(u16),
    Communication(u16),
    ProtocolError(u16),
    PdoNotProcessedDueToLengthError(u16),
    PdoLengthExceeded(u16),
    ExternalError(u16),
    EsmTransitionError(u16),
    AdditionalFunctions(u16),
    DeviceSpecific(u16),
    Other(u16),
}
