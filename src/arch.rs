/// System time
pub trait EtherCATSystemTime {
    fn system_time_from_2000_1_1_as_nanos() -> u64;
}

/// Ethernet II raw packet intrface
pub trait RawPacketInterface {
    fn send(&mut self, packet: &[u8]) -> bool;
    fn recv(&mut self, rx_buffer: &mut [u8]) -> Option<usize>;
}
