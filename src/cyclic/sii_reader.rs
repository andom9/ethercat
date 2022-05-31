use crate::cyclic::Cyclic;
use crate::error::CommonError;
use crate::interface::*;
use crate::network::*;
use crate::register::datalink::*;
use crate::util::*;

use super::EtherCATSystemTime;
use super::ReceivedData;

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
    Wait,
    Read,
    ResetOwnership,
    Complete,
}

impl Default for SIIState {
    fn default() -> Self {
        Self::Init
    }
}

#[derive(Debug)]
pub struct SIIReader {
    timer_start: EtherCATSystemTime,
    command: Command,
    slave_address: SlaveAddress,
    buffer: [u8; buffer_size()],
    state: SIIState,
    sii_address: u16,
    read_size: usize,
}

impl SIIReader {
    pub fn new() -> Self {
        Self {
            timer_start: EtherCATSystemTime(0),
            state: SIIState::Idle,
            slave_address: SlaveAddress::SlavePosition(0),
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
}

impl Cyclic for SIIReader {
    fn next_command(
        &mut self,
        _: &mut NetworkDescription,
        _: EtherCATSystemTime,
    ) -> Option<(Command, &[u8])> {
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
            SIIState::Wait => {
                self.command = Command::new_read(self.slave_address, SIIControl::ADDRESS);
                self.buffer.fill(0);
                Some((self.command, &self.buffer[..SIIControl::SIZE]))
            }
            SIIState::Read => {
                self.command = Command::new_read(self.slave_address, SIIData::ADDRESS);
                self.buffer.fill(0);
                Some((self.command, &self.buffer[..SIIData::SIZE]))
            }
            SIIState::ResetOwnership => {
                self.buffer.fill(0);
                let mut sii_access = SIIAccess(self.buffer);
                sii_access.set_owner(true);
                sii_access.set_reset_access(false);
                self.command = Command::new_write(self.slave_address, SIIAccess::ADDRESS);
                Some((self.command, &self.buffer[..SIIAccess::SIZE]))
            }
            SIIState::Complete => None,
        }
    }

    fn recieve_and_process(
        &mut self,
        recv_data: Option<ReceivedData>,
        _: &mut NetworkDescription,
        sys_time: EtherCATSystemTime,
    ) {
        let data = if let Some(recv_data) = recv_data {
            let ReceivedData { command, data, wkc } = recv_data;
            if command != self.command {
                self.state = SIIState::Error(SIIError::Common(CommonError::BadPacket));
            }
            if wkc != 1 {
                self.state = SIIState::Error(SIIError::Common(CommonError::UnexpectedWKC(wkc)));
            }
            data
        } else {
            self.state = SIIState::Error(SIIError::Common(CommonError::LostCommand));
            return;
        };

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
                self.state = SIIState::Wait;
                self.timer_start = sys_time;
            }
            SIIState::Wait => {
                let sii_control = SIIControl(data);
                if sii_control.command_error() {
                    self.state = SIIState::Error(SIIError::CommandError);
                } else if !sii_control.busy() && !sii_control.read_operation() {
                    self.state = SIIState::Read;
                } else {
                    if self.timer_start.0 < sys_time.0
                        && TIMEOUT_MS as u64 * 1000 < sys_time.0 - self.timer_start.0
                    {
                        self.state = SIIState::Error(SIIError::TimeoutMs(TIMEOUT_MS))
                    }
                }
            }
            SIIState::Read => {
                self.state = SIIState::ResetOwnership;
            }
            SIIState::ResetOwnership => {
                self.state = SIIState::Complete;
            }
            SIIState::Complete => {}
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
        pub const ADDRESS: u16 = 1;
        pub const SIZE: usize = 2;
    }

    pub struct SyncImpulseLen;
    impl SyncImpulseLen {
        pub const ADDRESS: u16 = 2;
        pub const SIZE: usize = 2;
    }

    pub struct PDIConfig2;
    impl PDIConfig2 {
        pub const ADDRESS: u16 = 3;
        pub const SIZE: usize = 2;
    }

    pub struct StationAlias;
    impl StationAlias {
        pub const ADDRESS: u16 = 4;
        pub const SIZE: usize = 2;
    }

    pub struct Checksum;
    impl Checksum {
        pub const ADDRESS: u16 = 7;
        pub const SIZE: usize = 2;
    }

    pub struct VenderID;
    impl VenderID {
        pub const ADDRESS: u16 = 8;
        pub const SIZE: usize = 2;
    }

    pub struct ProductCode;
    impl ProductCode {
        pub const ADDRESS: u16 = 0xA;
        pub const SIZE: usize = 2;
    }

    pub struct RevisionNumber;
    impl RevisionNumber {
        pub const ADDRESS: u16 = 0xC;
        pub const SIZE: usize = 2;
    }

    pub struct SerialNumber;
    impl SerialNumber {
        pub const ADDRESS: u16 = 0xE;
        pub const SIZE: usize = 2;
    }

    pub struct BootstrapRxMailboxOffset;
    impl BootstrapRxMailboxOffset {
        pub const ADDRESS: u16 = 0x14;
        pub const SIZE: usize = 2;
    }

    pub struct BootstrapRxMailboxSize;
    impl BootstrapRxMailboxSize {
        pub const ADDRESS: u16 = 0x15;
        pub const SIZE: usize = 2;
    }

    pub struct BootstrapTxMailboxOffset;
    impl BootstrapTxMailboxOffset {
        pub const ADDRESS: u16 = 0x16;
        pub const SIZE: usize = 2;
    }

    pub struct BootstrapTxMailboxSize;
    impl BootstrapTxMailboxSize {
        pub const ADDRESS: u16 = 0x17;
        pub const SIZE: usize = 2;
    }

    pub struct StandardRxMailboxOffset;
    impl StandardRxMailboxOffset {
        pub const ADDRESS: u16 = 0x18;
        pub const SIZE: usize = 2;
    }

    pub struct StandardRxMailboxSize;
    impl StandardRxMailboxSize {
        pub const ADDRESS: u16 = 0x19;
        pub const SIZE: usize = 2;
    }

    pub struct StandardTxMailboxOffset;
    impl StandardTxMailboxOffset {
        pub const ADDRESS: u16 = 0x1A;
        pub const SIZE: usize = 2;
    }

    pub struct StandardTxMailboxSize;
    impl StandardTxMailboxSize {
        pub const ADDRESS: u16 = 0x1B;
        pub const SIZE: usize = 2;
    }

    pub struct MailboxProtocol;
    impl MailboxProtocol {
        pub const ADDRESS: u16 = 0x1C;
        pub const SIZE: usize = 2;
    }

    pub struct Size;
    impl Size {
        pub const ADDRESS: u16 = 0x3E;
        pub const SIZE: usize = 2;
    }

    pub struct Version;
    impl Version {
        pub const ADDRESS: u16 = 0x3F;
        pub const SIZE: usize = 2;
    }
}
