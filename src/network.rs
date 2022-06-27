use crate::{interface::SlaveAddress, slave::*};

pub const EMPTY_SLAVE_CONTEXT: Option<Slave> = None;

#[derive(Debug)]
pub struct NetworkDescription<'a, 'b, 'c> {
    slaves: &'a mut [Option<Slave<'b, 'c>>],
    push_count: usize,
}

impl<'a, 'b, 'c> NetworkDescription<'a, 'b, 'c> {
    pub fn new(slave_buf: &'a mut [Option<Slave<'b, 'c>>]) -> Self {
        Self {
            slaves: slave_buf,
            push_count: 0,
        }
    }

    pub(crate) fn clear(&mut self) {
        self.slaves.iter_mut().for_each(|buf| *buf = None);
        self.push_count = 0;
    }

    pub(crate) fn push_slave(&mut self, slave: Slave<'b, 'c>) -> Result<(), Slave<'b, 'c>> {
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

    pub fn slave(&self, addr: SlaveAddress) -> Option<&Slave<'b, 'c>> {
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

    pub fn slave_mut(&mut self, addr: SlaveAddress) -> Option<&mut Slave<'b, 'c>> {
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

    pub fn slaves(&self) -> impl Iterator<Item = &Slave<'b, 'c>> {
        self.slaves.iter().filter_map(|s| s.as_ref())
    }

    pub fn slaves_mut(&mut self) -> impl Iterator<Item = &mut Slave<'b, 'c>> {
        self.slaves.iter_mut().filter_map(|s| s.as_mut())
    }

    pub fn recieved_ports(&'a self) -> RecievedPorts<'a, 'b, 'c> {
        let Self { slaves, .. } = self;
        RecievedPorts::new(slaves)
    }

    pub(crate) fn calculate_pdo_entry_positions_in_pdo_image(&mut self) {
        let mut start_addess = 0;
        //let mut start_bit=0;
        for slave in self.slaves_mut() {
            if slave.pdo_mappings.is_none() {
                continue;
            }
            let pdo_mappings = slave.pdo_mappings.as_mut().unwrap();
            //先にRxPdoを並べる
            for rx_pdo in pdo_mappings.rx_mapping.iter_mut() {
                for pdo in rx_pdo.entries.iter_mut() {
                    pdo.logical_start_address = Some(start_addess);
                    start_addess += pdo.byte_length();
                }
            }

            //RxPdoの後にTxPdoを並べる
            for tx_pdo in pdo_mappings.tx_mapping.iter_mut() {
                for pdo in tx_pdo.entries.iter_mut() {
                    pdo.logical_start_address = Some(start_addess);
                    start_addess += pdo.byte_length();
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct RecievedPorts<'a, 'b, 'c> {
    slaves: &'a [Option<Slave<'b, 'c>>],
    position: u16,
    length: usize,
}

impl<'a, 'b, 'c> RecievedPorts<'a, 'b, 'c> {
    fn new(slaves: &'a [Option<Slave<'b, 'c>>]) -> RecievedPorts<'a, 'b, 'c> {
        let mut length = 0;
        for slave in slaves.iter().filter_map(|s| s.as_ref()) {
            let current_port = slave
                .status
                .linked_ports
                .iter()
                .position(|p| *p)
                .unwrap_or(4);
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

impl<'a, 'b, 'c> Iterator for RecievedPorts<'a, 'b, 'c> {
    type Item = RecievedPort;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let posision_tmp = self.position;
            let slave = self.slaves[self.position as usize].as_ref().unwrap();
            let mut dc = slave.dc_context.borrow_mut();

            let current_port_tmp = dc.current_port;

            let linked_ports = slave.status.linked_ports;
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
