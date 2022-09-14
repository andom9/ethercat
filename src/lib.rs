//TODO: Initステートから移動するときは、Siiのアクセス権をSlaveに渡すべき
//TODO: Operationステート以外では出力のシンクマネージャー（プロセスデータ）を無効化すべき？
//TODO: Siiの読み込みに対応する。
//TODO: プロセスデータでメールボックスのライトイベントを読み込んで、メールボックスの到着を確認する。
//      このとき使用するFmmuは３番目となる(3番目のFmmuはMBoxState用みたい)

#![no_std]
pub mod cyclic_task;
mod error;
pub mod frame;
pub mod hal;
pub(crate) mod master;
pub mod register;
pub mod slave_network;
pub use error::*;
pub(crate) mod util;
pub use master::*;
