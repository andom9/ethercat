use super::EtherCatSystemTime;
use super::ReceivedData;
use crate::cyclic::CyclicProcess;
use crate::error::EcError;
use crate::interface::Command;
use crate::interface::TargetSlave;
use crate::packet::ethercat::CommandType;
use crate::register::application::AlStatus;
use crate::slave::AlState;
use crate::util::const_max;
use core::convert::TryFrom;
use num_enum::TryFromPrimitive;

#[derive(Debug)]
enum State {
    Error(EcError<()>),
    Idle,
    Read,
    Complete,
}

#[derive(Debug)]
pub struct AlStateReader {
    state: State,
    slave_address: TargetSlave,
    command: Command,
    buffer: [u8; buffer_size()],
    current_al_state: AlState,
    current_al_status_code: Option<AlStatusCode>,
}

impl AlStateReader {
    pub fn new() -> Self {
        Self {
            state: State::Idle,
            slave_address: TargetSlave::default(),
            command: Command::default(),
            buffer: [0; buffer_size()],
            current_al_state: AlState::Init,
            current_al_status_code: None,
        }
    }

    pub fn start(&mut self, slave_address: TargetSlave) {
        self.slave_address = slave_address;
        self.state = State::Read;
        self.buffer.fill(0);
        self.command = Command::default();
    }

    pub fn wait(&mut self) -> Option<Result<(AlState, Option<AlStatusCode>), EcError<()>>> {
        match &self.state {
            State::Complete => Some(Ok((
                self.current_al_state,
                self.current_al_status_code.clone(),
            ))),
            State::Error(err) => Some(Err(err.clone())),
            _ => None,
        }
    }
}

impl CyclicProcess for AlStateReader {
    fn next_command(
        &mut self,
        //_: &mut NetworkDescription,
        _: EtherCatSystemTime,
    ) -> Option<(Command, &[u8])> {
        match self.state {
            State::Idle => None,
            State::Error(_) => None,
            State::Read => {
                match self.slave_address {
                    TargetSlave::Single(slave_address) => {
                        self.command = Command::new_read(slave_address, AlStatus::ADDRESS)
                    }
                    TargetSlave::All(_num_slaves) => {
                        self.command = Command::new(CommandType::BRD, 0, AlStatus::ADDRESS)
                    }
                }
                self.buffer.fill(0);
                Some((self.command, &self.buffer[..AlStatus::SIZE]))
            }
            State::Complete => None,
        }
    }

    fn recieve_and_process(
        &mut self,
        recv_data: Option<ReceivedData>,
        //desc: &mut NetworkDescription,
        _: EtherCatSystemTime,
    ) {
        let data = if let Some(recv_data) = recv_data {
            let ReceivedData { command, data, wkc } = recv_data;
            if !(command.c_type == self.command.c_type && command.ado == self.command.ado) {
                self.state = State::Error(EcError::UnexpectedCommand);
            }
            match self.slave_address {
                TargetSlave::Single(slave_address) => {
                    if wkc != 1 {
                        self.state = State::Error(EcError::UnexpectedWKC(wkc));
                    }
                }
                TargetSlave::All(num_slaves) => {
                    if wkc != num_slaves {
                        self.state = State::Error(EcError::UnexpectedWKC(wkc));
                    }
                }
            }
            data
        } else {
            self.state = State::Error(EcError::LostCommand);
            return;
        };

        match self.state {
            State::Idle => {}
            State::Error(_) => {}
            State::Complete => {}
            State::Read => {
                let al_status = AlStatus(data);
                let al_state = AlState::from(al_status.state());
                self.current_al_state = al_state;
                if al_status.change_err() {
                    self.current_al_status_code =
                        Some(AlStatusCode::try_from(al_status.al_status_code()).unwrap());
                }
                self.state = State::Complete;
            }
        }
    }
}

const fn buffer_size() -> usize {
    let mut size = 0;
    size = const_max(size, AlStatus::SIZE);
    size
}

#[derive(Debug, Clone, Copy, TryFromPrimitive)]
#[repr(u16)]
pub enum AlStatusCode {
    NoError = 0,
    UnspecifiedError = 0x0001,
    NoMemory = 0x0002,
    InvalidRevision = 0x0004,
    InvalidDeviceSetup = 0x0003,
    SiiInfomationDoesNoeMatchFirmware = 0x0006,
    FirmwareUpdateNotSuccessful = 0x0007,
    LicenceError = 0x000E,
    InvalidRequestedStateChange = 0x0011,
    UnknownRequestedStateChange = 0x0012,
    BootstrapNotSupported = 0x0013,
    NoValidFirmware = 0x0014,
    InvalidMailboxConfiguration = 0x0015,
    InvalidSyncManagerConfiguration = 0x0017,
    NoValidInputsAvailable = 0x0018,
    NoValidOutputs = 0x0019,
    SynchronizationError = 0x001A,
    SyncManagerWatchdog = 0x001B,
    InvalidSyncManagerTypes = 0x001C,
    InvalidOutputConfiguration = 0x001D,
    InvalidInputConfiguration = 0x001E,
    InvalidWatchdogConfiguraion = 0x001F,
    SlaveNeedsColdStart = 0x0020,
    SlaveNeedsInit = 0x0021,
    SlaveNeedsPreop = 0x0022,
    SlaveNeedsSafeop = 0x0023,
    InvalidInputMapping = 0x0024,
    InvalidOutputMapping = 0x0025,
    InconsistentSettings = 0x0026,
    FreerunNotSupported = 0x0027,
    SynchronizationNotSupported = 0x0028,
    FreerunNeeds3BufferMode = 0x0029,
    BackgroundWatchDog = 0x002A,
    NoValidInputsAndOutpus = 0x002B,
    FatalSyncError = 0x002C,
    NoSyncError = 0x002D,
    CycleTimeTooSmall = 0x002E,
    InvalidDcSyncConfiguration = 0x0030,
    InvalidDcLatchConfiguration = 0x0031,
    PllError = 0x0032,
    DcSyncIoError = 0x0033,
    DcSyncTimeoutError = 0x0034,
    DcInvalidSyncCycleTime = 0x0035,
    DcSync0CycleTime = 0x0036,
    DcSync1CycleTime = 0x0037,
    MbxAoe = 0x0041,
    MbxEoe = 0x0042,
    MbxCoe = 0x0043,
    MbxFoe = 0x0044,
    MbxSoe = 0x0045,
    MbxVoe = 0x004F,
    EepromNoAccess = 0x0050,
    EepromError = 0x0051,
    ExternalHardwareNotReady = 0x0052,
    SlaveRestartedLocally = 0x0060,
    DeviceIdentificationValueUpdated = 0x0061,
    DetectedModuleIdentListDoesNotMatch = 0x0070,
    SupplyVoltageToolow = 0x0080,
    SupplyVoltageYooHigh = 0x0081,
    TemperatureTooLow = 0x0082,
    TemperatureTooHigh = 0x0083,
    ApplocationControllerAvailable = 0x00F0,
    UndefinedError,
}
