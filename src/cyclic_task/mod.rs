mod interface;
pub mod tasks;
use core::task::Poll;

pub use interface::*;

use self::socket::{CommandData, CommandSocket};
pub mod socket;

///EtherCat system time is expressed in nanoseconds elapsed since January 1, 2000.
#[derive(Debug, Clone, Copy)]
pub struct EtherCatSystemTime(pub u64);

pub trait Cyclic {
    fn process_one_step(&mut self, socket: &mut CommandSocket, sys_time: EtherCatSystemTime) {
        let recv_data = socket.get_recieved_command();
        self.recieve_and_process(recv_data.as_ref(), sys_time);
        socket.set_command(|buf| self.next_command(buf))
    }

    fn next_command(&mut self, buf: &mut [u8]) -> Option<(Command, usize)>;

    fn recieve_and_process(
        &mut self,
        recv_data: Option<&CommandData>,
        sys_time: EtherCatSystemTime,
    );

    fn is_finished(&self) -> bool;
}

// impl CyclicProcess for () {
//     fn next_command<'a, 'b, 'c>(&mut self, _: EtherCatSystemTime) -> Option<(Command, &[u8])> {
//         None
//     }

//     fn recieve_and_process(&mut self, _: Option<ReceivedData>, _: EtherCatSystemTime) {}
// }

// #[derive(Debug, Clone)]
// pub struct ReceivedData<'a> {
//     pub command: Command,
//     pub data: &'a [u8],
//     pub wkc: u16,
// }

// #[derive(Debug, Clone)]
// pub struct TaskHandle(usize);
// impl From<TaskHandle> for usize {
//     fn from(handle: TaskHandle) -> Self {
//         handle.0
//     }
// }

// #[derive(Debug)]
// pub enum TaskOption<C: CyclicProcess> {
//     NextFreeHandle(TaskHandle),
//     Task((C, bool)),
// }

// impl<C: CyclicProcess> Default for TaskOption<C> {
//     fn default() -> Self {
//         Self::NextFreeHandle(TaskHandle(0))
//     }
// }

// impl<C: CyclicProcess> From<C> for TaskOption<C> {
//     fn from(task: C) -> Self {
//         Self::Task((task, false))
//     }
// }

// #[derive(Debug)]
// pub struct CyclicTasks<'packet, 'tasks, D, C, T>
// where
//     D: for<'d> Device<'d>,
//     C: CyclicProcess,
//     T: CountDown,
// {
//     iface: CommandInterface<'packet, D, T>,
//     tasks: &'tasks mut [TaskOption<C>],
//     free_task: TaskHandle,
// }

// impl<'packet, 'tasks, D, C, T> CyclicTasks<'packet, 'tasks, D, C, T>
// where
//     D: for<'d> Device<'d>,
//     C: CyclicProcess,
//     T: CountDown,
// {
//     pub fn new(iface: CommandInterface<'packet, D, T>, tasks: &'tasks mut [TaskOption<C>]) -> Self {
//         tasks
//             .iter_mut()
//             .enumerate()
//             .for_each(|(i, task)| *task = TaskOption::NextFreeHandle(TaskHandle(i + 1)));
//         Self {
//             iface,
//             tasks,
//             free_task: TaskHandle(0),
//         }
//     }

//     pub fn take_resources(self) -> (CommandInterface<'packet, D, T>, &'tasks mut [TaskOption<C>]) {
//         (self.iface, self.tasks)
//     }

//     pub fn add_task(&mut self, task: C) -> Result<TaskHandle, C> {
//         let index = self.free_task.clone();
//         if let Some(task_enum) = self.tasks.get_mut(index.0) {
//             if let TaskOption::NextFreeHandle(next) = task_enum {
//                 self.free_task = next.clone();
//                 *task_enum = TaskOption::Task((task, false));
//                 Ok(index)
//             } else {
//                 unreachable!()
//             }
//         } else {
//             Err(task)
//         }
//     }

