use crate::arch::*;
use core::{ops::Range, panic};
use heapless::spsc::*;
use log::*;

struct SocketConsumer<'a, const S: usize> {
    size: Consumer<'a, usize, 32>,
    queue: Consumer<'a, u8, S>,
}

struct SocketProducer<'a, const S: usize> {
    size: Producer<'a, usize, 32>,
    queue: Producer<'a, u8, S>,
}

impl<'a, const S: usize> SocketConsumer<'a, S>{
    fn dequeue_slice(&mut self, buffer: &mut [u8]) -> bool{
        if let Some(size) = self.size.dequeue(){
            for i in 0..size as usize{
                if let Some(buf) = buffer.get_mut(i){
                    if let Some(d) = self.queue.dequeue(){
                        *buf = d;
                    }else{
                        error!("no data in queue");
                        panic!();
                    }
                }else{
                    break;
                }
            }
            true
        }else{
            false
        }
    }
}

impl<'a, const S: usize> SocketProducer<'a, S>{

    fn enqueue_slice(&mut self, data: &[u8]) -> bool{
        let len = data.len();
        if self.queue.capacity() < len{return false}

        for d in data{
            if self.queue.enqueue(*d).is_err(){
                error!("failed to enqueue");
                panic!();
            }
        }
        if self.size.enqueue(len).is_err(){
            error!("failed to enqueue");
            panic!();
        }
        true
    }
}

pub struct Socket<'a, const S: usize> {
    index_range: Range<u8>,
    rx: SocketConsumer<'a, S>,
    tx: SocketProducer<'a, S>,
}

pub struct SocketToken<'a, const S: usize>{
    tx: SocketConsumer<'a, S>,
    rx: SocketProducer<'a, S>,
}

pub struct Dispatcher<'a, R: RawPacketInterface, const S1: usize, const S2: usize> {
    ethdev: R,
    socket1: SocketToken<'a, S1>,
    socket2: SocketToken<'a, S1>,
}

impl<'a, R, const S1: usize, const S2: usize> Dispatcher<'a, R, S1, S2>
where
    R: RawPacketInterface,
{
    pub fn send(&mut self) -> bool {
        let Self { ethdev, .. } = self;

        todo!()
    }

    pub fn recv(&mut self) -> bool {
        todo!()
    }
}
