use crate::interface::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskError<E> {
    UnexpectedCommand,
    UnexpectedWkc(UnexpectedWkc),
    Interface(PhyError),
    TaskSpecific(E),
    Timeout,
}

impl<E> From<PhyError> for TaskError<E> {
    fn from(err: PhyError) -> Self {
        Self::Interface(err)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnexpectedWkc {
    pub expected: u16,
    pub recieved: u16,
}

impl From<(u16, u16)> for UnexpectedWkc {
    fn from(v: (u16, u16)) -> Self {
        let (expected, recieved) = v;
        Self { expected, recieved }
    }
}
