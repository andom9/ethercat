use crate::AlState;

pub struct EtherCATCommand {}

pub struct MailboxCommand {}

pub struct CoECommand {
    transition: AlState,
    timeout_ns: u32,
    command_type: CoECommnadType,
    index: u16,
    sub_index: u8,
    data: [u8; 4],
}

pub enum CoECommnadType {
    SDOUpload,
    SDODownload,
}
