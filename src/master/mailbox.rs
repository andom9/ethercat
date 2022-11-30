use crate::frame::{Mailbox, MailboxFrame};
use crate::interface::SlaveAddress;
use crate::interface::{PduSocket, RawEthernetDevice, SocketHandle, SocketInterface};
use crate::register::SyncManagerStatus;
use crate::slave::{Network, Slave};
use crate::task::{CyclicTask, EtherCatSystemTime, MailboxTask, MailboxTaskError, TaskError};

use super::NUM_SOCKETS;

#[derive(Debug)]
pub(super) struct MailboxManager {
    task: MailboxTask,
    slave_with_mailbox: Option<SlaveAddress>,
}

impl MailboxManager {
    pub fn new(task: MailboxTask) -> Self {
        Self {
            task,
            slave_with_mailbox: None,
        }
    }

    pub fn process_one_step<'a>(
        &mut self,
        network: &Network,
        mb_socket: &'a mut PduSocket,
        sys_time: EtherCatSystemTime,
    ) {
        if self.task.is_finished() {
            if let Some(slave_with_mailbox) = self.slave_with_mailbox {
                let (slave, _) = network.slave(slave_with_mailbox).unwrap();
                let tx_sm = slave.info().mailbox_tx_sm().unwrap();
                self.task.start_to_read(slave_with_mailbox, tx_sm, false);
            }
        }
        self.task.process_one_step(mb_socket, sys_time);
    }

    pub fn received_mailbox<'a>(
        &self,
        mb_socket: &'a PduSocket,
    ) -> Option<Result<(MailboxSessionId, MailboxFrame<&'a [u8]>), TaskError<MailboxTaskError>>>
    {
        if self.task.is_write_mode() {
            return None;
        }
        let _ = self.task.wait()?;
        let mb_frame = MailboxFrame(mb_socket.data_buf());
        let session_id = MailboxSessionId {
            slave_address: self.task.slave_address(),
            mailbox_count: mb_frame.count(),
        };
        Some(Ok((session_id, mb_frame)))
    }

    pub fn find_slave_with_mailbox_from_process_data(
        &mut self,
        network: &Network,
        logical_address_offset: u32,
        process_data_image: &[u8],
    ) {
        for (pos, (slave, _)) in network.slaves().enumerate() {
            if let Some(ref fmmu_config) = slave.fmmu_config()[2] {
                let tx_sm_number = slave.info().mailbox_tx_sm().unwrap().number();
                let mb_tx_sm_status = SyncManagerStatus::ADDRESS + 0x08 * tx_sm_number as u16;
                assert_eq!(mb_tx_sm_status, fmmu_config.physical_address());

                let mut buf = [0; SyncManagerStatus::SIZE];
                fmmu_config
                    .read_to_buffer(logical_address_offset, process_data_image, &mut buf)
                    .unwrap();
                let sm_status = SyncManagerStatus(buf);
                if sm_status.is_mailbox_full() {
                    self.slave_with_mailbox = Some(SlaveAddress::SlavePosition(pos as u16));
                }
            }
        }
        self.slave_with_mailbox = None;
    }

    pub fn try_get_mailbox_request_interface<'a>(
        &'a mut self,
    ) -> Option<MailboxRequestInterface<'a>> {
        if !self.task.is_finished() {
            return None;
        }

        let Self { ref mut task, .. } = self;

        Some(MailboxRequestInterface { task })
    }
}

#[derive(Debug)]
pub(super) struct MailboxRequestInterface<'a> {
    task: &'a mut MailboxTask,
}

impl<'a> MailboxRequestInterface<'a> {
    pub fn request(
        &mut self,
        slave: &Slave,
        mailbox: &mut Mailbox,
        mb_buf: &mut [u8],
    ) -> MailboxSessionId {
        let mut mb_frame = MailboxFrame(mb_buf);
        mb_frame.set_mailbox(mailbox).unwrap();
        let rx_sm = slave.info().mailbox_rx_sm().unwrap();
        let session_id = MailboxSessionId {
            slave_address: slave.info().slave_address(),
            mailbox_count: slave.increment_mb_count(),
        };
        mb_frame.set_count(session_id.mailbox_count);
        self.task
            .start_to_write(session_id.slave_address, rx_sm, false);
        session_id
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct MailboxSessionId {
    slave_address: SlaveAddress,
    mailbox_count: u8,
}

#[derive(Debug)]
pub struct MailboxReqIfWrapper<'a, 'b, 'frame, 'socket, D>
where
    D: RawEthernetDevice,
{
    mif: MailboxRequestInterface<'a>,
    sif: &'b mut SocketInterface<'frame, 'socket, D, NUM_SOCKETS>,
    mb_handle: SocketHandle,
}

impl<'a, 'b, 'frame, 'socket, D> MailboxReqIfWrapper<'a, 'b, 'frame, 'socket, D>
where
    D: RawEthernetDevice,
{
    pub(super) fn new(
        mif: MailboxRequestInterface<'a>,
        sif: &'b mut SocketInterface<'frame, 'socket, D, NUM_SOCKETS>,
        mb_handle: SocketHandle,
    ) -> Self {
        Self {
            mif,
            sif,
            mb_handle,
        }
    }

    pub fn write_sdo_request(
        &mut self,
        slave: &Slave,
        index: u16,
        sub_index: u8,
        data: &[u8],
    ) -> MailboxSessionId {
        let mut mailbox = Mailbox::new_sdo_download_request(index, sub_index, data);
        let socket = self.sif.get_socket_mut(&self.mb_handle).unwrap();
        self.mif.request(slave, &mut mailbox, socket.data_buf_mut())
    }

    pub fn read_sdo_request(
        &mut self,
        slave: &Slave,
        index: u16,
        sub_index: u8,
    ) -> MailboxSessionId {
        let mut mailbox = Mailbox::new_sdo_upload_request(index, sub_index);
        let socket = self.sif.get_socket_mut(&self.mb_handle).unwrap();
        self.mif.request(slave, &mut mailbox, socket.data_buf_mut())
    }
}
