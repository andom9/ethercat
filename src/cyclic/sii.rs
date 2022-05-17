use crate::cyclic::Cyclic;
use crate::error::CommonError;
use crate::interface::*;
use crate::register::datalink::*;
use crate::util::*;
use embedded_hal::timer::CountDown;
use fugit::MicrosDurationU32;

const TIMEOUT_MS: u32 = 100;

#[derive(Debug, Clone)]
pub enum SIIError {
    Common(CommonError),
    PermittionDenied,
    AddressSizeOver,
    Busy,
    CheckSumError,
    DeviceInfoError,
    CommandError,
    TimeoutMs(u32),
}

impl From<CommonError> for SIIError {
    fn from(err: CommonError) -> Self {
        Self::Common(err)
    }
}

#[derive(Debug)]
enum SIIState {
    Error(SIIError),
    Idle,
    Init,
    SetOwnership,
    CheckOwnership,
    SetAddress,
    SetReadOperation,
    Poll,
    Read,
    Complete,
}

impl Default for SIIState {
    fn default() -> Self {
        Self::Init
    }
}

#[derive(Debug)]
pub struct SIIReader<'a, T>
where
    T: CountDown<Time = MicrosDurationU32>,
{
    pub(crate) timer: &'a mut T,
    command: Command,
    slave_address: SlaveAddress,
    buffer: [u8; buffer_size()],
    state: SIIState,
    sii_address: u16,
    read_size: usize,
}

impl<'a, T> SIIReader<'a, T>
where
    T: CountDown<Time = MicrosDurationU32>,
{
    pub fn new(timer: &'a mut T) -> Self {
        Self {
            timer: timer,
            state: SIIState::Idle,
            slave_address: SlaveAddress::SlaveNumber(0),
            sii_address: 0,
            command: Command::default(),
            buffer: [0; buffer_size()],
            read_size: 0,
        }
    }

    pub fn start(&mut self, slave_address: SlaveAddress, sii_address: u16) {
        self.slave_address = slave_address;
        self.sii_address = sii_address;
        self.state = SIIState::Init;
        self.buffer.fill(0);
        self.command = Command::default();
    }

    pub fn wait(&mut self) -> nb::Result<(SIIData<[u8; SIIData::SIZE]>, usize), SIIError> {
        match &self.state {
            SIIState::Complete => Ok((SIIData(self.buffer.clone()), self.read_size)),
            SIIState::Error(err) => Err(nb::Error::Other(err.clone())),
            _ => Err(nb::Error::WouldBlock),
        }
    }

    //pub fn start(&mut self, slave_address: SlaveAddress, sii_address: u16) -> bool {
    //    match self.state {
    //        SIIState::Idle | SIIState::Complete | SIIState::Error(_) => {
    //            self.reset();
    //            self.slave_address = slave_address;
    //            self.sii_address = sii_address;
    //            self.state = SIIState::Init;
    //            true
    //        }
    //        _ => false,
    //    }
    //}

    //pub fn reset(&mut self) {
    //    self.state = SIIState::Init;
    //    //self.slave_address = SlaveAddress::default();
    //    //self.sii_address = 0;
    //    self.command = Command::default();
    //    self.buffer.fill(0);
    //    self.read_size = 0;
    //}

    //pub fn error(&self) -> Option<SIIError> {
    //    if let SIIState::Error(err) = &self.state {
    //        Some(err.clone())
    //    } else {
    //        None
    //    }
    //}

    //pub fn wait_read_data(
    //    &self,
    //) -> Result<Option<(SIIData<[u8; SIIData::SIZE]>, usize)>, SIIError> {
    //    if let SIIState::Error(err) = &self.state {
    //        Err(err.clone())
    //    } else {
    //        if let SIIState::Complete = self.state {
    //            Ok(Some((SIIData(self.buffer.clone()), self.read_size)))
    //        } else {
    //            Ok(None)
    //        }
    //    }
    //}

    pub(crate) fn take_timer(self) -> &'a mut T {
        self.timer
    }
}

