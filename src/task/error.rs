use crate::interface::*;

use super::{
    AlStateTransferError, MailboxTaskError, NetworkInitializerError, SdoTaskError, SiiTaskError,
    SlaveInitializerError,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskError<E> {
    UnexpectedCommand,
    UnexpectedWkc(u16),
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
pub enum TaskSpecificErrorKind {
    AlStateTransfer(AlStateTransferError),
    Mailbox(MailboxTaskError),
    NetworkInitializer(NetworkInitializerError),
    Sdo(SdoTaskError),
    SlaveInitializer(SlaveInitializerError),
    Sii(SiiTaskError),
}

impl From<AlStateTransferError> for TaskSpecificErrorKind {
    fn from(err: AlStateTransferError) -> Self {
        TaskSpecificErrorKind::AlStateTransfer(err)
    }
}

impl From<MailboxTaskError> for TaskSpecificErrorKind {
    fn from(err: MailboxTaskError) -> Self {
        TaskSpecificErrorKind::Mailbox(err)
    }
}

impl From<NetworkInitializerError> for TaskSpecificErrorKind {
    fn from(err: NetworkInitializerError) -> Self {
        TaskSpecificErrorKind::NetworkInitializer(err)
    }
}

impl From<SdoTaskError> for TaskSpecificErrorKind {
    fn from(err: SdoTaskError) -> Self {
        TaskSpecificErrorKind::Sdo(err)
    }
}

impl From<SlaveInitializerError> for TaskSpecificErrorKind {
    fn from(err: SlaveInitializerError) -> Self {
        TaskSpecificErrorKind::SlaveInitializer(err)
    }
}

impl From<SiiTaskError> for TaskSpecificErrorKind {
    fn from(err: SiiTaskError) -> Self {
        TaskSpecificErrorKind::Sii(err)
    }
}

impl From<TaskError<()>> for TaskError<TaskSpecificErrorKind> {
    fn from(err: TaskError<()>) -> Self {
        match err {
            TaskError::Timeout => TaskError::Timeout,
            TaskError::UnexpectedCommand => TaskError::UnexpectedCommand,
            TaskError::UnexpectedWkc(wkc) => TaskError::UnexpectedWkc(wkc),
            TaskError::Interface(e) => TaskError::Interface(e),
            TaskError::TaskSpecific(_) => unreachable!(),
        }
    }
}

impl From<TaskError<AlStateTransferError>> for TaskError<TaskSpecificErrorKind> {
    fn from(err: TaskError<AlStateTransferError>) -> Self {
        match err {
            TaskError::Timeout => TaskError::Timeout,
            TaskError::UnexpectedCommand => TaskError::UnexpectedCommand,
            TaskError::UnexpectedWkc(wkc) => TaskError::UnexpectedWkc(wkc),
            TaskError::Interface(e) => TaskError::Interface(e),
            TaskError::TaskSpecific(e) => TaskError::TaskSpecific(e.into()),
        }
    }
}

impl From<TaskError<MailboxTaskError>> for TaskError<TaskSpecificErrorKind> {
    fn from(err: TaskError<MailboxTaskError>) -> Self {
        match err {
            TaskError::Timeout => TaskError::Timeout,
            TaskError::UnexpectedCommand => TaskError::UnexpectedCommand,
            TaskError::UnexpectedWkc(wkc) => TaskError::UnexpectedWkc(wkc),
            TaskError::Interface(e) => TaskError::Interface(e),
            TaskError::TaskSpecific(e) => TaskError::TaskSpecific(e.into()),
        }
    }
}

impl From<TaskError<NetworkInitializerError>> for TaskError<TaskSpecificErrorKind> {
    fn from(err: TaskError<NetworkInitializerError>) -> Self {
        match err {
            TaskError::Timeout => TaskError::Timeout,
            TaskError::UnexpectedCommand => TaskError::UnexpectedCommand,
            TaskError::UnexpectedWkc(wkc) => TaskError::UnexpectedWkc(wkc),
            TaskError::Interface(e) => TaskError::Interface(e),
            TaskError::TaskSpecific(e) => TaskError::TaskSpecific(e.into()),
        }
    }
}

impl From<TaskError<SdoTaskError>> for TaskError<TaskSpecificErrorKind> {
    fn from(err: TaskError<SdoTaskError>) -> Self {
        match err {
            TaskError::Timeout => TaskError::Timeout,
            TaskError::UnexpectedCommand => TaskError::UnexpectedCommand,
            TaskError::UnexpectedWkc(wkc) => TaskError::UnexpectedWkc(wkc),
            TaskError::Interface(e) => TaskError::Interface(e),
            TaskError::TaskSpecific(e) => TaskError::TaskSpecific(e.into()),
        }
    }
}

impl From<TaskError<SlaveInitializerError>> for TaskError<TaskSpecificErrorKind> {
    fn from(err: TaskError<SlaveInitializerError>) -> Self {
        match err {
            TaskError::Timeout => TaskError::Timeout,
            TaskError::UnexpectedCommand => TaskError::UnexpectedCommand,
            TaskError::UnexpectedWkc(wkc) => TaskError::UnexpectedWkc(wkc),
            TaskError::Interface(e) => TaskError::Interface(e),
            TaskError::TaskSpecific(e) => TaskError::TaskSpecific(e.into()),
        }
    }
}

impl From<TaskError<SiiTaskError>> for TaskError<TaskSpecificErrorKind> {
    fn from(err: TaskError<SiiTaskError>) -> Self {
        match err {
            TaskError::Timeout => TaskError::Timeout,
            TaskError::UnexpectedCommand => TaskError::UnexpectedCommand,
            TaskError::UnexpectedWkc(wkc) => TaskError::UnexpectedWkc(wkc),
            TaskError::Interface(e) => TaskError::Interface(e),
            TaskError::TaskSpecific(e) => TaskError::TaskSpecific(e.into()),
        }
    }
}
