use super::Command;
use super::CommandInterface;
use super::CommandInterfaceError;
use crate::frame::*;
use crate::hal::*;

#[derive(Debug, Clone)]
pub struct CommandData<'a> {
    pub command: Command,
    pub data: &'a [u8],
    pub wkc: u16,
}

impl<'a> CommandData<'a> {
    pub fn new(command: Command, data: &'a [u8]) -> Self {
        Self {
            command,
            data,
            wkc: 0,
        }
    }
}

#[derive(Debug)]
pub struct CommandSocket<'a> {
    command: Option<Command>,
    data_buf: &'a mut [u8],
    wkc: u16,
    data_length: usize,
    recv_flag: bool,
}

impl<'a> CommandSocket<'a> {
    pub fn new(buf: &'a mut [u8]) -> Self {
        Self {
            command: None,
            data_buf: buf,
            wkc: 0,
            data_length: 0,
            recv_flag: false,
        }
    }

    pub fn data_buf(&self) -> &[u8] {
        self.data_buf
    }

    pub fn data_buf_mut(&mut self) -> &mut [u8] {
        self.data_buf
    }

    pub fn capacity(&self) -> usize {
        self.data_buf.len()
    }

    pub fn clear(&mut self) {
        self.data_buf.fill(0);
        self.command = None;
        self.wkc = 0;
        self.data_length = 0;
        self.recv_flag = false;
    }

    pub fn set_command<F: FnOnce(&mut [u8]) -> Option<(Command, usize)>>(
        &mut self,
        command_data: F,
    ) {
        self.recv_flag = false;
        self.wkc = 0;
        log::info!("set_command");
        if let Some((command, length)) = command_data(self.data_buf) {
            self.command = Some(command);
            self.data_length = length;
        } else {
            log::info!("but none");
            self.command = None;
            self.data_length = 0;
        }
    }

