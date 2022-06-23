//TODO: Initステートから移動するときは、Siiのアクセス権をSlaveに渡すべき
//TODO: Operationステート以外では出力のシンクマネージャー（プロセスデータ）を無効化すべき？
//TODO: Siiの読み込みに対応する。
//TODO: プロセスデータでメールボックスのライトイベントを読み込んで、メールボックスの到着を確認する。
//      このとき使用するFmmuは３番目となる(3番目のFmmuはMBoxState用みたい)

#![no_std]
pub mod arch;
pub mod cyclic;
pub mod error;
pub mod ethercat_frame;
pub mod interface;
pub mod master;
pub mod network;
pub mod packet;
pub mod register;
pub mod slave;
pub(crate) mod util;
