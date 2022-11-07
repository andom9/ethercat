//#![no_std]
#![cfg_attr(not(feature = "pcap"), no_std)]
pub mod frame;
pub mod interface;
pub mod master;
pub mod register;
pub mod slave;
pub mod task;
pub(crate) mod util;
pub use master::EtherCatMaster;
