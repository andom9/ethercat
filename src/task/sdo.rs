use super::mailbox::MailboxTaskError;
use super::TaskError;
use crate::frame::AbortCode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SdoTaskError {
    Mailbox(MailboxTaskError),
    MailboxAlreadyExisted,
    AbortCode(AbortCode),
    UnexpectedCommandSpecifier,
}

impl From<SdoTaskError> for TaskError<SdoTaskError> {
    fn from(err: SdoTaskError) -> Self {
        Self::TaskSpecific(err)
    }
}

impl From<TaskError<()>> for TaskError<SdoTaskError> {
    fn from(err: TaskError<()>) -> Self {
        match err {
            TaskError::Interface(e) => TaskError::Interface(e),
            TaskError::UnexpectedCommand => TaskError::UnexpectedCommand,
            TaskError::UnexpectedWkc(e) => TaskError::UnexpectedWkc(e),
            TaskError::TaskSpecific(_) => unreachable!(),
            TaskError::Timeout => TaskError::Timeout,
        }
    }
}

impl From<TaskError<MailboxTaskError>> for TaskError<SdoTaskError> {
    fn from(err: TaskError<MailboxTaskError>) -> Self {
        match err {
            TaskError::TaskSpecific(err) => TaskError::TaskSpecific(SdoTaskError::Mailbox(err)),
            TaskError::Interface(e) => TaskError::Interface(e),
            TaskError::UnexpectedCommand => TaskError::UnexpectedCommand,
            TaskError::UnexpectedWkc(wkc) => TaskError::UnexpectedWkc(wkc),
            TaskError::Timeout => TaskError::Timeout,
        }
    }
}