impl<'a, T> Cyclic for SIIReader<'a, T>
where
    T: CountDown<Time = MicrosDurationU32>,
{
    fn next_command(&mut self) -> Option<(Command, &[u8])> {
        match self.state {
            SIIState::Idle => None,
            SIIState::Error(_) => None,
            SIIState::Init => {
                self.command = Command::new_read(self.slave_address, SIIControl::ADDRESS);
                self.buffer.fill(0);
                Some((self.command, &self.buffer[..SIIControl::SIZE]))
            }
            SIIState::SetOwnership => {
                self.buffer.fill(0);
                let mut sii_access = SIIAccess(self.buffer);
                sii_access.set_owner(false);
                sii_access.set_reset_access(true);
                self.command = Command::new_write(self.slave_address, SIIAccess::ADDRESS);
                Some((self.command, &self.buffer[..SIIAccess::SIZE]))
            }
            SIIState::CheckOwnership => {
                self.command = Command::new_read(self.slave_address, SIIAccess::ADDRESS);
                self.buffer.fill(0);
                Some((self.command, &self.buffer[..SIIAccess::SIZE]))
            }
            SIIState::SetAddress => {
                self.buffer.fill(0);
                let mut sii_address = SIIAddress(self.buffer);
                sii_address.set_sii_address(self.sii_address as u32);
                self.command = Command::new_write(self.slave_address, SIIAddress::ADDRESS);
                Some((self.command, &self.buffer[..SIIAddress::SIZE]))
            }
            SIIState::SetReadOperation => {
                self.buffer.fill(0);
                let mut sii_address = SIIControl(self.buffer);
                sii_address.set_read_operation(true);
                self.command = Command::new_write(self.slave_address, SIIControl::ADDRESS);
                Some((self.command, &self.buffer[..SIIControl::SIZE]))
            }
            SIIState::Poll => {
                self.command = Command::new_read(self.slave_address, SIIControl::ADDRESS);
                self.buffer.fill(0);
                Some((self.command, &self.buffer[..SIIControl::SIZE]))
            }
            SIIState::Read => {
                self.command = Command::new_read(self.slave_address, SIIData::ADDRESS);
                self.buffer.fill(0);
                Some((self.command, &self.buffer[..SIIData::SIZE]))
            }
            SIIState::Complete => None,
        }
    }

    fn recieve_and_process(&mut self, command: Command, data: &[u8], wkc: u16) -> bool {
        if command != self.command {
            self.state = SIIState::Error(SIIError::Common(CommonError::PacketDropped));
        }
        if wkc != 1 {
            self.state = SIIState::Error(SIIError::Common(CommonError::UnexpectedWKC(wkc)));
        }

        match self.state {
            SIIState::Idle => {}
            SIIState::Error(_) => {}
            SIIState::Init => {
                let sii_control = SIIControl(data);
                if sii_control.check_sum_error() {
                    self.state = SIIState::Error(SIIError::CheckSumError);
                }
                if sii_control.device_info_error() {
                    self.state = SIIState::Error(SIIError::DeviceInfoError);
                }

                // アドレスアルゴリズムが0なら、アドレスは1オクテットでなければならない。
                if !sii_control.address_algorithm() && self.sii_address >> 8 != 0 {
                    self.state = SIIState::Error(SIIError::AddressSizeOver);
                }

                if sii_control.busy()
                    || sii_control.read_operation()
                    || sii_control.write_operation()
                    || sii_control.reload_operation()
                {
                    self.state = SIIState::Error(SIIError::Busy);
                }

                self.read_size = if sii_control.read_size() { 8 } else { 4 };

                self.state = SIIState::SetOwnership;
            }
            SIIState::SetOwnership => {
                self.state = SIIState::CheckOwnership;
            }
            SIIState::CheckOwnership => {
                let sii_access = SIIAccess(data);
                if sii_access.owner() || sii_access.pdi_accessed() {
                    self.state = SIIState::Error(SIIError::PermittionDenied);
                }
                self.state = SIIState::SetAddress;
            }
            SIIState::SetAddress => {
                self.state = SIIState::SetReadOperation;
            }
            SIIState::SetReadOperation => {
                self.state = SIIState::Poll;
                self.timer
                    .start(MicrosDurationU32::from_ticks(TIMEOUT_MS * 1000));
            }
            SIIState::Poll => {
                let sii_control = SIIControl(data);
                if sii_control.command_error() {
                    self.state = SIIState::Error(SIIError::CommandError);
                } else if !sii_control.busy() && !sii_control.read_operation() {
                    self.state = SIIState::Read;
                } else {
                    match self.timer.wait() {
                        Ok(_) => self.state = SIIState::Error(SIIError::TimeoutMs(TIMEOUT_MS)),
                        Err(nb::Error::Other(_)) => {
                            self.state = SIIState::Error(CommonError::UnspcifiedTimerError.into())
                        }
                        Err(nb::Error::WouldBlock) => (),
                    }
                }
            }
            SIIState::Read => {
                self.state = SIIState::Complete;
            }
            SIIState::Complete => {}
        }

        if let SIIState::Error(_) = self.state {
            false
        } else {
            true
        }
    }
}

