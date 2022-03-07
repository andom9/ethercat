use bit_field::BitField;
use crate::register::datalink::PortPhysics;
use core::ops::Range;

// PDOの入力しかないやつもある
// →片方だけにも対応する。
// そもそも入出力が無いやつもある（分岐スレーブとか）
// →ないやつは設定しないようにする。
// DCがないやつもある
// →DCがないやつはDC関連処理をスキップする。
// Sync0イベント同期しかないやつもある
// →DCはモードが同じじゃないと意味がない
// →sync0イベント同期はほぼ間違いなくあるからこれを標準にする。
// FMMUとSyncManagerが固定のやつもある
// →デフォルト値を調べてそのまま使えないか？
// FMMUとSMがないやつもある（分岐スレーブ）

// SMがあるか？なければ、基本的なレジスタアクセスしかできない
//  SMのタイプは？メールボックスがあるか？
//  SMのサイズは？
//  SMのスタートアドレスは？
//  SMのコントロールバイトは？
// FMMUがあるか？
//  FMMUの種類は？
// １．SMにメールボックス入出力があれば、メールボックス可能
// ２. FMMU,SMの両方にInputsがあれば、プロセスデータの入力が可能
// ３。FMMU,SMの両方にOutputsがあれば、プロセスデータの出力が可能
// ４．DCがあれば少なくとも、sync0イベント同期は可能？（と信じたい）。
// FMMUは両方あるか？なければプロセスデータは片方だけしかできない。
// DCはあるか？なければ、DCの設定はできない（ただしリファレンスクロックにはできるはず）。

#[derive(Debug, Clone, Default)]
pub struct Slave {
    pub(crate) vender_id: u16,    // read EEPROM 0x0008 or CoE 0x1018.01
    pub(crate) product_code: u16, // read EEPROM 0x000A or CoE 1018.02
    pub(crate) revision_no: u16,  // read EEPROM 0x000C or CoE 1018.03

    pub(crate) al_state: AlState,
    pub(crate) mailbox_count: u8,
    pub(crate) station_address: u16,

    pub(crate) physics: [Option<PortPhysics>; 4], // read 0x0E00

    pub(crate) fmmu_out: Option<u8>,
    pub(crate) fmmu_in: Option<u8>,
    pub(crate) fmmu_mbox: Option<u8>,

    pub(crate) sm_pd_out: Option<SyncManagerConfig>,
    pub(crate) sm_pd_in: Option<SyncManagerConfig>,
    pub(crate) sm_mbox_out: Option<SyncManagerConfig>,
    pub(crate) sm_mbox_in: Option<SyncManagerConfig>,

    pub(crate) coe: Option<CoE>,
    pub(crate) foe: Option<()>,

    pub(crate) dc: Option<DistributedClock>,
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash, Copy)]
pub enum AlState {
    Init = 0x1,
    PreOperational = 0x2,
    Bootstrap = 0x3,
    SafeOperational = 0x4,
    Operational = 0x8,
    Invalid,
}

impl From<u8> for AlState {
    fn from(v: u8) -> Self {
        if v == AlState::Init as u8 {
            AlState::Init
        } else if v == AlState::PreOperational as u8 {
            AlState::PreOperational
        } else if v == AlState::Bootstrap as u8 {
            AlState::Bootstrap
        } else if v == AlState::SafeOperational as u8 {
            AlState::SafeOperational
        } else if v == AlState::PreOperational as u8 {
            AlState::PreOperational
        } else if v == AlState::Operational as u8 {
            AlState::Operational
        } else {
            AlState::Invalid
        }
    }
}

impl Default for AlState {
    fn default() -> Self {
        AlState::Invalid
    }
}

#[derive(Debug, Clone)]
pub struct SyncManagerConfig {
    size: u16,          // read EEPROM 0x0018, 0x001A
    start_address: u16, // read EEPROM 0x0019, 0x001B
}

#[derive(Debug, Clone)]
pub struct CoE {
    pdo_assign: bool,
    pdo_config: bool,
}

#[derive(Debug, Clone)]
pub enum DistributedClock {
    FreeRun,
    Sync0Signal,
    //Sync1Signal,
    //SyncManagerEvent,
}

#[derive(Debug, Clone)]
pub struct PDOEntry {
    station_address: u16,
    logical_memory: Range<u16>,
    index: u16,
    sub_index: u8,
    bit_length: u8,
}
