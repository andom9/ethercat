use crate::{interface::SlaveAddress, slave::SyncMode, task::*};

#[derive(Debug, Clone)]
pub struct ConfigError {
    pub slave_address: SlaveAddress,
    pub kind: ConfigErrorKind,
}

#[derive(Debug, Clone)]
pub enum ConfigErrorKind {
    SetFmmuRegister(RegisterError),
    GetSyncManagerCycleTime(SdoError),
    SetSyncManagerSyncType(SdoError),
    GetSyncManagerSyncType(SdoError),
    SyncModeNotSupported(SyncMode),
    GetMinimumCycleTime(SdoError),
    CycleTimeTooSmall(u32),
    SetSyncManagerCycleTime(SdoError),
    ResetSyncError(SdoError),
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

#[derive(Debug, Clone)]
pub struct SdoError {
    pub index: u16,
    pub sub_index: u8,
    pub error: TaskError<SdoErrorKind>,
}

#[derive(Debug, Clone)]
pub struct RegisterError {
    pub address: u16,
    pub error: TaskError<()>,
}
