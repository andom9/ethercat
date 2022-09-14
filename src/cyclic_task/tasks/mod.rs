mod al_state_reader;
mod al_state_transfer;
mod dc_drift_comp;
mod dc_initilizer;
mod logical_process_data;
mod mailbox;
mod mailbox_reader;
mod mailbox_writer;
mod network_initilizer;
mod ram_access_task;
mod rx_error_checker;
mod sdo;
mod sdo_downloader;
mod sdo_uploader;
mod sii_reader;
mod slave_initializer;

use core::time::Duration;

pub use al_state_reader::*;
pub use al_state_transfer::*;
pub use dc_drift_comp::*;
pub use dc_initilizer::*;
pub use logical_process_data::*;
pub use mailbox::{MailboxTask, MailboxTaskError};
pub use mailbox_reader::*;
pub use mailbox_writer::*;
pub use network_initilizer::*;
pub use ram_access_task::*;
pub use rx_error_checker::*;
pub use sdo::*;
pub use sdo_downloader::*;
pub use sdo_uploader::*;
pub use sii_reader::*;
pub use slave_initializer::*;

use crate::{
    frame::MailboxHeader,
    hal::{CountDown, Device},
    register::{AlStatusCode, SiiData},
    slave_network::{AlState, NetworkDescription, Slave, SlaveInfo},
    EcError,
};

use super::{
    socket::{SocketHandle, SocketsInterface},
    Cyclic, EtherCatSystemTime, SlaveAddress, TargetSlave,
};

