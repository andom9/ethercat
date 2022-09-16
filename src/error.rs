use crate::task::CommandInterfaceError;

#[derive(Debug, Clone, PartialEq)]
pub enum EcError<E> {
    UnexpectedCommand,
    LostPacket,
    UnexpectedWkc(u16),
    Interface(CommandInterfaceError),
    TaskSpecific(E),
    Timeout,
}

impl<E> From<CommandInterfaceError> for EcError<E> {
    fn from(err: CommandInterfaceError) -> Self {
        Self::Interface(err)
    }
}
