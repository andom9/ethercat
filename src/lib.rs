#![no_std]
pub mod arch;
mod error;
pub mod interface;
pub mod master;
pub mod packet;
pub mod register;
pub mod slave_device;
pub(crate) mod util;
