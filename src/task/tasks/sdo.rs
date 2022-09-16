use super::mailbox::MailboxTaskError;
use crate::frame::AbortCode;
use crate::EcError;

#[derive(Debug, Clone, PartialEq)]
pub enum SdoTaskError {
    Mailbox(MailboxTaskError),
    MailboxAlreadyExisted,
    AbortCode(AbortCode),
    UnexpectedCommandSpecifier,
}

impl From<SdoTaskError> for EcError<SdoTaskError> {
    fn from(err: SdoTaskError) -> Self {
        Self::TaskSpecific(err)
    }
}

impl From<EcError<()>> for EcError<SdoTaskError> {
    fn from(err: EcError<()>) -> Self {
        match err {
            EcError::Interface(e) => EcError::Interface(e),
            EcError::LostPacket => EcError::LostPacket,
            EcError::UnexpectedCommand => EcError::UnexpectedCommand,
            EcError::UnexpectedWkc(e) => EcError::UnexpectedWkc(e),
            EcError::TaskSpecific(_) => unreachable!(),
            EcError::Timeout => EcError::Timeout,
        }
    }
}

impl From<EcError<MailboxTaskError>> for EcError<SdoTaskError> {
    fn from(err: EcError<MailboxTaskError>) -> Self {
        match err {
            EcError::TaskSpecific(err) => EcError::TaskSpecific(SdoTaskError::Mailbox(err)),
            EcError::Interface(e) => EcError::Interface(e),
            EcError::LostPacket => EcError::LostPacket,
            EcError::UnexpectedCommand => EcError::UnexpectedCommand,
            EcError::UnexpectedWkc(wkc) => EcError::UnexpectedWkc(wkc),
            EcError::Timeout => EcError::Timeout,
        }
    }
}
