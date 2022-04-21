use fugit::MicrosDurationU32;

#[derive(Debug, Clone)]
pub enum CommonError {
    DeviceErrorTx,
    DeviceErrorRx,
    BufferExhausted,
    PacketDropped,
    UnspcifiedTimerError,
    ReceiveTimeout,
    UnexpectedWKC(u16),
}

// TODO: 整理する
//#[derive(Debug, Clone)]
//pub enum Error {
//    LargeData,
//    SmallBuffer,
//    WkcNeq(u16, u16),
//    RxError(u8),
//    UnexpectedPacket,
//    EEPROMNotOperation,
//    ALStateTransfer(u16, AlState),
//    ALStateTimeout(u64, AlState),
//    TooManySlave(usize),
//    UnimplementedFeature,
//    NotFoundSlaves,
//    NotExistSlave(u16),
//    CannotAccessEEPROM,
//    EEPROMStatusError,
//    EEPROMBusyTimeout,
//    MailboxCounterError,
//    MailboxAbort(AbortCode),
//    UnexpectedMailbox(u8),
//    SDOMaxDataLength,
//    MailboxError(MailboxErrorDetail),
//    MaxRxPdo,
//    MaxTxPdo,
//    Sync0Timeout(u64),
//    MailboxDisable,
//    MailboxTimeout(u64),
//    UnexpectedAlState(AlState, AlState),
//    NotRecieveEtherCATFrame,
//    InvalidCommand,
//    MaxMailboxLength,
//    UnableToRecievePacket,
//    UnableToSendPacket,
//}
