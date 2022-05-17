#![no_std]
pub mod arch;
pub mod cyclic;
mod error;
pub mod ethercat_frame;
pub mod interface;
pub mod master;
pub mod network;
pub mod packet;
pub mod register;
pub mod slave;
pub(crate) mod util;

pub const MAILBOX_REQUEST_RETRY_TIMEOUT_DEFAULT_MS: u32 = 100;
pub const MAILBOX_RESPONSE_RETRY_TIMEOUT_DEFAULT_MS: u32 = 2000;
// Timeout. Init -> PreOp or Init -> Boot
pub const PREOP_TIMEOUT_DEFAULT_MS: u32 = 3000;
// Timeout. SafeOp -> Op or PreOp -> SafeOp
pub const SAFEOP_OP_TIMEOUT_DEFAULT_MS: u32 = 10000;
// Timeout. Op/SafeOp/PreOp/Boot -> Init or SafeOp -> PreOp
pub const BACK_TO_INIT_TIMEOUT_DEFAULT_MS: u32 = 5000;
// Timeout. Op -> SafeOp
pub const BACK_TO_SAFEOP_TIMEOUT_DEFAULT_MS: u32 = 200;

pub(crate) const LOGICAL_START_ADDRESS: u32 = 0;
