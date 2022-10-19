use crate::{interface::SlaveAddress, task::*};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub slave_address: SlaveAddress,
    pub kind: ConfigErrorKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigErrorKind {
    SetFmmuRegister(RegisterError),
    SetSyncManagerSyncType(SdoError),
    SetSyncManagerCycleTime(SdoError),
    SetSync0CycleTime(RegisterError),
    SetSync1CycleTime(RegisterError),
    SetSyncSignalStartTime(RegisterError),
    ActivateDc(RegisterError),
    ClearPdoMappings(SdoError),
    AssignPdoMapToSyncManager(SdoError),
    ClearPdoEntries(SdoError),
    AssignPdoEntryToPdoMap(SdoError),
    SetNumberOfPdoMappings(SdoError),
    GetNumberOfPdoMappings(SdoError),
    GetPdoMappingAddress(SdoError),
    SetNumberOfPdoEntries(SdoError),
    GetNumberOfPdoEntries(SdoError),
    GetPdoEntrtyAddress(SdoError),
    GetNumberOfSyncManagerChannel(SdoError),
    GetSyncManagerCommunicationType(SdoError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdoError {
    pub index: u16,
    pub sub_index: u8,
    pub error: TaskError<SdoTaskError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterError {
    pub address: u16,
    pub error: TaskError<()>,
}
