mod al_state_reader;
mod al_state_transfer;
mod dc_drift_compensator;
mod dc_initilizer;
mod mailbox;
mod mailbox_reader;
mod mailbox_writer;
mod network_initilizer;
// mod ram_access_task;
mod sdo;
mod sdo_downloader;
mod sdo_uploader;
mod sii_reader;
mod slave_initializer;

use core::time::Duration;

pub use al_state_reader::*;
pub use al_state_transfer::*;
pub use dc_drift_compensator::*;
pub use dc_initilizer::*;
//pub use mailbox::*;
pub use mailbox_reader::*;
pub use mailbox_writer::*;
pub use network_initilizer::*;
//pub use ram_access_task::*;
pub use sdo::*;
pub use sdo_downloader::*;
pub use sdo_uploader::*;
pub use sii_reader::*;
pub use slave_initializer::*;

use crate::{
    hal::{CountDown, Device},
    slave_network::{NetworkDescription, SlaveInfo},
    EcError,
};

use super::{
    socket::{SocketHandle, SocketsInterface},
    Cyclic, EtherCatSystemTime,
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
            let socket1 = self.get_socket_mut(handle).unwrap();
            unit.process_one_step(socket1, EtherCatSystemTime(count));
            if unit.is_finished() {
                break;
            };
            count += 1000;
        }
        Ok(())
    }

    pub fn initilize_slaves<'slaves, 'pdo_mapping, 'pdo_entry>(
        &mut self,
        handle: &SocketHandle,
        network: &mut NetworkDescription<'slaves, 'pdo_mapping, 'pdo_entry>,
    ) -> Result<(), EcError<NetworkInitializerError>> {
        let mut unit = NetworkInitializer::new(network);

        let socket1 = self.get_socket_mut(handle).expect("socket not found");
        assert!(NetworkInitializer::required_buffer_size() < socket1.capacity());

        unit.start();
        self.block(handle, &mut unit)?;
        unit.wait().unwrap()
    }

    pub fn read_sdo(
        &mut self,
        handle: &SocketHandle,
        slave_info: &SlaveInfo,
        index: u16,
        sub_index: u8,
    ) -> Result<&[u8], EcError<SdoTaskError>> {
        let mut unit = SdoUploader::new();
        {
            let socket1 = self.get_socket_mut(handle).expect("socket not found");
            assert!(NetworkInitializer::required_buffer_size() < socket1.capacity());
            unit.start(slave_info, index, sub_index, socket1.data_buf_mut());
        }
        self.block::<_, SdoTaskError>(handle, &mut unit)?;
        unit.wait().unwrap()?;
        let socket1 = self.get_socket_mut(handle).expect("socket not found");
        Ok(unit.sdo_data(socket1.data_buf()))
    }

    pub fn write_sdo(
        &mut self,
        handle: &SocketHandle,
        slave_info: &SlaveInfo,
        index: u16,
        sub_index: u8,
        data: &[u8],
    ) -> Result<(), EcError<SdoTaskError>> {
        let mut unit = SdoDownloader::new();
        {
            let socket1 = self.get_socket_mut(handle).expect("socket not found");
            assert!(NetworkInitializer::required_buffer_size() < socket1.capacity());
            unit.start(slave_info, index, sub_index, data, socket1.data_buf_mut());
        }
        self.block::<_, SdoTaskError>(handle, &mut unit)?;
        unit.wait().unwrap()
    }
}
