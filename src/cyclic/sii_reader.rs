use crate::cyclic::Cyclic;
use crate::error::CommonError;
use crate::interface::{Command, SlaveAddress};
use crate::network::NetworkDescription;
use crate::register::datalink::{SIIAccess, SIIAddress, SIIControl, SIIData};
use crate::util::const_max;

use super::EtherCatSystemTime;
use super::ReceivedData;

const TIMEOUT_MS: u32 = 100;

#[derive(Debug, Clone)]
pub enum Error {
    Common(CommonError),
    PermittionDenied,
    AddressSizeOver,
    Busy,
    CheckSumError,
    DeviceInfoError,
    CommandError,
    TimeoutMs(u32),
}

impl From<CommonError> for Error {
    fn from(err: CommonError) -> Self {
        Self::Common(err)
    }
}

#[derive(Debug)]
enum State {
    Error(Error),
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

impl Default for State {
    fn default() -> Self {
        Self::Init
    }
}

#[derive(Debug)]
pub struct SIIReader {
    timer_start: EtherCatSystemTime,
    command: Command,
    slave_address: SlaveAddress,
    buffer: [u8; buffer_size()],
    state: State,
    sii_address: u16,
    read_size: usize,
}

impl SIIReader {
    pub fn new() -> Self {
        Self {
            timer_start: EtherCatSystemTime(0),
            state: State::Idle,
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
        self.state = State::Init;
        self.buffer.fill(0);
        self.command = Command::default();
    }

    pub fn wait(&mut self) -> nb::Result<(SIIData<[u8; SIIData::SIZE]>, usize), Error> {
        match &self.state {
            State::Complete => Ok((SIIData(self.buffer.clone()), self.read_size)),
            State::Error(err) => Err(nb::Error::Other(err.clone())),
            _ => Err(nb::Error::WouldBlock),
        }
    }
}

impl Cyclic for SIIReader {
    fn next_command(
        &mut self,
        _: &mut NetworkDescription,
        _: EtherCatSystemTime,
    ) -> Option<(Command, &[u8])> {
        match self.state {
            State::Idle => None,
            State::Error(_) => None,
            State::Init => {
                self.command = Command::new_read(self.slave_address, SIIControl::ADDRESS);
                self.buffer.fill(0);
                Some((self.command, &self.buffer[..SIIControl::SIZE]))
            }
            State::SetOwnership => {
                self.buffer.fill(0);
                let mut sii_access = SIIAccess(self.buffer);
                sii_access.set_owner(false);
                sii_access.set_reset_access(true);
                self.command = Command::new_write(self.slave_address, SIIAccess::ADDRESS);
                Some((self.command, &self.buffer[..SIIAccess::SIZE]))
            }
            State::CheckOwnership => {
                self.command = Command::new_read(self.slave_address, SIIAccess::ADDRESS);
                self.buffer.fill(0);
                Some((self.command, &self.buffer[..SIIAccess::SIZE]))
            }
            State::SetAddress => {
                self.buffer.fill(0);
                let mut sii_address = SIIAddress(self.buffer);
                sii_address.set_sii_address(self.sii_address as u32);
                self.command = Command::new_write(self.slave_address, SIIAddress::ADDRESS);
                Some((self.command, &self.buffer[..SIIAddress::SIZE]))
            }
            State::SetReadOperation => {
                self.buffer.fill(0);
                let mut sii_address = SIIControl(self.buffer);
                sii_address.set_read_operation(true);
                self.command = Command::new_write(self.slave_address, SIIControl::ADDRESS);
                Some((self.command, &self.buffer[..SIIControl::SIZE]))
            }
            State::Wait => {
                self.command = Command::new_read(self.slave_address, SIIControl::ADDRESS);
                self.buffer.fill(0);
                Some((self.command, &self.buffer[..SIIControl::SIZE]))
            }
            State::Read => {
                self.command = Command::new_read(self.slave_address, SIIData::ADDRESS);
                self.buffer.fill(0);
                Some((self.command, &self.buffer[..SIIData::SIZE]))
            }
            State::ResetOwnership => {
                self.buffer.fill(0);
                let mut sii_access = SIIAccess(self.buffer);
                sii_access.set_owner(true);
                sii_access.set_reset_access(false);
                self.command = Command::new_write(self.slave_address, SIIAccess::ADDRESS);
                Some((self.command, &self.buffer[..SIIAccess::SIZE]))
            }
            State::Complete => None,
        }
    }

