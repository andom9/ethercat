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