const fn buffer_size() -> usize {
    let mut size = 0;
    size = const_max(size, SIIAccess::SIZE);
    size = const_max(size, SIIControl::SIZE);
    size = const_max(size, SIIAddress::SIZE);
    size = const_max(size, SIIData::SIZE);
    size
}

pub mod sii_reg {
    pub struct PDIControl;
    impl PDIControl {
        pub const ADDRESS: u16 = 0;
        pub const SIZE: usize = 2;
    }

    pub struct PDIConfig;
    impl PDIConfig {
        pub const ADDRESS: u16 = 0;
        pub const SIZE: usize = 2;
    }

    pub struct SyncImpulseLen;
    impl SyncImpulseLen {
        pub const ADDRESS: u16 = 0;
        pub const SIZE: usize = 2;
    }

    pub struct StationAlias;
    impl StationAlias {
        pub const ADDRESS: u16 = 0;
        pub const SIZE: usize = 2;
    }

    pub struct PDIConfig2;
    impl PDIConfig2 {
        pub const ADDRESS: u16 = 0;
        pub const SIZE: usize = 2;
    }

    pub struct Checksum;
    impl Checksum {
        pub const ADDRESS: u16 = 0;
        pub const SIZE: usize = 2;
    }

    pub struct VenderID;
    impl VenderID {
        pub const ADDRESS: u16 = 0;
        pub const SIZE: usize = 2;
    }

    pub struct ProductCode;
    impl ProductCode {
        pub const ADDRESS: u16 = 0;
        pub const SIZE: usize = 2;
    }

    pub struct RevisionNumber;
    impl RevisionNumber {
        pub const ADDRESS: u16 = 0;
        pub const SIZE: usize = 2;
    }

    pub struct SerialNumber;
    impl SerialNumber {
        pub const ADDRESS: u16 = 0;
        pub const SIZE: usize = 2;
    }

    pub struct BootstrapRxMailboxOffset;
    impl BootstrapRxMailboxOffset {
        pub const ADDRESS: u16 = 0;
        pub const SIZE: usize = 2;
    }

    pub struct BootstrapRxMailboxSize;
    impl BootstrapRxMailboxSize {
        pub const ADDRESS: u16 = 0;
        pub const SIZE: usize = 2;
    }

    pub struct BootstrapTxMailboxOffset;
    impl BootstrapTxMailboxOffset {
        pub const ADDRESS: u16 = 0;
        pub const SIZE: usize = 2;
    }

    pub struct BootstrapTxMailboxSize;
    impl BootstrapTxMailboxSize {
        pub const ADDRESS: u16 = 0;
        pub const SIZE: usize = 2;
    }

    pub struct StandardRxMailboxOffset;
    impl StandardRxMailboxOffset {
        pub const ADDRESS: u16 = 0;
        pub const SIZE: usize = 2;
    }

    pub struct StandardRxMailboxSize;
    impl StandardRxMailboxSize {
        pub const ADDRESS: u16 = 0;
        pub const SIZE: usize = 2;
    }

    pub struct StandardTxMailboxOffset;
    impl StandardTxMailboxOffset {
        pub const ADDRESS: u16 = 0;
        pub const SIZE: usize = 2;
    }

    pub struct StandardTxMailboxSize;
    impl StandardTxMailboxSize {
        pub const ADDRESS: u16 = 0;
        pub const SIZE: usize = 2;
    }

    pub struct MailboxProtocol;
    impl MailboxProtocol {
        pub const ADDRESS: u16 = 0;
        pub const SIZE: usize = 2;
    }

    pub struct Size;
    impl Size {
        pub const ADDRESS: u16 = 0;
        pub const SIZE: usize = 2;
    }

    pub struct Version;
    impl Version {
        pub const ADDRESS: u16 = 0;
        pub const SIZE: usize = 2;
    }
}
