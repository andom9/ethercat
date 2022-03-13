#![no_std]
pub mod arch;
mod error;
pub mod ethercat_frame;
pub mod interface;
//pub mod master;
pub mod packet;
pub mod register;
pub mod sii;
pub mod slave_device;
pub(crate) mod util;