    fn recieve_and_process(
        &mut self,
        recv_data: Option<ReceivedData>,
        _: &mut NetworkDescription,
        sys_time: EtherCatSystemTime,
    ) {
        let data = if let Some(recv_data) = recv_data {
            let ReceivedData { command, data, wkc } = recv_data;
            if command != self.command {
                self.state = State::Error(Error::Common(CommonError::BadPacket));
            }
            if wkc != 1 {
                self.state = State::Error(Error::Common(CommonError::UnexpectedWKC(wkc)));
            }
            data
        } else {
            self.state = State::Error(Error::Common(CommonError::LostCommand));
            return;
        };

        match self.state {
            State::Idle => {}
            State::Error(_) => {}
            State::Init => {
                let sii_control = SIIControl(data);
                if sii_control.check_sum_error() {
                    self.state = State::Error(Error::CheckSumError);
                }
                if sii_control.device_info_error() {
                    self.state = State::Error(Error::DeviceInfoError);
                }

                // アドレスアルゴリズムが0なら、アドレスは1オクテットでなければならない。
                if !sii_control.address_algorithm() && self.sii_address >> 8 != 0 {
                    self.state = State::Error(Error::AddressSizeOver);
                }

                if sii_control.busy()
                    || sii_control.read_operation()
                    || sii_control.write_operation()
                    || sii_control.reload_operation()
                {
                    self.state = State::Error(Error::Busy);
                }

                self.read_size = if sii_control.read_size() { 8 } else { 4 };

                self.state = State::SetOwnership;
            }
            State::SetOwnership => {
                self.state = State::CheckOwnership;
            }
            State::CheckOwnership => {
                let sii_access = SIIAccess(data);
                if sii_access.owner() || sii_access.pdi_accessed() {
                    self.state = State::Error(Error::PermittionDenied);
                }
                self.state = State::SetAddress;
            }
            State::SetAddress => {
                self.state = State::SetReadOperation;
            }
            State::SetReadOperation => {
                self.state = State::Wait;
                self.timer_start = sys_time;
            }
            State::Wait => {
                let sii_control = SIIControl(data);
                if sii_control.command_error() {
                    self.state = State::Error(Error::CommandError);
                } else if !sii_control.busy() && !sii_control.read_operation() {
                    self.state = State::Read;
                } else {
                    if self.timer_start.0 < sys_time.0
                        && TIMEOUT_MS as u64 * 1000 < sys_time.0 - self.timer_start.0
                    {
                        self.state = State::Error(Error::TimeoutMs(TIMEOUT_MS))
                    }
                }
            }
            State::Read => {
                self.state = State::ResetOwnership;
            }
            State::ResetOwnership => {
                self.state = State::Complete;
            }
            State::Complete => {}
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
    pub struct PdiControl;
    impl PdiControl {
        pub const ADDRESS: u16 = 0;
        pub const SIZE: usize = 2;
    }

    pub struct PdiConfig;
    impl PdiConfig {
        pub const ADDRESS: u16 = 1;
        pub const SIZE: usize = 2;
    }

    pub struct SyncImpulseLen;
    impl SyncImpulseLen {
        pub const ADDRESS: u16 = 2;
        pub const SIZE: usize = 2;
    }

    pub struct PdiConfig2;
    impl PdiConfig2 {
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
