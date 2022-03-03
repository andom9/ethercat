#![no_std]
pub mod arch;
pub mod datalink;
mod error;
pub mod master;
pub mod packet;
pub mod slave_device;
pub(crate) mod util;

