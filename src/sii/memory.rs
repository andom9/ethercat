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
