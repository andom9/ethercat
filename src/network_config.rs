use crate::slave_status::Identification;

#[derive(Debug)]
pub struct NetworkConfig<'a> {
    slaves: &'a [SlaveConfig<'a>],
}

#[derive(Debug)]
pub struct SlaveConfig<'a> {
    name: &'a str,
    auto_incremented_address: u16,
    configured_address: u16,
    outputs: Option<SyncManagerConfig<'a>>,
    inputs: Option<SyncManagerConfig<'a>>,
    expected_id: Option<Identification>,
}

#[derive(Debug)]
pub struct SyncManagerConfig<'a> {
    pdo: &'a [PDOConfig<'a>],
}

#[derive(Debug)]
pub struct PDOConfig<'a> {
    mapping_index: u8, // e.g. 0x1600
    entries: &'a [EntryConfig],
}

#[derive(Debug, Clone)]
pub struct EntryConfig {
    index: u16,
    sub_index: u8,
    bit_length: u8,
}
