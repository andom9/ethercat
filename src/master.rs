use crate::cyclic_task::socket::{CommandSocket, SocketHandle, SocketOption, SocketsInterface};
use crate::cyclic_task::{tasks::*, *};
use crate::error::EcError;
use crate::hal::*;
use crate::register::SyncManagerActivation;
use crate::register::SyncManagerControl;
use crate::register::{AlStatusCode, FmmuRegister};
use crate::register::{SiiData, SyncManagerStatus};
use crate::slave_network::AlState;
use crate::slave_network::NetworkDescription;
use crate::slave_network::PdoMapping;
use crate::slave_network::Slave;
use crate::slave_network::SlaveInfo;
use core::marker::PhantomPinned;
use core::pin::Pin;
use core::time::Duration;

#[derive(Debug)]
pub struct EtherCatMaster<'packet, 'socket_buf, 'slaves, 'pdo_mapping, 'pdo_entry, D, T>
where
    D: for<'d> Device<'d>,
    T: CountDown,
{
    sif: SocketsInterface<'packet, 'socket_buf, D, T, 3>,
    network: NetworkDescription<'slaves, 'pdo_mapping, 'pdo_entry>,
    socket_handles: [SocketHandle; 3],
}

impl<'packet, 'socket_buf, 'slaves, 'pdo_mapping, 'pdo_entry, D, T>
    EtherCatMaster<'packet, 'socket_buf, 'slaves, 'pdo_mapping, 'pdo_entry, D, T>
