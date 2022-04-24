use crate::register::datalink::PortPhysics;
use heapless::Deque;

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

#[derive(Debug, Clone)]
pub enum SlaveError {
    PDINotOperational,
    UnexpectedALState,
    SMSettingsAreNotCorrect,
    WatchdogTimeout,
    PDOStateError,
    PDOControlError,
    PDOToggleError,
    EarlySMEvnet,
    SMEvnetJitterTooMuch,
    SMEventNotRecieved,
    OutputCalcAndCopyNotFinished,
    Sync0NotRecieved,
    Sync1NotRecieved,
    SyncEventNotDetected,
}

#[derive(Debug, Clone, Default)]
pub struct Identification {
    pub(crate) vender_id: u16,
    pub(crate) product_code: u16,
    pub(crate) revision_number: u16,
}

#[derive(Debug, Default)]
pub struct Slave {
    pub(crate) error: Option<SlaveError>,
    pub(crate) error_history: Deque<SlaveError, 10>,

    pub(crate) configured_address: u16,
    pub(crate) position_address: u16,
    pub(crate) id: Identification,
    pub(crate) al_state: AlState,

    pub(crate) mailbox_count: u8,

    pub(crate) linked_ports: [bool; 4],
    pub(crate) ports: [Option<PortPhysics>; 4], // read 0x0E00

    pub(crate) ram_size_kb: u8,

    pub(crate) fmmu0: Option<u16>,
    pub(crate) fmmu1: Option<u16>,

    pub(crate) number_of_sm: u8,
    pub(crate) pdo_start_address: Option<u16>,
    pub(crate) pdo_ram_size: u16,
    pub(crate) rx_pdo_mapping: Option<&'static mut [PDOMapping]>,
    pub(crate) tx_pdo_mapping: Option<&'static mut [PDOMapping]>,

    pub(crate) sm_mailbox_rx: Option<MailboxSyncManager>,
    pub(crate) sm_mailbox_tx: Option<MailboxSyncManager>,
    //pub(crate) bootstrap_sm_mailbox_in: Option<MailboxSyncManager>,
    //pub(crate) bootstrap_sm_mailbox_out: Option<MailboxSyncManager>,

    pub(crate) support_dc: bool,
    pub(crate) is_dc_range_64bits: bool,
    pub(crate) support_fmmu_bit_operation: bool,
    pub(crate) support_lrw: bool,
    pub(crate) support_rw: bool,

    pub(crate) operation_mode: OperationMode,

    pub(crate) has_coe: bool,
    //pub(crate) has_foe: bool,
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
pub struct MailboxSyncManager {
    pub size: u16,
    pub start_address: u16,
}

//#[derive(Debug)]
//pub struct ProcessDataSyncManager {
//    start_address: u16,
//    pdo_mapping: &'static mut [PDOMapping],
//}

#[derive(Debug, Clone)]
pub enum OperationMode {
    FreeRun,
    Sync0Event,
    Sync1Event,
    SyncManagerEvent,
}

impl Default for OperationMode {
    fn default() -> Self {
        OperationMode::FreeRun
    }
}

#[derive(Debug)]
pub struct PDOMapping {
    index: u16,
    entries: &'static mut [PDOEntry],
}

#[derive(Debug)]
pub struct PDOEntry {
    index: u16,
    sub_index: u8,
    byte_length: u8, // NOTE: not bit length
    data: &'static mut [u8],
}

pub(crate) fn process_cyclic_data(datagram: &mut [u8], slaves: &mut [Slave]) {
    let mut offset = 0;
    let len = slaves.len();
    for i in 0..len {
        let slave = &mut slaves[i];
        //先にRxPDOを並べているとする
        if let Some(ref mut sm_in) = slave.rx_pdo_mapping {
            for pdo_mapping in sm_in.iter_mut() {
                for pdo in pdo_mapping.entries.iter_mut() {
                    let byte_length = pdo.byte_length as usize;
                    pdo.data
                        .copy_from_slice(&datagram[offset..offset + byte_length]);
                    offset += byte_length;
                }
            }
        }
        //RxPDOの後にTxPDOを並べているとする
        if let Some(ref mut sm_out) = slave.tx_pdo_mapping {
            for pdo_mapping in sm_out.iter_mut() {
                for pdo in pdo_mapping.entries.iter_mut() {
                    let byte_length = pdo.byte_length as usize;
                    datagram[offset..offset + byte_length].copy_from_slice(&pdo.data);
                    offset += byte_length;
                }
            }
        }
    }
}
