mod interface;
pub mod tasks;

pub use interface::*;

use self::socket::{CommandData, CommandSocket};
pub mod socket;

///EtherCat system time is expressed in nanoseconds elapsed since January 1, 2000.
#[derive(Debug, Clone, Copy)]
pub struct EtherCatSystemTime(pub u64);

pub trait Cyclic {
    fn process_one_step(&mut self, socket: &mut CommandSocket, sys_time: EtherCatSystemTime) {
        let recv_data = socket.get_recieved_command();
        if let Some(recv_data) = recv_data {
            self.recieve_and_process(&recv_data, sys_time);
        }
        socket.set_command(|buf| self.next_command(buf))
    }

    fn next_command(&mut self, buf: &mut [u8]) -> Option<(Command, usize)>;

    fn recieve_and_process(&mut self, recv_data: &CommandData, sys_time: EtherCatSystemTime);

    fn is_finished(&self) -> bool;
}
