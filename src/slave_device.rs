use crate::AlState;
use bit_field::BitField;
use heapless::Vec;

pub const MAX_RXPDO_ENTRY: usize = 16;
pub const MAX_TXPDO_ENTRY: usize = 16;

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
pub struct SlaveDevice {
    // current status
    al_state: AlState,
    mailbox_count: u8,

    // settings
    number: u16,
    assigned_id: u16, // write 0x0010 0x0011
    rx_pdo_mapping: heapless::Vec<PDOEntry, MAX_RXPDO_ENTRY>,
    tx_pdo_mapping: heapless::Vec<PDOEntry, MAX_TXPDO_ENTRY>,
    rx_pdo_start_offset: usize,
    rx_pdo_length: usize,
    rx_pdo_start_bit: usize,
    rx_pdo_stop_bit: usize,
    tx_pdo_start_offset: usize,
    tx_pdo_length: usize,
    tx_pdo_start_bit: usize,
    tx_pdo_stop_bit: usize,

    // info
    vender_id: u16,    // read EEPROM 0x0008 or CoE 0x1018.01
    product_code: u16, // read EEPROM 0x000A or CoE 1018.02
    revision_no: u16,  // read EEPROM 0x000C or CoE 1018.03

    physics: Vec<Physics, 4>, // read 0x0E00

    fmmu_out: Option<u8>,
    fmmu_in: Option<u8>,

    sm_out: Option<SyncManager>,
    sm_in: Option<SyncManager>,
    sm_mbox_out: Option<SyncManager>,
    sm_mbox_in: Option<SyncManager>,

    coe: Option<MailBoxCoE>,
    foe: Option<()>,

    dc: Option<DistributedClock>, // read 0x0008.2 モードはどうやって調べる？
}

impl SlaveDevice {
    pub fn new(number: u16) -> Self{
        Self{
            number,
            ..Default::default()
        }
    }

    pub fn number(&self) -> u16 {
        self.number
    }

    pub(crate) fn set_number(&mut self, number: u16) {
        self.number = number;
    }

    pub fn mailbox_count(&self) -> u8 {
        self.mailbox_count
    }

    pub(crate) fn increment_mailbox_count(&mut self) {
        self.mailbox_count = if self.mailbox_count == 7 {
            1
        } else {
            self.mailbox_count + 1
        };
    }

    pub fn rx_pdo_mapping<'a>(&'a self) -> &'a [PDOEntry] {
        &self.rx_pdo_mapping
    }

    pub fn rx_pdo_mapping_mut<'a>(&'a mut self) -> &'a mut [PDOEntry] {
        &mut self.rx_pdo_mapping
    }

    pub fn tx_pdo_mapping<'a>(&'a self) -> &'a [PDOEntry] {
        &self.tx_pdo_mapping
    }

    pub fn tx_pdo_mapping_mut<'a>(&'a mut self) -> &'a mut [PDOEntry] {
        &mut self.tx_pdo_mapping
    }

    pub fn rx_pdo_bit_size(&self) -> u16 {
        let mut size = 0;
        for entry in &self.rx_pdo_mapping {
            size += entry.size_bits();
        }
        size
    }

    pub fn tx_pdo_bit_size(&self) -> u16 {
        let mut size = 0;
        for entry in &self.tx_pdo_mapping {
            size += entry.size_bits();
        }
        size
    }

    pub fn rx_pdo_entry<'a>(&'a self, index: usize) -> Option<&'a PDOEntry> {
        self.rx_pdo_mapping.get(index)
    }

    pub fn tx_pdo_entry<'a>(&'a self, index: usize) -> Option<&'a PDOEntry> {
        self.tx_pdo_mapping.get(index)
    }

    pub fn rx_pdo_entry_mut<'a>(&'a mut self, index: usize) -> Option<&'a mut PDOEntry> {
        self.rx_pdo_mapping.get_mut(index)
    }

    pub fn tx_pdo_entry_mut<'a>(&'a mut self, index: usize) -> Option<&'a mut PDOEntry> {
        self.tx_pdo_mapping.get_mut(index)
    }

    pub fn push_rx_pdo_entry(&mut self, pdo_entry: PDOEntry) -> Result<(), PDOEntry> {
        self.rx_pdo_mapping.push(pdo_entry)
    }

    pub fn push_tx_pdo_entry(&mut self, pdo_entry: PDOEntry) -> Result<(), PDOEntry> {
        self.tx_pdo_mapping.push(pdo_entry)
    }

    pub fn rx_pdo_start_offset(&self) -> usize {
        self.rx_pdo_start_offset
    }

    pub(crate) fn set_rx_pdo_start_offset(&mut self, offset: usize) {
        self.rx_pdo_start_offset = offset;
    }

    pub fn rx_pdo_length(&self) -> usize {
        self.rx_pdo_length
    }

    pub(crate) fn set_rx_pdo_length(&mut self, length: usize) {
        self.rx_pdo_length = length;
    }

    pub fn rx_pdo_start_bit(&self) -> usize {
        self.rx_pdo_start_bit
    }

    pub fn set_rx_pdo_start_bit(&mut self, start_bit: usize) {
        self.rx_pdo_start_bit = start_bit;
    }

    pub fn rx_pdo_stop_bit(&self) -> usize {
        self.rx_pdo_stop_bit
    }

    pub fn set_rx_pdo_stop_bit(&mut self, stop_bit: usize) {
        self.rx_pdo_stop_bit = stop_bit;
    }

    pub fn tx_pdo_start_offset(&self) -> usize {
        self.tx_pdo_start_offset
    }

    pub(crate) fn set_tx_pdo_start_offset(&mut self, offset: usize) {
        self.tx_pdo_start_offset = offset;
    }

    pub fn tx_pdo_length(&self) -> usize {
        self.tx_pdo_length
    }

    pub(crate) fn set_tx_pdo_length(&mut self, length: usize) {
        self.tx_pdo_length = length;
    }

    pub fn tx_pdo_start_bit(&self) -> usize {
        self.tx_pdo_start_bit
    }

    pub fn set_tx_pdo_start_bit(&mut self, start_bit: usize) {
        self.tx_pdo_start_bit = start_bit;
    }

    pub fn tx_pdo_stop_bit(&self) -> usize {
        self.tx_pdo_stop_bit
    }

    pub fn set_tx_pdo_stop_bit(&mut self, stop_bit: usize) {
        self.tx_pdo_stop_bit = stop_bit;
    }
}

