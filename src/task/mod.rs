mod address_access_task;
mod al_state_transfer;
mod dc_initilize;
mod error;
mod mailbox;
mod mailbox_read;
mod mailbox_write;
mod network_initilize;
mod sii_read;
mod slave_initialize;

pub use address_access_task::AddressAccessTask;
pub use al_state_transfer::{AlStateTransferTask, AlStateTransferTaskError};
pub use dc_initilize::DcInitTask;
pub use error::TaskError;
pub use mailbox::{MailboxTask, MailboxTaskError};
pub use network_initilize::{NetworkInitTask, NetworkInitTaskError};
pub use sii_read::{SiiReader, SiiTaskError};
pub use slave_initialize::*;

pub mod loop_task;
use loop_task::AlStateReadTask;

use crate::{
    frame::{
        AbortCode, EmmergencyErrorCode, EmmergencyFrame, LengthError, Mailbox, MailboxErrorDetail,
        MailboxFrame, Message,
    },
    interface::{
        Command, Pdu, PduSocket, PhyError, RawEthernetDevice, SlaveAddress, SocketHandle,
        SocketInterface, TargetSlave,
    },
    register::{AlStatusCode, SiiData, SyncManagerStatus},
    slave::{AlState, Network, SlaveInfo, Slave},
};

use core::time::Duration;

/// Time elapsed since January 1, 2000 in nanoseconds. 64-bit.
#[derive(Debug, Clone, Copy)]
pub struct EtherCatSystemTime(pub u64);

impl From<Duration> for EtherCatSystemTime {
    fn from(duration: Duration) -> Self {
        EtherCatSystemTime(duration.as_nanos() as u64)
    }
}

pub trait CyclicTask {
    fn process_one_step(&mut self, socket: &mut PduSocket, sys_time: EtherCatSystemTime) {
        let recv_data = socket.get_recieved_pdu();
        if let Some(recv_data) = recv_data {
            self.recieve_and_process(&recv_data, sys_time);
        }
        socket.set_pdu(|buf| self.next_pdu(buf))
    }

    fn next_pdu(&mut self, buf: &mut [u8]) -> Option<(Command, usize)>;

    fn recieve_and_process(&mut self, recv_data: &Pdu, sys_time: EtherCatSystemTime);

    fn is_finished(&self) -> bool;
}

