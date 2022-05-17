use crate::slave::*;
use heapless::{FnvIndexMap, Vec};

#[derive(Debug)]
pub struct EtherCATNetwork<const N: usize> {
    slaves: Vec<Slave, N>,
    //pdo_buffer: &'static mut [u8],
}

impl<const N: usize> EtherCATNetwork<N> {
    pub fn new() -> Self {
        Self {
            slaves: Vec::default(),
            //pdo_buffer,
        }
    }

    pub fn clear(&mut self) {
        self.slaves.clear()
    }

    pub fn push_slave(&mut self, slave: Slave) -> Result<(), Slave> {
        self.slaves.push(slave)
    }

    pub fn len(&self) -> usize {
        self.slaves.len()
    }

    pub fn slave(&self, position: u16) -> Option<&Slave> {
        self.slaves.get(position as usize)
    }

    pub fn slave_mut(&mut self, position: u16) -> Option<&mut Slave> {
        self.slaves.get_mut(position as usize)
    }

    pub fn recieved_ports(&self) -> RecievedPorts<N> {
        RecievedPorts::new(&self.slaves)
    }

    pub fn read_write_pdo_buffer(&mut self, pdo_buffer: &mut [u8]) {
        read_write_pdo_buffer(pdo_buffer, &mut self.slaves);
    }
}

struct SlavePortInfo {
    linked_ports: [bool; 4],
    current_port: usize,
}

pub struct RecievedPorts<const N: usize> {
    slaves_port: Vec<SlavePortInfo, N>,
    position: usize,
}

impl<const N: usize> RecievedPorts<N> {
    fn new(slaves: &Vec<Slave, N>) -> Self {
        let mut slaves_port = Vec::default();
        for slave in slaves {
            let current_port = slave.linked_ports.iter().position(|p| *p).unwrap_or(4);
            let _ = slaves_port.push(SlavePortInfo {
                linked_ports: slave.linked_ports,
                current_port,
            });
        }

        Self {
            slaves_port,
            position: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RecievedPort {
    pub position: usize,
    pub port: usize,
}

impl<const N: usize> Iterator for RecievedPorts<N> {
    type Item = RecievedPort;
    fn next(&mut self) -> Option<Self::Item> {
        let posision_tmp = self.position;
        let current_port_tmp = self.slaves_port[self.position].current_port;

        let linked_ports = self.slaves_port[self.position].linked_ports;
        if let Some(next_port) = linked_ports
            .iter()
            .enumerate()
            .position(|(port_num, has_port)| *has_port && port_num > current_port_tmp)
        {
            self.slaves_port[self.position].current_port = next_port;
            self.position += 1;
        } else {
            self.slaves_port[self.position].current_port = 4;
            if let Some((ancestor_position, _ancestor_port)) = self
                .slaves_port
                .iter()
                .enumerate()
                .rev()
                .find(|(_, port)| port.current_port < 4)
            {
                self.position = ancestor_position;
            }
        }

        if posision_tmp < self.slaves_port.len() && current_port_tmp < 4 {
            Some(RecievedPort {
                port: current_port_tmp,
                position: posision_tmp,
            })
        } else {
            None
        }
    }
}

pub(crate) fn read_write_pdo_buffer(pdo_buffer: &mut [u8], slaves: &mut [Slave]) {
    let mut offset = 0;
    let len = slaves.len();
    for i in 0..len {
        let slave = &mut slaves[i];
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
