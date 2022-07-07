use crate::cyclic_task::CommandInterfaceError;

#[derive(Debug, Clone)]
pub enum EcError<E> {
    UnexpectedCommand,
    LostPacket,
    UnexpectedWkc(u16),
    Interface(CommandInterfaceError),
    TaskSpecific(E),
}

impl<E> From<CommandInterfaceError> for EcError<E> {
    fn from(err: CommandInterfaceError) -> Self {
        Self::Interface(err)
    }
}