impl<'frame, 'buf, D, const N: usize> SocketInterface<'frame, 'buf, D, N>
where
    D: for<'d> RawEthernetDevice<'d>,
{
    fn block_on<C: CyclicTask, E>(
        &mut self,
        handle: &SocketHandle,
        unit: &mut C,
    ) -> Result<(), TaskError<E>> {
        let mut count = 0;
        loop {
            match self.poll_tx_rx() {
                Ok(is_ok) => {
                    if !is_ok {
                        continue;
                    }
                }
                Err(PhyError::Busy) => continue,
                Err(err) => return Err(err.into()),
            }
            let socket = self.get_socket_mut(handle).unwrap();
            unit.process_one_step(socket, EtherCatSystemTime(count));
            if unit.is_finished() {
                break;
            };
            count += 1;
            if 10000 < count {
                return Err(TaskError::Timeout);
            }
        }
        Ok(())
    }

    pub fn read_register(
        &mut self,
        handle: &SocketHandle,
        target_slave: TargetSlave,
        register_address: u16,
        data_size: usize,
    ) -> Result<&[u8], TaskError<()>> {
        let mut unit = AddressAccessTask::new();

        {
            let socket = self.get_socket_mut(&handle).expect("socket not found");
            assert!(data_size <= socket.data_buf().len());

            unit.start_to_read(target_slave, register_address, data_size);
        }

        self.block_on(handle, &mut unit)?;
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
    ) -> Result<(), TaskError<()>> {
        let mut unit = AddressAccessTask::new();

        let socket = self.get_socket_mut(handle).expect("socket not found");
        assert!(data.len() <= socket.data_buf().len());

        unit.start_to_write(target_slave, register_address, data, socket.data_buf_mut());
        self.block_on(handle, &mut unit)?;
        unit.wait().unwrap()
    }

    pub fn init<'slave, 'pdo_mapping, 'pdo_entry>(
        &mut self,
        handle: &SocketHandle,
        network: &mut Network<'slave, 'pdo_mapping, 'pdo_entry>,
    ) -> Result<(), TaskError<NetworkInitTaskError>> {
        let mut unit = NetworkInitTask::new(network);

        let socket = self.get_socket_mut(handle).expect("socket not found");
        assert!(NetworkInitTask::required_buffer_size() <= socket.data_buf().len());

        unit.start();
        self.block_on::<_, NetworkInitTaskError>(handle, &mut unit)?;
        unit.wait().unwrap()
    }

    pub fn init_dc<'slave, 'pdo_mapping, 'pdo_entry>(
        &mut self,
        handle: &SocketHandle,
        network: &mut Network<'slave, 'pdo_mapping, 'pdo_entry>,
    ) -> Result<(), TaskError<()>> {
        let mut unit = DcInitTask::new(network);

        let socket = self.get_socket_mut(handle).expect("socket not found");
        assert!(DcInitTask::required_buffer_size() <= socket.data_buf().len());

        unit.start();
        self.block_on(handle, &mut unit)?;
        unit.wait().unwrap()
    }

    pub fn read_al_state(
        &mut self,
        handle: &SocketHandle,
        target_slave: TargetSlave,
    ) -> Result<(AlState, Option<AlStatusCode>), TaskError<()>> {
        let mut unit = AlStateReadTask::new();
        {
            let socket = self.get_socket_mut(handle).expect("socket not found");
            assert!(AlStateReadTask::required_buffer_size() <= socket.data_buf().len());
            unit.set_target(target_slave);
        }
        self.block_on::<_, _>(handle, &mut unit)?;
        if unit.invalid_wkc_count != 0 {
            Err(TaskError::UnexpectedWkc(
                (unit.expected_wkc(), unit.last_wkc()).into(),
            ))
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
    ) -> Result<AlState, TaskError<AlStateTransferTaskError>> {
        let mut unit = AlStateTransferTask::new();
        {
            let socket = self.get_socket_mut(handle).expect("socket not found");
            assert!(AlStateTransferTask::required_buffer_size() <= socket.data_buf().len());
            unit.start(target_slave, al_state);
        }
        self.block_on(handle, &mut unit)?;
        unit.wait().unwrap()
    }

    pub fn read_sii(
        &mut self,
        handle: &SocketHandle,
        slave_address: SlaveAddress,
        sii_address: u16,
    ) -> Result<(SiiData<[u8; SiiData::SIZE]>, usize), TaskError<SiiTaskError>> {
        let mut unit = SiiReader::new();
        {
            let socket = self.get_socket_mut(handle).expect("socket not found");
            assert!(SiiReader::required_buffer_size() <= socket.data_buf().len());
            unit.start(slave_address, sii_address);
        }
        self.block_on::<_, _>(handle, &mut unit)?;
        unit.wait().unwrap()
    }

    pub fn read_mailbox(
        &mut self,
        handle: &SocketHandle,
        slave_info: &SlaveInfo,
        wait_full: bool,
    ) -> Result<MailboxFrame<&[u8]>, TaskError<MailboxTaskError>> {
        let mut unit = MailboxTask::new();
        {
            let socket = self.get_socket_mut(handle).expect("socket not found");
            assert!(
                (slave_info.mailbox_tx_sm().unwrap_or_default().size() as usize)
                    <= socket.data_buf().len()
            );
            let slave_address = slave_info.slave_address();
            let tx_sm = slave_info.mailbox_tx_sm().unwrap_or_default();
            unit.start_to_read(slave_address, tx_sm, wait_full);
        }
        self.block_on(handle, &mut unit)?;
        unit.wait().unwrap()?;
        let socket = self.get_socket_mut(handle).expect("socket not found");
        Ok(MailboxFrame(socket.data_buf()))
    }

    pub fn write_mailbox<F: FnOnce(&mut MailboxFrame<&mut [u8]>) -> Result<(), LengthError>>(
        &mut self,
        handle: &SocketHandle,
        slave_info: &SlaveInfo,
        mb_frame_writer: F,
        wait_empty: bool,
    ) -> Result<(), TaskError<MailboxTaskError>> {
        let mut unit = MailboxTask::new();
        {
            let socket = self.get_socket_mut(handle).expect("socket not found");
            assert!(
                (slave_info.mailbox_rx_sm().unwrap_or_default().size() as usize)
                    <= socket.data_buf().len()
            );
            let slave_address = slave_info.slave_address();
            let tx_sm = slave_info.mailbox_tx_sm().unwrap_or_default();
            mb_frame_writer(&mut MailboxFrame(socket.data_buf_mut()))
                .map_err(|_| TaskError::TaskSpecific(MailboxTaskError::BufferSmall))?;
            unit.start_to_write(slave_address, tx_sm, wait_empty);
        }
        self.block_on(handle, &mut unit)?;
        unit.wait().unwrap()
    }

    pub fn write_sdo(
        &mut self,
        handle: &SocketHandle,
        slave: &Slave,
        index: u16,
        sub_index: u8,
        data: &[u8],
    ) -> Result<(), TaskError<SdoErrorKind>> {
        let count = slave.increment_mb_count();
        let slave_info = slave.info();
        
        self.write_mailbox(
            handle,
            slave_info,
            |mb_frame| {
                let message = Message::new_sdo_download_request(index, sub_index, data);
                let mailbox = Mailbox::new(0, count, message);
                mb_frame.set_mailbox(&mailbox)
            },
            false,
        );
        let mb = self.read_mailbox(handle, slave_info, true).unwrap();
        let mb = mb
            .mailbox()
            .map_err(|_| SdoErrorKind::Mailbox(MailboxTaskError::BufferSmall))?;

        match mb.message() {
            Message::Error(err) => Err(SdoErrorKind::ErrorMailbox(err.clone()).into()),
            Message::UnsupportedProtocol(_) => Err(SdoErrorKind::UnexpectedMailbox.into()),
            Message::CoE(coe) => match coe {
                crate::frame::CoE::Emmergency(emm_f) => {
                    Err(SdoErrorKind::Emmergency(emm_f.emmergency_error_code()).into())
                }
                crate::frame::CoE::SdoReq(_) => Err(SdoErrorKind::UnexpectedMailbox.into()),
                crate::frame::CoE::SdoRes(sdo_res) => match sdo_res.res_type() {
                    crate::frame::SdoResType::DownLoad => {
                        if mb.mailbox_count() != count{
                            Err(SdoErrorKind::UnexpectedMailbox.into())
                        }else{
                            Ok(())
                        }
                    },
                    crate::frame::SdoResType::Upload(_) => Err(SdoErrorKind::UnexpectedMailbox.into()),
                    crate::frame::SdoResType::Other(_) => Err(SdoErrorKind::UnexpectedMailbox.into()),
                },
                crate::frame::CoE::UnsupportedType(_) => Err(SdoErrorKind::UnexpectedMailbox.into()),
            },
        }
    }

    pub fn read_sdo(
        &mut self,
        handle: &SocketHandle,
        slave: &Slave,
        index: u16,
        sub_index: u8,
    ) -> Result<&[u8], TaskError<SdoErrorKind>> {
        let count = slave.increment_mb_count();
        let slave_info = slave.info();

        self.write_mailbox(
            handle,
            slave_info,
            |mb_frame| 
            {
                let message = Message::new_sdo_upload_request(index, sub_index);
                let mailbox = Mailbox::new(0, count, message);
                mb_frame.set_mailbox(&mailbox)
            },
            false,
        );
        let mb = self.read_mailbox(handle, slave_info, true).unwrap();
        let mb = mb
            .mailbox()
            .map_err(|_| SdoErrorKind::Mailbox(MailboxTaskError::BufferSmall))?;

        match mb.message() {
            Message::Error(err) => Err(SdoErrorKind::ErrorMailbox(err.clone()).into()),
            Message::UnsupportedProtocol(_) => Err(SdoErrorKind::UnexpectedMailbox.into()),
            Message::CoE(coe) => match coe {
                crate::frame::CoE::Emmergency(emm_f) => {
                    Err(SdoErrorKind::Emmergency(emm_f.emmergency_error_code()).into())
                }
                crate::frame::CoE::SdoReq(_) => Err(SdoErrorKind::UnexpectedMailbox.into()),
                crate::frame::CoE::SdoRes(sdo_res) => match sdo_res.res_type() {
                    crate::frame::SdoResType::DownLoad => Err(SdoErrorKind::UnexpectedMailbox.into()),
                    crate::frame::SdoResType::Upload(res) => {
                        if mb.mailbox_count() != count{
                            Err(SdoErrorKind::UnexpectedMailbox.into())
                        }else{
                            Ok(res)
                        }
                    }
                    crate::frame::SdoResType::Other(_) => Err(SdoErrorKind::UnexpectedMailbox.into()),
                },
                crate::frame::CoE::UnsupportedType(_) => Err(SdoErrorKind::UnexpectedMailbox.into()),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub enum SdoErrorKind {
    Mailbox(MailboxTaskError),
    AbortCode(AbortCode),
    ErrorMailbox(MailboxErrorDetail),
    Emmergency(EmmergencyErrorCode),
    UnexpectedMailbox,
}

impl From<TaskError<()>> for TaskError<SdoErrorKind> {
    fn from(err: TaskError<()>) -> Self {
        match err {
            TaskError::Interface(e) => TaskError::Interface(e),
            TaskError::UnexpectedCommand => TaskError::UnexpectedCommand,
            TaskError::UnexpectedWkc(e) => TaskError::UnexpectedWkc(e),
            TaskError::TaskSpecific(_) => unreachable!(),
            TaskError::Timeout => TaskError::Timeout,
        }
    }
}

impl From<SdoErrorKind> for TaskError<SdoErrorKind> {
    fn from(err: SdoErrorKind) -> Self {
        Self::TaskSpecific(err)
    }
}
