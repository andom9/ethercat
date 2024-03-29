use super::hal::RawEthernetDevice;
use super::Command;
use super::PduInterface;
use super::PhyError;
use crate::frame::*;
use crate::util::*;

#[derive(Debug, Clone)]
pub struct Pdu<'a> {
    pub command: Command,
    pub data: &'a [u8],
    pub wkc: u16,
}

impl<'a> Pdu<'a> {
    pub fn new(command: Command, data: &'a [u8]) -> Self {
        Self {
            command,
            data,
            wkc: 0,
        }
    }
}

#[derive(Debug)]
pub struct PduSocket<'a> {
    command: Option<Command>,
    pub(crate) data_buf: &'a mut [u8],
    wkc: u16,
    data_length: usize,
    recv_flag: bool,
}

impl<'a> PduSocket<'a> {
    pub fn new(buf: &'a mut [u8]) -> Self {
        Self {
            command: None,
            data_buf: buf,
            wkc: 0,
            data_length: 0,
            recv_flag: false,
        }
    }

    pub fn data_buf<'b>(&'b self) -> &'b [u8] {
        &self.data_buf
    }

    pub fn data_buf_mut<'b>(&'b mut self) -> &'b mut [u8] {
        &mut self.data_buf
    }

    pub fn clear(&mut self) {
        self.data_buf.fill(0);
        self.command = None;
        self.wkc = 0;
        self.data_length = 0;
        self.recv_flag = false;
    }

    pub fn set_pdu<F>(&mut self, command_data: F)
    where
        F: FnOnce(&mut [u8]) -> Option<(Command, usize)>,
    {
        self.recv_flag = false;
        self.wkc = 0;
        if let Some((command, length)) = command_data(self.data_buf) {
            self.command = Some(command);
            self.data_length = length;
        } else {
            self.command = None;
            self.data_length = 0;
        }
    }

    pub fn get_recieved_pdu(&self) -> Option<Pdu> {
        if self.recv_flag {
            self.command.map(|command| Pdu {
                command,
                data: &self.data_buf[..self.data_length],
                wkc: self.wkc,
            })
        } else {
            None
        }
    }

    fn data_length(&self) -> usize {
        if self.command.is_some() {
            self.data_length
        } else {
            0
        }
    }

    fn take_pdu(&mut self) -> Option<Pdu> {
        if self.recv_flag {
            return None;
        }
        let command = core::mem::take(&mut self.command)?;
        Some(Pdu::new(command, &self.data_buf[..self.data_length]))
    }

    fn recieve(&mut self, recv_data: Pdu) {
        self.recv_flag = true;
        self.command = Some(recv_data.command);
        self.data_buf
            .iter_mut()
            .zip(recv_data.data)
            .for_each(|(buf, d)| *buf = *d);
        self.wkc = recv_data.wkc;
    }
}

#[derive(Debug, Clone, Default)]
pub struct SocketHandle(usize);
impl From<SocketHandle> for usize {
    fn from(handle: SocketHandle) -> Self {
        handle.0
    }
}
impl From<usize> for SocketHandle {
    fn from(index: usize) -> Self {
        SocketHandle(index)
    }
}

#[derive(Debug)]
pub struct SocketInterface<'frame, 'buf, D, const N: usize>
where
    D: RawEthernetDevice,
{
    iface: PduInterface<'frame, D>,
    socket_set: IndexSet<SocketHandle, PduSocket<'buf>, N>,
    pub lost_frame_count: usize,
}

impl<'frame, 'buf, D, const N: usize> SocketInterface<'frame, 'buf, D, N>
where
    D: RawEthernetDevice,
{
    pub fn new(iface: PduInterface<'frame, D>) -> Self {
        Self {
            iface,
            socket_set: IndexSet::new(),
            lost_frame_count: 0,
        }
    }

    pub fn add_socket(&mut self, socket: PduSocket<'buf>) -> Result<SocketHandle, PduSocket> {
        self.socket_set.add_item(socket)
    }

    pub fn remove_socket(&mut self, socket_handle: SocketHandle) -> Option<PduSocket> {
        self.socket_set.remove_item(socket_handle)
    }

    pub fn get_socket(&self, socket_handle: &SocketHandle) -> Option<&PduSocket<'buf>> {
        self.socket_set.get_item(socket_handle)
    }

    pub fn get_socket_mut(&mut self, socket_handle: &SocketHandle) -> Option<&mut PduSocket<'buf>> {
        self.socket_set.get_item_mut(socket_handle)
    }

    /// If true, all PDUs are transmitted and received,
    pub fn poll_tx_rx(&mut self) -> Result<bool, PhyError> {
        let is_all_commands_enqueued = self.enqueue_pdus()?;

        let is_all_enqueued_commands_processed = self.transmit_and_receive()?;
        Ok(is_all_commands_enqueued && is_all_enqueued_commands_processed)
    }

    fn enqueue_pdus(&mut self) -> Result<bool, PhyError> {
        let mut complete = true;
        for (i, socket) in self.socket_set.items_mut().enumerate() {
            //if let SocketOption::Socket(socket) = socket_enum {
            let len = socket.data_length();
            if self.iface.remainig_pdu_data_capacity() < len {
                complete = false;
                break;
            }
            if let Some(command_data) = socket.take_pdu() {
                self.iface
                    .add_pdu(i as u8, command_data.command, len, |buf| {
                        for (b, d) in buf.iter_mut().zip(command_data.data) {
                            *b = *d;
                        }
                    })
                    .expect("always success");
            }
            //}
        }
        Ok(complete)
    }

    /// If true, all PDUs are transmitted and received
    fn transmit_and_receive(&mut self) -> Result<bool, PhyError> {
        let Self {
            iface, socket_set, ..
        } = self;
        let is_tx_ok = iface.transmit_one_frame()?;
        let is_rx_ok = iface.receive_one_frame()?;
        if !(is_tx_ok && is_rx_ok) {
            return Ok(false);
        }
        let pdus = iface.consume_pdus();
        for pdu in pdus {
            let index = SocketHandle(pdu.index() as usize);
            if let Some(ref mut socket) = socket_set.get_item_mut(&index) {
                let wkc = pdu.wkc().unwrap_or_default();
                let command =
                    Command::new(CommandType::from(pdu.command_type()), pdu.adp(), pdu.ado());
                let recv_data = Pdu {
                    command,
                    data: &pdu.without_header()[..pdu.length() as usize],
                    wkc,
                };
                socket.recieve(recv_data);
            }
        }
        Ok(true)
    }
}