//     pub fn remove_task(&mut self, task_handle: TaskHandle) -> Option<C> {
//         if let Some(task_enum) = self.tasks.get_mut(task_handle.0) {
//             match task_enum {
//                 TaskOption::Task(_) => {
//                     let mut next = TaskOption::NextFreeHandle(self.free_task.clone());
//                     self.free_task = task_handle;
//                     core::mem::swap(task_enum, &mut next);
//                     if let TaskOption::Task((task, _)) = next {
//                         Some(task)
//                     } else {
//                         unreachable!()
//                     }
//                 }
//                 TaskOption::NextFreeHandle(_) => None,
//             }
//         } else {
//             None
//         }
//     }

//     pub fn get_task(&mut self, task_handle: &TaskHandle) -> Option<&mut C> {
//         match self.tasks.get_mut(task_handle.0) {
//             Some(TaskOption::Task((ref mut task, _))) => Some(task),
//             _ => None,
//         }
//     }

//     pub fn poll<I: Into<Duration>>(
//         &mut self,
//         sys_time: EtherCatSystemTime,
//         recv_timeout: I,
//     ) -> Result<(), CommandInterfaceError> {
//         let timeout: Duration = recv_timeout.into();
//         loop {
//             let is_all_commands_enqueued = self.enqueue_commands(sys_time)?;

//             self.process(sys_time, timeout)?;

//             if is_all_commands_enqueued {
//                 break;
//             }
//         }
//         Ok(())
//     }

//     fn enqueue_commands(
//         &mut self,
//         sys_time: EtherCatSystemTime,
//     ) -> Result<bool, CommandInterfaceError> {
//         let mut complete = true;
//         for (i, task_enum) in self.tasks.iter_mut().enumerate() {
//             if let TaskOption::Task((task, sent)) = task_enum {
//                 if *sent {
//                     continue;
//                 }
//                 if let Some((command, data)) = task.next_command(sys_time) {
//                     let len = data.len();
//                     if self.iface.remainig_capacity() < len {
//                         complete = false;
//                         break;
//                     }
//                     let _ = self.iface.add_command(i as u8, command, len, |buf| {
//                         for (b, d) in buf.iter_mut().zip(data) {
//                             *b = *d;
//                         }
//                     })?;
//                     *sent = true;
//                 }
//             }
//         }
//         Ok(complete)
//     }

//     fn process<I: Into<Duration> + Clone>(
//         &mut self,
//         sys_time: EtherCatSystemTime,
//         phy_timeout: I,
//     ) -> Result<(), CommandInterfaceError> {
//         let Self { iface, tasks, .. } = self;
//         match iface.poll(phy_timeout) {
//             Ok(_) => {}
//             Err(CommandInterfaceError::RxTimeout) => {} //lost packet
//             Err(err) => return Err(err),
//         }
//         let pdus = iface.consume_commands();
//         let mut last_index = 0;
//         for pdu in pdus {
//             let index = pdu.index() as usize;
//             for j in last_index..index {
//                 if let Some((task, sent)) = get_task_with_sent_flag(tasks, TaskHandle(j)) {
//                     if *sent {
//                         task.recieve_and_process(None, sys_time);

//                         *sent = false;
//                     }
//                 }
//             }
//             if let Some((task, sent)) = get_task_with_sent_flag(tasks, TaskHandle(index)) {
//                 let wkc = pdu.wkc().unwrap_or_default();
//                 let command =
//                     Command::new(CommandType::new(pdu.command_type()), pdu.adp(), pdu.ado());
//                 let recv_data = ReceivedData {
//                     command,
//                     data: pdu.data(),
//                     wkc,
//                 };
//                 assert!(*sent);
//                 task.recieve_and_process(Some(recv_data), sys_time);

//                 *sent = false;
//             }
//             last_index = index + 1;
//         }
//         for j in last_index..tasks.len() {
//             if let Some((task, sent)) = get_task_with_sent_flag(tasks, TaskHandle(j)) {
//                 if *sent {
//                     task.recieve_and_process(None, sys_time);

//                     *sent = false;
//                 }
//             }
//         }
//         Ok(())
//     }
// }

// fn get_task_with_sent_flag<C: CyclicProcess>(
//     tasks: &mut [TaskOption<C>],
//     task_handle: TaskHandle,
// ) -> Option<(&mut C, &mut bool)> {
//     match tasks.get_mut(task_handle.0) {
//         Some(TaskOption::Task((ref mut task, ref mut sent))) => Some((task, sent)),
//         _ => None,
//     }
// }
