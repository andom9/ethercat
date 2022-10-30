use crate::{
    frame::{CoE, Mailbox, Message},
    slave::Slave,
};

use super::mailbox::{self, MailboxRequestInterface, MailboxSessionId};

#[derive(Debug)]
pub struct SdoManager {
    session_id: Option<MailboxSessionId>,
}

impl SdoManager {
    pub fn session_id(&self) -> Option<&MailboxSessionId> {
        self.session_id.as_ref()
    }

    pub fn is_same_session_id(&self, session_id: &MailboxSessionId) -> bool {
        if let Some(ref id) = self.session_id {
            id == session_id
        } else {
            false
        }
    }

    pub fn write_request(
        &mut self,
        writer: &mut MailboxRequestInterface,
        slave: &Slave,
        index: u16,
        sub_index: u8,
        data: &[u8],
    ) {
        let message = Message::new_sdo_download_request(index, sub_index, data);
        let mut mailbox = Mailbox::new(0, 0, message);
        let session_id = writer.request(slave, &mut mailbox);
        self.session_id = Some(session_id);
    }
}