where
    D: for<'d> Device<'d>,
    T: CountDown,
{
    pub fn new(
        slave_buf: &'slaves mut [Option<Slave<'pdo_mapping, 'pdo_entry>>],
        pdu_buffer: &'socket_buf mut [u8],
        iface: CommandInterface<'packet, D, T>,
    ) -> Self {
        assert!(
            512 < pdu_buffer.len(),
            "pdu_buffer must be at least 512 bytes."
        );
        let (pdu_buffer1, rest) = pdu_buffer.split_at_mut(64);
        let (pdu_buffer2, rest) = rest.split_at_mut(64);
        let (pdu_buffer3, _) = rest.split_at_mut(256);

        let sockets = [
            SocketOption::default(),
            SocketOption::default(),
            SocketOption::default(),
        ];
        let mut sif = SocketsInterface::new(iface, sockets);
        let socket_handle1 = sif.add_socket(CommandSocket::new(pdu_buffer1)).unwrap();
        let socket_handle2 = sif.add_socket(CommandSocket::new(pdu_buffer2)).unwrap();
        let socket_handle3 = sif.add_socket(CommandSocket::new(pdu_buffer3)).unwrap();

        let network = NetworkDescription::new(slave_buf);
        Self {
            sif,
            network,
            socket_handles: [socket_handle1, socket_handle2, socket_handle3],
        }
    }

    pub fn initilize(&mut self) -> Result<(), EcError<NetworkInitializerError>> {
        let Self { network, sif, .. } = self;
        let mut initializer = NetworkInitializer::new(network);
        initializer.start();

        let mut count = 0;
        loop {
            sif.poll(Duration::from_millis(1000))?;
            let socket1 = sif.get_socket_mut(&self.socket_handles[0]).unwrap();
            initializer.process_one_step(socket1, EtherCatSystemTime(count));
            match initializer.wait() {
                None => {}
                Some(Err(err)) => return Err(err),
                Some(Ok(_)) => break,
            }
            count += 1000;
        }

        Ok(())
    }

    // pub fn configure_pdo_mappings(&mut self) -> Result<(), EcError<SdoTaskError>> {
    //     let Self {
    //         network, sif, ..
    //     } = self;
    //     let sif = sif.as_mut().unwrap();
    //     network.calculate_pdo_entry_positions_in_pdo_image();
    //     let mut logical_address = 0;
    //     for slave in network.slaves() {
    //         //PDOマップが無ければ終わり
    //         if slave.pdo_mappings.is_none() {
    //             continue;
    //         }
    //         let slave_info = slave.info();

    //         ////////
    //         // RX
    //         ////////
    //         let mut rx_pdo_map_size = 0;
    //         if let Some(rx_sm_number) = slave_info.process_data_rx_sm_number() {
    //             let rx_sm_assign = 0x1c10 + rx_sm_number as u16;
    //             let rx_sm_address = SyncManagerControl::ADDRESS + 0x08 * rx_sm_number as u16;

    //             // SMへのPDOマップ割り当てをクリア
    //             cyclic.write_sdo(sdo_task_handle, slave_info, rx_sm_assign, 0, &[0])?;

    //             let pdo_maps = slave.pdo_mappings().unwrap();
    //             let mut map_index = 0;
    //             for rx_pdo_map in pdo_maps.rx_mapping.iter() {
    //                 if rx_pdo_map.entries.is_empty() {
    //                     continue;
    //                 }
    //                 map_index += 1;
    //                 let PdoMapping {
    //                     is_fixed,
    //                     index: pdo_index,
    //                     entries,
    //                 } = rx_pdo_map;
    //                 //SMへPDOマップを割り当て
    //                 cyclic.write_sdo(
    //                     sdo_task_handle,
    //                     slave_info,
    //                     rx_sm_assign,
    //                     map_index,
    //                     &pdo_index.to_le_bytes(),
    //                 )?;
    //                 if *is_fixed {
    //                     continue;
    //                 }
    //                 // PDOマップへのPDOエントリー割り当てをクリア
    //                 cyclic.write_sdo(sdo_task_handle, slave_info, *pdo_index, 0, &[0])?;
    //                 let mut entry_index = 0;
    //                 for entry in entries.iter() {
    //                     let mut data: u32 = 0;
    //                     data |= (entry.index as u32) << 16;
    //                     data |= (entry.sub_index as u32) << 8;
    //                     data |= entry.bit_length as u32;
    //                     entry_index += 1;
    //                     // PDOマップへPDOエントリーを割り当て
    //                     cyclic.write_sdo(
    //                         sdo_task_handle,
    //                         slave_info,
    //                         *pdo_index,
    //                         entry_index,
    //                         &data.to_le_bytes(),
    //                     )?;
    //                     let bit_diff = entry.bit_length() * 8 - entry.bit_length;
    //                     //パディング
    //                     if bit_diff != 0 {
    //                         entry_index += 1;
    //                         cyclic.write_sdo(
    //                             sdo_task_handle,
    //                             slave_info,
    //                             *pdo_index,
    //                             entry_index,
    //                             &(bit_diff as u32).to_le_bytes(),
    //                         )?;
    //                     }
    //                     rx_pdo_map_size += entry.byte_length();
    //                 }
    //                 //PDOマップに何個のエントリーを割り当てたか？
    //                 cyclic.write_sdo(
    //                     sdo_task_handle,
    //                     slave_info,
    //                     *pdo_index,
    //                     0,
    //                     &entry_index.to_le_bytes(),
    //                 )?;
    //             }

    //             //SMに何個のPDOを割り当てたか？
    //             cyclic.write_sdo(
    //                 sdo_task_handle,
    //                 slave_info,
    //                 rx_sm_assign,
    //                 0,
    //                 &map_index.to_be_bytes(),
    //             )?;

    //             //SMの設定
    //             let mut sm_control = SyncManagerControl::new();
    //             sm_control.set_physical_start_address(slave_info.pdo_start_address.unwrap());
    //             sm_control.set_length(rx_pdo_map_size);
    //             sm_control.set_buffer_type(0b00); //buffer mode
    //             sm_control.set_direction(1); //pdi read access
    //             sm_control.set_dls_user_event_enable(true);
    //             cyclic
    //                 .write_register(
    //                     slave_info.slave_address().into(),
    //                     rx_sm_address,
    //                     &sm_control.0,
    //                 )
    //                 .unwrap(); //unwrap for now
    //             let mut sm_active = SyncManagerActivation::new();
    //             sm_active.set_channel_enable(true);
    //             sm_active.set_repeat(false);
    //             cyclic
    //                 .write_register(
    //                     slave_info.slave_address().into(),
    //                     rx_sm_address,
    //                     &sm_active.0,
    //                 )
    //                 .unwrap(); //unwrap for now

    //             // FMMU Setting
    //             let mut fmmu0 = FmmuRegister::new();
    //             fmmu0.set_logical_start_address(logical_address);
    //             logical_address += rx_pdo_map_size as u32;
    //             fmmu0.set_length(rx_pdo_map_size);
    //             fmmu0.set_logical_start_address(0);
    //             fmmu0.set_logical_end_bit(0);
    //             fmmu0.set_physical_start_address(rx_sm_address);
    //             fmmu0.set_physical_start_bit(0);
    //             fmmu0.set_read_enable(true);
    //             fmmu0.set_write_enable(false);
    //             fmmu0.set_enable(true);
    //             cyclic
    //                 .write_register(
    //                     slave_info.slave_address().into(),
    //                     FmmuRegister::ADDRESS,
    //                     &fmmu0.0,
    //                 )
    //                 .unwrap();
    //         }

    //         ////////
    //         // TX
    //         ////////
    //         let mut tx_pdo_map_size = 0;
    //         if let Some(tx_sm_number) = slave_info.process_data_tx_sm_number() {
    //             let tx_sm_assign = 0x1c10 + tx_sm_number as u16;
    //             let tx_sm_address = SyncManagerControl::ADDRESS + 0x08 * tx_sm_number as u16;

    //             //smへのPDOマップの割り当てをクリア
    //             cyclic.write_sdo(sdo_task_handle, slave_info, tx_sm_assign, 0, &[0])?;

    //             let pdo_maps = slave.pdo_mappings().unwrap();
    //             //PDOマップにエントリーを割り当てる
    //             let mut map_index = 0;
    //             for tx_pdo_map in pdo_maps.tx_mapping.iter() {
    //                 if tx_pdo_map.entries.is_empty() {
    //                     continue;
    //                 }
    //                 map_index += 1;
    //                 let PdoMapping {
    //                     is_fixed,
    //                     index: pdo_index,
    //                     entries,
    //                 } = tx_pdo_map;
    //                 // SMにPDOマップを割り当てる
    //                 cyclic.write_sdo(
    //                     sdo_task_handle,
    //                     slave_info,
    //                     tx_sm_assign,
    //                     map_index,
    //                     &pdo_index.to_le_bytes(),
    //                 )?;
    //                 if *is_fixed {
    //                     continue;
    //                 }
    //                 //まずsub_index=0を0でクリアする。
    //                 cyclic.write_sdo(sdo_task_handle, slave_info, *pdo_index, 0, &[0])?;
    //                 let mut entry_index = 0;
    //                 for entry in entries.iter() {
    //                     let mut data: u32 = 0;
    //                     data |= (entry.index as u32) << 16;
    //                     data |= (entry.sub_index as u32) << 8;
    //                     data |= entry.bit_length as u32;
    //                     entry_index += 1;
    //                     cyclic.write_sdo(
    //                         sdo_task_handle,
    //                         slave_info,
    //                         *pdo_index,
    //                         entry_index,
    //                         &data.to_le_bytes(),
    //                     )?;
    //                     let bit_diff = entry.bit_length() * 8 - entry.bit_length;
    //                     //パディング
    //                     if bit_diff != 0 {
    //                         entry_index += 1;
    //                         cyclic.write_sdo(
    //                             sdo_task_handle,
    //                             slave_info,
    //                             *pdo_index,
    //                             entry_index,
    //                             &(bit_diff as u32).to_le_bytes(),
    //                         )?;
    //                     }
    //                     tx_pdo_map_size += entry.byte_length();
    //                 }
    //                 //PDOマップに何個のエントリーを割り当てたか？
    //                 cyclic.write_sdo(
    //                     sdo_task_handle,
    //                     slave_info,
    //                     *pdo_index,
    //                     0,
    //                     &entry_index.to_le_bytes(),
    //                 )?;
    //             }

    //             //SMに何個のPDOを割り当てたか？
    //             cyclic.write_sdo(
    //                 sdo_task_handle,
    //                 slave_info,
    //                 tx_sm_assign,
    //                 0,
    //                 &map_index.to_be_bytes(),
    //             )?;
    //             assert!(rx_pdo_map_size * 3 + tx_pdo_map_size * 3 <= slave_info.pdo_ram_size);
    //             //SMの設定
    //             let mut sm_control = SyncManagerControl::new();
    //             sm_control.set_physical_start_address(
    //                 slave_info.pdo_start_address.unwrap() + rx_pdo_map_size * 3,
    //             );
    //             sm_control.set_length(tx_pdo_map_size);
    //             sm_control.set_buffer_type(0b00); //buffer mode
    //             sm_control.set_direction(0); //pdi write access
    //             sm_control.set_dls_user_event_enable(true);
    //             cyclic
    //                 .write_register(
    //                     slave_info.slave_address().into(),
    //                     tx_sm_address,
    //                     &sm_control.0,
    //                 )
    //                 .unwrap(); //unwrap for now
    //             let mut sm_active = SyncManagerActivation::new();
    //             sm_active.set_channel_enable(true);
    //             sm_active.set_repeat(false);
    //             cyclic
    //                 .write_register(
    //                     slave_info.slave_address().into(),
    //                     tx_sm_address,
    //                     &sm_active.0,
    //                 )
    //                 .unwrap(); //unwrap for now

    //             // FMMU Setting
    //             let mut fmmu1 = FmmuRegister::new();
    //             fmmu1.set_logical_start_address(logical_address);
    //             logical_address += tx_pdo_map_size as u32;
    //             fmmu1.set_length(tx_pdo_map_size);
    //             fmmu1.set_logical_start_address(0);
    //             fmmu1.set_logical_end_bit(0);
    //             fmmu1.set_physical_start_address(tx_sm_address);
    //             fmmu1.set_physical_start_bit(0);
    //             fmmu1.set_read_enable(false);
    //             fmmu1.set_write_enable(true);
    //             fmmu1.set_enable(true);
    //             cyclic
    //                 .write_register(
    //                     slave_info.slave_address().into(),
    //                     FmmuRegister::ADDRESS + FmmuRegister::SIZE as u16,
    //                     &fmmu1.0,
    //                 )
    //                 .unwrap();
    //         }

    //         // FMMU2でメールボックスステータスをポーリングする。
    //         if let Some(tx_sm) = slave_info.mailbox_tx_sm() {
    //             let mb_tx_sm_address = SyncManagerStatus::ADDRESS + 0x08 * tx_sm.number as u16;
    //             let mut fmmu2 = FmmuRegister::new();
    //             fmmu2.set_logical_start_address(logical_address);
    //             logical_address += SyncManagerStatus::SIZE as u32;
    //             fmmu2.set_length(SyncManagerStatus::SIZE as u16);
    //             fmmu2.set_logical_start_address(0);
    //             fmmu2.set_logical_end_bit(0);
    //             fmmu2.set_physical_start_address(mb_tx_sm_address);
    //             fmmu2.set_physical_start_bit(0);
    //             fmmu2.set_read_enable(false);
    //             fmmu2.set_write_enable(true);
    //             fmmu2.set_enable(true);
    //             cyclic
    //                 .write_register(
    //                     slave_info.slave_address().into(),
    //                     FmmuRegister::ADDRESS + 2 * FmmuRegister::SIZE as u16,
    //                     &fmmu2.0,
    //                 )
    //                 .unwrap();
    //         }
    //     }
    //     Ok(())
    // }
}
