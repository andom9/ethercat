use crate::{interface::SlaveAddress, slave::*};

pub const EMPTY_SLAVE_CONTEXT: Option<Slave> = None;
//const SLAVE_SIZE: usize = core::mem::size_of::<Option<Slave>>();
//pub const EMPTY_SLAVE_PORT_CONTEXT: Option<SlavePort> = None;
//pub const EMPTY_SLAVE_Dc_CONTEXT: Option<SlaveDc> = None;

#[derive(Debug)]
pub struct NetworkDescription<'a> {
    slaves: &'a mut [Option<Slave>],
    push_count: usize,
    //max_push: usize,
}

impl<'a> NetworkDescription<'a> {
    //pub fn new(context_buf: &mut [u8]) -> Self{
    //    let len = context_buf.len();
    //    let max_num_slaves = len/(SLAVE_SIZE+SLAVE_Dc_SIZE);
    //    let (slave_buf, rest) = context_buf.split_at_mut(max_num_slaves*SLAVE_SIZE);
    //    let (dc_buf, _) = rest.split_at_mut(max_num_slaves*SLAVE_Dc_SIZE);
    //
    //    let (slave_buf, dc_buf) = unsafe{
    //        (
    //        core::mem::transmute::<&mut[u8],&mut[Option<Slave>]>(slave_buf),
    //        core::mem::transmute::<&mut[u8],&mut[Option<SlaveDc>]>(dc_buf))
    //    };
    //    slave_buf.iter_mut().for_each(|b|*b=None);
    //    dc_buf.iter_mut().for_each(|b|*b=None);
    //
    //    Self{
    //        slaves: slave_buf,
    //        slave_dcs: dc_buf,
    //        push_count: 0,
    //        max_push: max_num_slaves,
    //    }
    //}

    pub fn new(slave_buf: &'a mut [Option<Slave>]) -> Self {
        //let len1 = slave_buf.iter_mut().map(|buf| *buf = None).count();
        Self {
            slaves: slave_buf,
            push_count: 0,
            //max_push: len1,
        }
    }

    pub(crate) fn clear(&mut self) {
        self.slaves.iter_mut().for_each(|buf| *buf = None);
        self.push_count = 0;
    }

    pub(crate) fn push_slave(&mut self, slave: Slave) -> Result<(), Slave> {
        if self.slaves.len() <= self.push_count {
            Err(slave)
        } else {
            self.slaves[self.push_count] = Some(slave);
            self.push_count += 1;
            Ok(())
        }
    }

    pub fn len(&self) -> usize {
        self.push_count
    }

    pub(crate) fn slave(&self, addr: SlaveAddress) -> Option<&Slave> {
        let addr = match addr {
            SlaveAddress::SlavePosition(n) => n,
            SlaveAddress::StationAddress(n) => {
                if n == 0 {
                    return None;
                } else {
                    n - 1
                }
            }
        };
        if (addr as usize) < self.push_count {
            self.slaves[addr as usize].as_ref()
        } else {
            None
        }
    }

    pub(crate) fn slave_mut(&mut self, addr: SlaveAddress) -> Option<&mut Slave> {
        let addr = match addr {
            SlaveAddress::SlavePosition(n) => n,
            SlaveAddress::StationAddress(n) => {
                if n == 0 {
                    return None;
                } else {
                    n - 1
                }
            }
        };
        if (addr as usize) < self.push_count {
            self.slaves[addr as usize].as_mut()
        } else {
            None
        }
    }

    pub(crate) fn slaves(&self) -> &[Option<Slave>] {
        self.slaves
    }

    pub(crate) fn recieved_ports(&self) -> RecievedPorts {
        let Self { slaves, .. } = self;
        RecievedPorts::new(&slaves)
    }

    pub(crate) fn read_write_pdo_buffer(&mut self, pdo_buffer: &mut [u8]) {
        let iter = self.slaves.iter_mut().filter_map(|s| s.as_mut());
        read_write_pdo_buffer(pdo_buffer, iter);
    }
}

#[derive(Debug)]
pub struct RecievedPorts<'a> {
    slaves: &'a [Option<Slave>],
    position: u16,
    length: usize,
}

impl<'a> RecievedPorts<'a> {
    fn new(slaves: &'a [Option<Slave>]) -> Self {
        let mut length = 0;
        for slave in slaves.iter().filter_map(|s| s.as_ref()) {
            let current_port = slave.linked_ports.iter().position(|p| *p).unwrap_or(4);
            let mut dc = slave.dc_context.borrow_mut();
            dc.current_port = current_port as u8;
            length += 1;
        }

        Self {
            slaves,
            position: 0,
            length,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RecievedPort {
    pub position: u16,
    pub port: u8,
}

impl<'a> Iterator for RecievedPorts<'a> {
    type Item = RecievedPort;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let posision_tmp = self.position;
            let slave = self.slaves[self.position as usize].as_ref().unwrap();
            let mut dc = slave.dc_context.borrow_mut();

            let current_port_tmp = dc.current_port;

            let linked_ports = slave.linked_ports;
            if let Some(next_port) =
                linked_ports
                    .iter()
                    .enumerate()
                    .position(|(port_num, has_port)| {
                        *has_port && (current_port_tmp as usize) < port_num
                    })
            {
                dc.current_port = next_port as u8;
                self.position += 1;
            } else {
                dc.current_port = 4;
                if 1 <= self.position {
                    self.position -= 1;
                } else {
                    break;
                }
            }
            if (posision_tmp as usize) < self.length && current_port_tmp < 4 {
                return Some(RecievedPort {
                    port: current_port_tmp,
                    position: posision_tmp,
                });
            }
        }
        None
    }
}

pub(crate) fn read_write_pdo_buffer<'a, S: IntoIterator<Item = &'a mut Slave>>(
    pdo_buffer: &mut [u8],
    slaves: S,
) {
    let mut offset = 0;
    for slave in slaves {
        //先にRxPDOを並べているとする
        if let Some(ref mut sm_in) = slave.rx_pdo_mapping {
            for pdo_mapping in sm_in.iter_mut() {
                for pdo in pdo_mapping.entries.iter_mut() {
                    let byte_length = pdo.byte_length as usize;
                    pdo.data
                        .copy_from_slice(&pdo_buffer[offset..offset + byte_length]);
                    offset += byte_length;
                }
            }
        }

        //RxPDOの後にTxPDOを並べているとする
        if let Some(ref mut sm_out) = slave.tx_pdo_mapping {
            for pdo_mapping in sm_out.iter_mut() {
                for pdo in pdo_mapping.entries.iter_mut() {
                    let byte_length = pdo.byte_length as usize;
                    pdo_buffer[offset..offset + byte_length].copy_from_slice(&pdo.data);
                    offset += byte_length;
                }
            }
        }
    }
}
