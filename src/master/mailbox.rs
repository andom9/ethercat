use crate::frame::{Mailbox, MailboxFrame, Message};
use crate::interface::{PduSocket, RawEthernetDevice, SocketInterface};
use crate::interface::{SlaveAddress, SocketHandle};
use crate::register::SyncManagerStatus;
use crate::slave::{Network, Slave, SlaveInfo};
use crate::task::{CyclicTask, EtherCatSystemTime, MailboxTask, MailboxTaskError, TaskError};

#[derive(Debug)]
pub struct MailboxManager {
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
    ) -> Option<Result<(MailboxSessionId, MailboxFrame<&'a [u8]>), TaskError<MailboxTaskError>>>
    {
        let mut session_id = MailboxSessionId::default();
        if self.task.is_finished() {
            if let Some(slave_with_mailbox) = self.slave_with_mailbox {
                let (slave, _) = network.slave(slave_with_mailbox).unwrap();
                let tx_sm = slave.info().mailbox_tx_sm().unwrap();
                self.task.start_to_read(slave_with_mailbox, tx_sm, false);
                session_id.slave_address = slave.info().slave_address();
            }
        }
        self.task.process_one_step(mb_socket, sys_time);
        if self.task.is_write_mode() {
            return None;
        }
        let _ = self.task.wait()?;
        let mb_frame = MailboxFrame(mb_socket.data_buf());
        session_id.mailbox_count = mb_frame.count();
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

    pub fn try_get_mailbox_request_interface<'a, 'b, 'c>(
        &'a mut self,
        mb_socket: &'b mut PduSocket<'c>,
    ) -> Option<MailboxRequestInterface<'a, 'b, 'c>> {
        if !self.task.is_finished() {
            return None;
        }

        let Self { ref mut task, .. } = self;

        Some(MailboxRequestInterface {
            task,
            socket: mb_socket,
        })
    }
}

#[derive(Debug)]
pub struct MailboxRequestInterface<'a, 'b, 'c> {
    task: &'a mut MailboxTask,
    socket: &'b mut PduSocket<'c>,
}

impl<'a, 'b, 'c> MailboxRequestInterface<'a, 'b, 'c> {
    pub fn request(&mut self, slave: &Slave, mailbox: &mut Mailbox) -> MailboxSessionId {
        let mut mb_frame = MailboxFrame(self.socket.data_buf_mut());
        mb_frame.set_mailbox(mailbox).unwrap();
        let rx_sm = slave.info().mailbox_rx_sm().unwrap();
        let session_id = MailboxSessionId {
            slave_address: slave.info().slave_address(),
            mailbox_count: slave.increment_mb_count(),
        };
        mailbox.set_mailbox_count(session_id.mailbox_count);
        self.task
            .start_to_write(session_id.slave_address, rx_sm, false);
        session_id
    }

    pub fn write_sdo_request(
        &mut self,
        writer: &mut MailboxRequestInterface,
        slave: &Slave,
        index: u16,
        sub_index: u8,
        data: &[u8],
    ) -> MailboxSessionId {
        let message = Message::new_sdo_download_request(index, sub_index, data);
        let mut mailbox = Mailbox::new(0, 0, message);
        writer.request(slave, &mut mailbox)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct MailboxSessionId {
    slave_address: SlaveAddress,
    mailbox_count: u8,
}
