use core::time::Duration;

#[derive(Debug, Clone, Copy, PartialOrd, PartialEq, Ord, Eq, Hash)]
pub enum CycleTime {
    T500Micros = 500_000,
    T1Millis = 500_000 * 2,
    T2Millis = 500_000 * 4,
    T4Millis = 500_000 * 8,
    T8Millis = 500_000 * 16,
    T16Millis = 500_000 * 32,
}

impl CycleTime {
    pub fn as_seconds(&self) -> f64 {
        ((*self as u64) as f64) / 1_000_000_000.0
    }
    pub fn as_duration(&self) -> Duration {
        Duration::from_nanos(*self as u64)
    }
}