    pub fn get_recieved_command(&self) -> Option<CommandData> {
        if self.recv_flag {
            if let Some(command) = self.command {
                Some(CommandData {
                    command,
                    data: &self.data_buf[..self.data_length],
                    wkc: self.wkc,
                })
            } else {
                None
            }
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

    fn take_command(&mut self) -> Option<CommandData> {
        let command = core::mem::take(&mut self.command)?;
        Some(CommandData::new(
            command,
            &self.data_buf[..self.data_length],
        ))
    }

    fn recieve(&mut self, recv_data: CommandData) {
        self.recv_flag = true;
        self.command = Some(recv_data.command);
        self.data_buf
            .iter_mut()
            .zip(recv_data.data)
            .for_each(|(buf, d)| *buf = *d);
        self.wkc = recv_data.wkc;
    }
}

#[derive(Debug, Clone)]
pub struct SocketHandle(usize);
impl From<SocketHandle> for usize {
    fn from(handle: SocketHandle) -> Self {
        handle.0
    }
}

#[derive(Debug)]
pub enum SocketOption<S> {
    NextFreeIndex(SocketHandle),
    Socket(S),
}

impl<S> Default for SocketOption<S> {
    fn default() -> Self {
        Self::NextFreeIndex(SocketHandle(0))
    }
}

#[derive(Debug)]
pub struct SocketsInterface<'packet, 'buf, D, const N: usize>
where
    D: for<'d> RawEthernetDevice<'d>,
{
    iface: CommandInterface<'packet, D>,
    sockets: [SocketOption<CommandSocket<'buf>>; N],
    free_index: SocketHandle,
    pub lost_frame_count: usize,
}

impl<'packet, 'buf, D, const N: usize> SocketsInterface<'packet, 'buf, D, N>
where
    D: for<'d> RawEthernetDevice<'d>,
{
    pub fn new(
        iface: CommandInterface<'packet, D>,
        mut sockets: [SocketOption<CommandSocket<'buf>>; N],
    ) -> Self {
        sockets
            .iter_mut()
            .enumerate()
            .for_each(|(i, socket)| *socket = SocketOption::NextFreeIndex(SocketHandle(i + 1)));
        Self {
            iface,
            sockets,
            free_index: SocketHandle(0),
            lost_frame_count: 0,
        }
    }

    pub fn add_socket(
        &mut self,
        socket: CommandSocket<'buf>,
    ) -> Result<SocketHandle, CommandSocket> {
        let index = self.free_index.clone();
        if let Some(socket_enum) = self.sockets.get_mut(index.0) {
            if let SocketOption::NextFreeIndex(next) = socket_enum {
                self.free_index = next.clone();
                *socket_enum = SocketOption::Socket(socket);
                Ok(index)
            } else {
                unreachable!()
            }
        } else {
            Err(socket)
        }
    }

    pub fn remove_socket(&mut self, socket_handle: SocketHandle) -> Option<CommandSocket> {
        if let Some(socket_enum) = self.sockets.get_mut(socket_handle.0) {
            match socket_enum {
                SocketOption::Socket(_) => {
                    let mut next = SocketOption::NextFreeIndex(self.free_index.clone());
                    self.free_index = socket_handle;
                    core::mem::swap(socket_enum, &mut next);
                    if let SocketOption::Socket(socket) = next {
                        Some(socket)
                    } else {
                        unreachable!()
                    }
                }
                SocketOption::NextFreeIndex(_) => None,
            }
        } else {
            None
        }
    }

    pub fn get_socket_mut(
        &mut self,
        socket_handle: &SocketHandle,
    ) -> Option<&mut CommandSocket<'buf>> {
        match self.sockets.get_mut(socket_handle.0) {
            Some(SocketOption::Socket(ref mut socket)) => Some(socket),
            _ => None,
        }
    }

    /// If true, all PDUs are transmitted and received,
    pub fn poll_tx_rx(&mut self) -> Result<bool, CommandInterfaceError> {
        let is_all_commands_enqueued = self.enqueue_commands()?;

        let is_all_enqueued_commands_processed = self.transmit_and_receive()?;
        Ok(is_all_commands_enqueued && is_all_enqueued_commands_processed)
    }

    fn enqueue_commands(&mut self) -> Result<bool, CommandInterfaceError> {
        let mut complete = true;
        for (i, socket_enum) in self.sockets.iter_mut().enumerate() {
            if let SocketOption::Socket(socket) = socket_enum {
                let len = socket.data_length();
                if self.iface.remainig_pdu_data_capacity() < len {
                    complete = false;
                    break;
                }
                if let Some(command_data) = socket.take_command() {
                    log::info!("send index{}", i);
                    let _ = self
                        .iface
                        .add_command(i as u8, command_data.command, len, |buf| {
                            for (b, d) in buf.iter_mut().zip(command_data.data) {
                                *b = *d;
                            }
                        })
                        .expect("always success");
                }
            }
        }
        Ok(complete)
    }

    /// If true, all PDUs are transmitted and received
    fn transmit_and_receive(&mut self) -> Result<bool, CommandInterfaceError> {
        let Self { iface, sockets, .. } = self;
        let is_tx_ok = iface.transmit_one_frame()?;
        let is_rx_ok = iface.receive_one_frame()?;
        let pdus = iface.consume_commands();
        for pdu in pdus {
            let index = pdu.index() as usize;
            log::info!("recv index{}", index);
            if let Some(SocketOption::Socket(ref mut socket)) = sockets.get_mut(index) {
                let wkc = pdu.wkc().unwrap_or_default();
                let command =
                    Command::new(CommandType::new(pdu.command_type()), pdu.adp(), pdu.ado());
                let recv_data = CommandData {
                    command,
                    data: pdu.data(),
                    wkc,
                };
                socket.recieve(recv_data);
                log::info!("socket recv");
            }
        }
        Ok(is_rx_ok && is_tx_ok)
    }
}
