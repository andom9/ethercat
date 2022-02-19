use crate::al_state::AlState;
use crate::packet::coe::AbortCode;
use crate::packet::ethercat::MailboxErrorDetail;

#[derive(Debug, Clone)]
pub enum PacketError {
    LargeData,
    SmallBuffer,
}

#[derive(Debug, Clone)]
pub enum EtherCATError {
    PacketError(PacketError),
    WkcNeq(u16, u16),
    RxError(u8),
    UnexpectedPacket,
    EEPROMNotOperation,
    ALStateTransfer(u16, AlState),
    ALStateTimeout(u64, AlState),
    TooManySlave(usize),
    Unimplemented(UnimplementedKind),
    NotFoundSlaves,
    NotExistSlave(u16),
    CannotAccessEEPROM,
    EEPROMStatusError,
    EEPROMBusyTimeout,
    MailboxCounterError,
    MailboxAbort(AbortCode),
    UnexpectedMailbox(u8),
    SDOMaxDataLength,
    MailboxError(MailboxErrorDetail),
    MaxRxPdo,
    MaxTxPdo,
    Sync0Timeout(u64),
    MailboxDisable,
    MailboxTimeout(u64),
    UnexpectedAlState(AlState, AlState),
    NotRecieveEtherCATPacket,
    InvalidCommand,
    MaxMailboxLength,
    UnableToRecievePacket,
    UnableToSendPacket,
}

#[derive(Debug, Clone)]
pub enum UnimplementedKind {
    NoDCSlave,
    NoLRWSlave,
    FixedFMMU,
    Topology,
    UnsupportedBus(u16, u8, u8),
}

impl From<PacketError> for EtherCATError {
    fn from(packet_error: PacketError) -> Self {
        Self::PacketError(packet_error)
    }
}
