use heapless::Vec;
use crate::slave_device::{MAX_RXPDO_ENTRY, MAX_TXPDO_ENTRY};
use crate::master::{MAX_SLAVES};
use super::command;

pub struct Config{
    slaves: Vec<SlaveConfig, MAX_SLAVES>,
}

pub struct SlaveConfig{
    name: &'static str,
    station_address: u32,
    process_data: Option<ProcessDataConfig>,
    mailbox: Option<MailboxConfig>,
    dc: Option<DCConfig>,
}

pub struct ProcessDataConfig{
    send: Vec<EntryConfig, MAX_TXPDO_ENTRY>,
    recv: Vec<EntryConfig, MAX_TXPDO_ENTRY>,
}

pub struct EntryConfig{
    address: u32,
    bit_length: u32,
}

pub struct MailboxConfig{
    coe: Option<CoEConfig>,
}

pub struct CoEConfig{}

pub struct DCConfig{
    sync0_cycle_time_ns: Option<u32>,
}