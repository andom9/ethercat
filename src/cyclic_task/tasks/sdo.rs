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
            EcError::TaskSpecific(e) => unreachable!(),
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
        }
    }
}

// #[derive(Debug, Clone, PartialEq)]
// enum State {
//     Error(EcError<SdoTaskError>),
//     Idle,
//     Complete,
//     Processing,
// }

// impl Default for State {
//     fn default() -> Self {
//         Self::Idle
//     }
// }

// #[derive(Debug)]
// enum Inner {
//     Reader(SdoUploader),
//     Writer(SdoDownloader),
//     //Taked,
// }

// //impl<'a> Inner<'a> {
// //    fn take_buffer(self) -> &'a mut [u8] {
// //        match self {
// //            Inner::Reader(reader) => reader.take_buffer(),
// //            Inner::Writer(writer) => writer.take_buffer(),
// //            Inner::Taked => unreachable!(),
// //        }
// //    }
// //}

// //impl<'a> Default for Inner<'a> {
// //    fn default() -> Self {
// //        Inner::Taked
// //    }
// //}

// #[derive(Debug)]
// pub struct SdoTask {
//     state: State,
//     inner: Inner,
// }

// impl SdoTask {
//     pub fn new() -> Self {
//         Self {
//             state: State::Idle,
//             inner: Inner::Reader(SdoUploader::new()),
//         }
//     }

//     //pub fn take_buffer(self) -> &'a mut [u8] {
//     //    self.inner.take_buffer()
//     //}

//     pub fn sdo_data<'a>(&self, mb_data: &'a [u8]) -> &'a [u8] {
//         match &self.inner {
//             Inner::Reader(reader) => reader.sdo_data(mb_data),
//             Inner::Writer(_) => &[],
//             //Inner::Taked => unreachable!(),
//         }
//         //SdoUploader::sdo_data(mb_data)
//     }

//     pub fn start_to_read(
//         &mut self,
//         slave_info: &SlaveInfo,
//         index: u16,
//         sub_index: u8,
//         buf: &mut [u8],
//     ) {
//         //let inner = core::mem::take(&mut self.inner);
//         //let buf = inner.take_buffer();
//         let mut reader = SdoUploader::new();
//         reader.start(slave_info, index, sub_index, buf);
//         self.inner = Inner::Reader(reader);
//         self.state = State::Processing;
//     }

//     pub fn start_to_write(
//         &mut self,
//         slave_info: &SlaveInfo,
//         index: u16,
//         sub_index: u8,
//         data: &[u8],
//         buf: &mut [u8],
//     ) {
//         //let inner = core::mem::take(&mut self.inner);
//         //let buf = inner.take_buffer();
//         let mut writer = SdoDownloader::new();
//         writer.start(slave_info, index, sub_index, data, buf);
//         self.inner = Inner::Writer(writer);
//         self.state = State::Processing;
//     }

//     pub fn wait<'b>(&'b self) -> Option<Result<(), EcError<SdoTaskError>>> {
//         match &self.state {
//             State::Complete => Some(Ok(())),
//             State::Error(err) => Some(Err(err.clone())),
//             _ => None,
//         }
//     }
// }

// impl Cyclic for SdoTask {
//     fn is_finished(&self) -> bool {
//         self.state == State::Complete
//     }

//     fn next_command(&mut self, buf: &mut [u8]) -> Option<(Command, usize)> {
//         match self.state {
//             State::Idle => None,
//             State::Error(_) => None,
//             State::Complete => None,
//             State::Processing => match &mut self.inner {
//                 Inner::Reader(reader) => reader.next_command(buf),
//                 Inner::Writer(writer) => writer.next_command(buf),
//                 //Inner::Taked => unreachable!(),
//             },
//         }
//     }

//     fn recieve_and_process(
//         &mut self,
//         recv_data: Option<&CommandData>,
//         sys_time: EtherCatSystemTime,
//     ) {
//         match self.state {
//             State::Idle => {}
//             State::Error(_) => {}
//             State::Complete => {}
//             State::Processing => match &mut self.inner {
//                 Inner::Reader(reader) => {
//                     reader.recieve_and_process(recv_data, sys_time);
//                     match reader.wait() {
//                         None => {}
//                         Some(Ok(_)) => self.state = State::Complete,
//                         Some(Err(err)) => self.state = State::Error(err),
//                     }
//                 }
//                 Inner::Writer(writer) => {
//                     writer.recieve_and_process(recv_data, sys_time);
//                     match writer.wait() {
//                         None => {}
//                         Some(Ok(_)) => self.state = State::Complete,
//                         Some(Err(err)) => self.state = State::Error(err),
//                     }
//                 } //Inner::Taked => unreachable!(),
//             },
//         }
//     }
// }
