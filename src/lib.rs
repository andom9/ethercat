#![no_std]
pub(crate) mod al_state;
pub mod arch;
pub mod config;
pub(crate) mod dc;
pub(crate) mod eeprom;
mod error;
pub mod iface;
pub(crate) mod mailbox;
pub mod master;
pub(crate) mod packet;
pub(crate) mod sdo;
pub mod slave_device;
pub(crate) mod util;

pub use al_state::AlState;
pub use error::Error;

// ・プロセスデータとメールボックスをそれぞれソケットのように扱って、受け取ったパケットをディスパッチする。
// ・プロセスデータへの割り付けと解釈は別モジュールでやったほうがいいか？
//   例えば、マスターはあくまでも、生データの送受信に徹するとか。