impl<'packet, 'buf, D, T, const N: usize> SocketsInterface<'packet, 'buf, D, T, N>
where
    D: for<'d> Device<'d>,
    T: CountDown,
{
    fn block<C: Cyclic, E>(
        &mut self,
        handle: &SocketHandle,
        unit: &mut C,
    ) -> Result<(), EcError<E>> {
        let mut count = 0;
        loop {
            self.poll(Duration::from_millis(1000))?;
            let socket = self.get_socket_mut(handle).unwrap();
            unit.process_one_step(socket, EtherCatSystemTime(count));
            if unit.is_finished() {
                break;
            };
            count += 1000;
        }
        Ok(())
    }

    pub fn read_register(
        &mut self,
        handle: &SocketHandle,
        target_slave: TargetSlave,
        register_address: u16,
        data_size: usize,
    ) -> Result<&[u8], EcError<()>> {
        let mut unit = RamAccessTask::new();

        {
            let socket = self.get_socket_mut(handle).expect("socket not found");
            assert!(data_size <= socket.capacity());

            unit.start_to_read(target_slave, register_address, data_size);
        }

        self.block(handle, &mut unit)?;
        unit.wait().unwrap()?;
        let socket = self.get_socket_mut(handle).expect("socket not found");
        Ok(&socket.data_buf()[..data_size])
    }

    pub fn write_register(
        &mut self,
        handle: &SocketHandle,
        target_slave: TargetSlave,
        register_address: u16,
        data: &[u8],
    ) -> Result<(), EcError<()>> {
        let mut unit = RamAccessTask::new();

        let socket = self.get_socket_mut(handle).expect("socket not found");
        assert!(data.len() <= socket.capacity());

        unit.start_to_write(target_slave, register_address, data, socket.data_buf_mut());
        self.block(handle, &mut unit)?;
        unit.wait().unwrap()
    }

    pub fn initilize_slaves<'slaves, 'pdo_mapping, 'pdo_entry>(
        &mut self,
        handle: &SocketHandle,
        network: &mut NetworkDescription<'slaves, 'pdo_mapping, 'pdo_entry>,
    ) -> Result<(), EcError<NetworkInitializerError>> {
        let mut unit = NetworkInitializer::new(network);

        let socket = self.get_socket_mut(handle).expect("socket not found");
        assert!(NetworkInitializer::required_buffer_size() <= socket.capacity());

        unit.start();
        self.block(handle, &mut unit)?;
        unit.wait().unwrap()
    }

    pub fn synchronize_dc<'slaves, 'pdo_mapping, 'pdo_entry>(
        &mut self,
        handle: &SocketHandle,
        network: &mut NetworkDescription<'slaves, 'pdo_mapping, 'pdo_entry>,
    ) -> Result<(), EcError<()>> {
        let mut unit = DcInitializer::new(network);

        let socket = self.get_socket_mut(handle).expect("socket not found");
        assert!(DcInitializer::required_buffer_size() <= socket.capacity());

        unit.start();
        self.block(handle, &mut unit)?;
        unit.wait().unwrap()
    }

    pub fn read_al_state(
        &mut self,
        handle: &SocketHandle,
        target_slave: TargetSlave,
    ) -> Result<(AlState, Option<AlStatusCode>), EcError<()>> {
        let mut unit = AlStateReader::new();
        {
            let socket = self.get_socket_mut(handle).expect("socket not found");
            assert!(AlStateReader::required_buffer_size() <= socket.capacity());
            unit.set_target(target_slave);
        }
        self.block::<_, _>(handle, &mut unit)?;
        if unit.invalid_wkc_count != 0 {
            Err(EcError::UnexpectedWkc(unit.last_wkc()))
        } else {
            let (al_state, al_status_code) = unit.last_al_state();
            Ok((al_state.unwrap(), al_status_code))
        }
    }

    pub fn change_al_state(
        &mut self,
        handle: &SocketHandle,
        target_slave: TargetSlave,
        al_state: AlState,
    ) -> Result<AlState, EcError<AlStateTransferError>> {
        let mut unit = AlStateTransfer::new();
        {
            let socket = self.get_socket_mut(handle).expect("socket not found");
            assert!(AlStateTransfer::required_buffer_size() <= socket.capacity());
            unit.start(target_slave, al_state);
        }
        self.block(handle, &mut unit)?;
        unit.wait().unwrap()
    }

    pub fn read_sii(
        &mut self,
        handle: &SocketHandle,
        slave_address: SlaveAddress,
        sii_address: u16,
    ) -> Result<(SiiData<[u8; SiiData::SIZE]>, usize), EcError<SiiTaskError>> {
        let mut unit = SiiReader::new();
        {
            let socket = self.get_socket_mut(handle).expect("socket not found");
            assert!(SiiReader::required_buffer_size() <= socket.capacity());
            unit.start(slave_address, sii_address);
        }
        self.block::<_, _>(handle, &mut unit)?;
        unit.wait().unwrap()
    }

    pub fn read_mailbox(
        &mut self,
        handle: &SocketHandle,
        slave_info: &SlaveInfo,
        wait_full: bool,
    ) -> Result<(MailboxHeader<&[u8]>, &[u8]), EcError<MailboxTaskError>> {
        let mut unit = MailboxReader::new();
        {
            let socket = self.get_socket_mut(handle).expect("socket not found");
            assert!(
                (slave_info.mailbox_tx_sm().unwrap_or_default().size as usize) <= socket.capacity()
            );
            let slave_address = slave_info.slave_address();
            let tx_sm = slave_info.mailbox_tx_sm().unwrap_or_default();
            unit.start(slave_address, tx_sm, wait_full);
        }
        self.block(handle, &mut unit)?;
        unit.wait().unwrap()?;
        let socket = self.get_socket_mut(handle).expect("socket not found");
        Ok(MailboxReader::mailbox_data(socket.data_buf()))
    }

    pub fn write_mailbox(
        &mut self,
        handle: &SocketHandle,
        slave_info: &SlaveInfo,
        mb_header: &[u8; MailboxHeader::SIZE],
        mb_data: &[u8],
        wait_empty: bool,
    ) -> Result<(), EcError<MailboxTaskError>> {
        let mut unit = MailboxWriter::new();
        {
            let socket = self.get_socket_mut(handle).expect("socket not found");
            assert!(
                (slave_info.mailbox_rx_sm().unwrap_or_default().size as usize) <= socket.capacity()
            );
            let slave_address = slave_info.slave_address();
            let tx_sm = slave_info.mailbox_tx_sm().unwrap_or_default();
            MailboxWriter::set_mailbox_data(mb_header, mb_data, socket.data_buf_mut());
            unit.start(slave_address, tx_sm, wait_empty);
        }
        self.block(handle, &mut unit)?;
        unit.wait().unwrap()
    }

    pub fn read_sdo(
        &mut self,
        handle: &SocketHandle,
        slave: &Slave,
        index: u16,
        sub_index: u8,
    ) -> Result<&[u8], EcError<SdoTaskError>> {
        let mut unit = SdoUploader::new();
        {
            let socket = self.get_socket_mut(handle).expect("socket not found");
            assert!(
                (slave.info().mailbox_tx_sm().unwrap_or_default().size as usize)
                    <= socket.capacity()
            );
            unit.start(slave, index, sub_index, socket.data_buf_mut());
        }
        self.block::<_, SdoTaskError>(handle, &mut unit)?;
        unit.wait().unwrap()?;
        let socket = self.get_socket_mut(handle).expect("socket not found");
        Ok(unit.sdo_data(socket.data_buf()))
    }

    pub fn write_sdo(
        &mut self,
        handle: &SocketHandle,
        slave: &Slave,
        index: u16,
        sub_index: u8,
        data: &[u8],
    ) -> Result<(), EcError<SdoTaskError>> {
        let mut unit = SdoDownloader::new();
        {
            let socket = self.get_socket_mut(handle).expect("socket not found");
            assert!(
                (slave.info().mailbox_rx_sm().unwrap_or_default().size as usize)
                    <= socket.capacity()
            );
            unit.start(slave, index, sub_index, data, socket.data_buf_mut());
        }
        self.block::<_, SdoTaskError>(handle, &mut unit)?;
        unit.wait().unwrap()
    }
}
