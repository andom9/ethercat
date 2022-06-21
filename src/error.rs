use crate::interface;

#[derive(Debug, Clone)]
pub enum EcError<E> {
    //UnexpectedCommand,
    LostCommand,
    UnexpectedWKC(u16),
    Interface(interface::Error),
    UnitSpecific(E),
}

impl<E> From<interface::Error> for EcError<E> {
    fn from(err: interface::Error) -> Self {
        Self::Interface(err)
    }
}
