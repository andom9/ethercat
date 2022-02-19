pub trait EtherCatEpoch {
    fn system_time_from_2000_1_1_as_nanos() -> u64;
}

pub trait RawPacketInterface {
    fn send(&mut self, packet: &[u8]) -> bool;
    fn recv(&mut self, rx_buffer: &mut [u8]) -> Option<usize>;
}