#[derive(Debug, Clone)]
pub enum Physics {
    MII,
    EBUS,
}

#[derive(Debug, Clone)]
pub struct SyncManager {
    default_size: Option<u16>, // read EEPROM 0x0018, 0x001A
    start_address: u16,        // read EEPROM 0x0019, 0x001B
}

#[derive(Debug, Clone)]
pub struct MailBoxCoE {
    pdo_assign: bool,
    pdo_config: bool,
}

#[derive(Debug, Clone)]
pub struct DistributedClock {
    assign_activate: u16,
}

impl DistributedClock {
    pub fn is_cycle_operation_active(&self) -> bool {
        self.assign_activate.get_bit(0)
    }

    pub fn is_sync0_output_active(&self) -> bool {
        self.assign_activate.get_bit(1)
    }

    pub fn is_sync1_output_active(&self) -> bool {
        self.assign_activate.get_bit(2)
    }
}

#[derive(Debug, Clone)]
pub struct PDOEntry {
    address: u16,
    size_bits: u16,
    data: [u8; 4],
}

impl PDOEntry {
    pub fn new(address: u16, size_bits: u16) -> Option<Self> {
        if size_bits > 4 * 8 {
            return None;
        }
        Some(Self {
            address,
            size_bits,
            data: [0; 4],
        })
    }

    pub fn address(&self) -> u16 {
        self.address
    }

    pub fn size_bits(&self) -> u16 {
        self.size_bits
    }

    pub fn data<'a>(&'a self) -> &'a [u8; 4] {
        &self.data
    }

    pub fn data_mut<'a>(&'a mut self) -> &'a mut [u8; 4] {
        &mut self.data
    }
}

impl Default for SlaveDevice {
    fn default() -> Self {
        Self {
            al_state: AlState::Init,
            number: 0,
            assigned_id: 0,
            vender_id: 0,
            product_code: 0,
            revision_no: 0,
            physics: Vec::default(),
            fmmu_in: None,
            fmmu_out: None,
            sm_in: None,
            sm_out: None,
            sm_mbox_in: None,
            sm_mbox_out: None,
            coe: None,
            foe: None,
            dc: None,
            rx_pdo_mapping: Vec::default(),
            tx_pdo_mapping: Vec::default(),
            rx_pdo_start_offset: 0,
            rx_pdo_length: 0,
            rx_pdo_start_bit: 0,
            rx_pdo_stop_bit: 0,
            tx_pdo_start_offset: 0,
            tx_pdo_length: 0,
            tx_pdo_start_bit: 0,
            tx_pdo_stop_bit: 0,
            mailbox_count: 1,
        }
    }
}
