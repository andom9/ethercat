use super::*;
use crate::interface::*;

//pub const EMPTY_SLAVE: Option<Slave> = None;

#[derive(Debug)]
pub struct NetworkDescription<'a, 'b, 'c> {
    slaves: &'a mut [(Option<Slave>, SlaveConfig<'b, 'c>)],
    push_count: u16,
}

impl<'a, 'b, 'c> NetworkDescription<'a, 'b, 'c> {
    pub fn new(slave_buf: &'a mut [(Option<Slave>, SlaveConfig<'b, 'c>)]) -> Self {
        Self {
            slaves: slave_buf,
            push_count: 0,
        }
    }

    pub(crate) fn clear(&mut self) {
        self.slaves.iter_mut().for_each(|buf| buf.0 = None);
        self.push_count = 0;
    }

    pub(crate) fn push_slave(&mut self, slave: Slave) -> Result<(), Slave> {
        if self.slaves.len() <= self.push_count as usize {
            Err(slave)
        } else {
            self.slaves[self.push_count as usize].0 = Some(slave);
            self.push_count += 1;
            Ok(())
        }
    }

    pub fn num_slaves(&self) -> u16 {
        self.push_count
    }

    pub fn slave(&self, addr: SlaveAddress) -> Option<(&Slave, &SlaveConfig<'b, 'c>)> {
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
        if (addr) < self.push_count {
            let slave_with_config = &self.slaves[addr as usize];
            Some((slave_with_config.0.as_ref().unwrap(), &slave_with_config.1))
        } else {
            None
        }
    }

    pub fn slave_mut(
        &mut self,
        addr: SlaveAddress,
    ) -> Option<(&mut Slave, &mut SlaveConfig<'b, 'c>)> {
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
        if (addr) < self.push_count {
            let slave_with_config = &mut self.slaves[addr as usize];
            Some((
                slave_with_config.0.as_mut().unwrap(),
                &mut slave_with_config.1,
            ))
        } else {
            None
        }
    }

    pub fn slaves(&self) -> impl Iterator<Item = (&Slave, &SlaveConfig<'b, 'c>)> {
        self.slaves
            .iter()
            .filter(|s| s.0.is_some())
            .map(|(s, c)| (s.as_ref().unwrap(), c))
    }

    pub fn slaves_mut(&mut self) -> impl Iterator<Item = (&mut Slave, &mut SlaveConfig<'b, 'c>)> {
        self.slaves
            .iter_mut()
            .filter(|s| s.0.is_some())
            .map(|(s, ref mut c)| (s.as_mut().unwrap(), c))
    }

    pub fn recieved_ports(&'a self) -> RecievedPorts<'a, 'b, 'c> {
        let Self { slaves, .. } = self;
        RecievedPorts::new(slaves)
    }
}

#[derive(Debug)]
pub struct RecievedPorts<'a, 'b, 'c> {
    slaves: &'a [(Option<Slave>, SlaveConfig<'b, 'c>)],
    position: u16,
    length: usize,
}

impl<'a, 'b, 'c> RecievedPorts<'a, 'b, 'c> {
    fn new(slaves: &'a [(Option<Slave>, SlaveConfig<'b, 'c>)]) -> RecievedPorts<'a, 'b, 'c> {
        let mut length = 0;
        for slave in slaves.iter().filter_map(|s| s.0.as_ref()) {
            let current_port = slave.info.linked_ports.iter().position(|p| *p).unwrap_or(4);
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
            let slave = self.slaves[self.position as usize].0.as_ref().unwrap();
            let mut dc = slave.dc_context.borrow_mut();

            let current_port_tmp = dc.current_port;

            let linked_ports = slave.info.linked_ports;
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
