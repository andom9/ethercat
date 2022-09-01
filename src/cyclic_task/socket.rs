use super::Command;
use super::CommandInterface;
use super::CommandInterfaceError;
use crate::frame::*;
use crate::hal::*;
use core::task::Poll;
use core::time::Duration;

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
    state: SocketState,
    command: Option<Command>,
    data_buf: &'a mut [u8],
    wkc: u16,
    data_length: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum SocketState {
    Set,
    Send,
    Receive,
    Process,
}

impl<'a> CommandSocket<'a> {
    pub fn new(buf: &'a mut [u8]) -> Self {
        Self {
            state: SocketState::Set,
            command: None,
            data_buf: buf,
            wkc: 0,
            data_length: 0,
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
        self.state = SocketState::Set;
    }

    pub fn is_ready_to_set(&self) -> bool {
        self.state == SocketState::Set
    }

    pub fn set_command<F: FnOnce(&mut [u8]) -> Option<(Command, usize)>>(
        &mut self,
        command_data: F,
    ) {
        if let Some((command, length)) = command_data(self.data_buf) {
            self.command = Some(command);
            self.data_length = length;
            self.wkc = 0;
            self.state = SocketState::Send;
        } else {
            self.command = None;
            self.data_length = 0;
            self.wkc = 0;
            self.state = SocketState::Set;
        }
    }

    pub fn get_recieved_command(&self) -> Poll<Option<CommandData>> {
        if let SocketState::Process = self.state {
            if let Some(command) = self.command {
                Poll::Ready(Some(CommandData {
                    command,
                    data: &self.data_buf[..self.data_length],
                    wkc: self.wkc,
                }))
            } else {
                Poll::Ready(None)
            }
        } else {
            Poll::Pending
        }
    }

    //fn data_length(&self) -> usize {
    //    if self.command.is_some() {
    //        self.data_length
    //    } else {
    //        0
    //    }
    //}

    fn take_command(&mut self) -> Option<CommandData> {
        if let SocketState::Send = self.state {
            let command = self.command?;
            self.state = SocketState::Receive;
            Some(CommandData::new(
                command,
                &self.data_buf[..self.data_length],
            ))
        } else {
            None
        }
    }

    //pub(crate) fn is_waiting_to_recieve(&self) -> bool{
    //    if let SocketState::Receive = self.state{
    //        true
    //    }else{
    //        false
    //    }
    //}

    fn recieve(&mut self, recv_data: Option<CommandData>) {
        if self.state != SocketState::Receive {
            return;
        }
        if let Some(recv_data) = recv_data {
            assert_eq!(recv_data.command, self.command.unwrap());
            self.data_buf
                .iter_mut()
                .zip(recv_data.data)
                .for_each(|(buf, d)| *buf = *d);
            self.wkc = recv_data.wkc;
        } else {
            self.command = None;
        }
        self.state = SocketState::Process;
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
pub struct SocketsInterface<'packet, 'buf, D, T, const N: usize>
where
    D: for<'d> Device<'d>,
    T: CountDown,
{
    iface: CommandInterface<'packet, D, T>,
    sockets: [SocketOption<CommandSocket<'buf>>; N],
    free_index: SocketHandle,
}

impl<'packet, 'buf, D, T, const N: usize> SocketsInterface<'packet, 'buf, D, T, N>
where
    D: for<'d> Device<'d>,
    T: CountDown,
{
    pub fn new(
        iface: CommandInterface<'packet, D, T>,
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
        }
    }

    //pub fn take_resources(self) -> (CommandInterface<'packet, D, T>, &'sockets mut [SocketOption<C>]) {
    //    (self.iface, self.sockets)
    //}

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

    pub fn poll<I: Into<Duration>>(
        &mut self,
        recv_timeout: I,
    ) -> Result<(), CommandInterfaceError> {
        let timeout: Duration = recv_timeout.into();
        loop {
            let is_all_commands_enqueued = self.enqueue_commands()?;

            self.process(timeout)?;

            if is_all_commands_enqueued {
                break;
            }
        }
        Ok(())
    }

    fn enqueue_commands(&mut self) -> Result<bool, CommandInterfaceError> {
        let mut complete = true;
        for (i, socket_enum) in self.sockets.iter_mut().enumerate() {
            if let SocketOption::Socket(socket) = socket_enum {
                if let Some(command_data) = socket.take_command() {
                    let len = command_data.data.len();
                    if self.iface.remainig_capacity() < len {
                        complete = false;
                        break;
                    }
                    let _ = self
                        .iface
                        .add_command(i as u8, command_data.command, len, |buf| {
                            for (b, d) in buf.iter_mut().zip(command_data.data) {
                                *b = *d;
                            }
                        })?;
                }
            }
        }
        Ok(complete)
    }

    fn process<I: Into<Duration> + Clone>(
        &mut self,
        phy_timeout: I,
    ) -> Result<(), CommandInterfaceError> {
        let Self { iface, sockets, .. } = self;
        match iface.poll(phy_timeout) {
            Ok(_) => {}
            Err(CommandInterfaceError::RxTimeout) => {} //lost packet
            Err(err) => return Err(err),
        }
        let pdus = iface.consume_commands();
        let mut last_index = 0;
        for pdu in pdus {
            let index = pdu.index() as usize;
            for j in last_index..index {
                if let Some(SocketOption::Socket(ref mut socket)) = sockets.get_mut(j) {
                    socket.recieve(None);
                }
            }
            if let Some(SocketOption::Socket(ref mut socket)) = sockets.get_mut(index) {
                let wkc = pdu.wkc().unwrap_or_default();
                let command =
                    Command::new(CommandType::new(pdu.command_type()), pdu.adp(), pdu.ado());
                let recv_data = CommandData {
                    command,
                    data: pdu.data(),
                    wkc,
                };
                socket.recieve(Some(recv_data));
            }
            last_index = index + 1;
        }
        for j in last_index..sockets.len() {
            if let Some(SocketOption::Socket(ref mut socket)) = sockets.get_mut(j) {
                socket.recieve(None);
            }
        }
        Ok(())
    }
}
