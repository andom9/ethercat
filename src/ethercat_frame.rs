//https://infosys.beckhoff.com/english.php?content=../content/1033/tc3_io_intro/1257993099.html

use crate::packet::ethercat::*;
use log::*;

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct EtherCATFrame<B> {
    pub(crate) buffer: B,
    pub free_offset: usize,
    pub index: u8,
}

impl<B: AsRef<[u8]>> EtherCATFrame<B> {
    pub fn new(buffer: B) -> Option<Self> {
        let header_length = ETHERCAT_HEADER_LENGTH + ETHERNET_HEADER_LENGTH;

        if buffer.as_ref().len() < header_length {
            return None;
        }
        let ec_packet = EtherCATHeader::new(&buffer.as_ref()[ETHERNET_HEADER_LENGTH..])?;
        let length = ec_packet.length();
        Some(Self {
            buffer,
            free_offset: header_length + length as usize,
            index: 0,
        })
    }

    pub fn new_unchecked(buffer: B) -> Self {
        let header_length = ETHERCAT_HEADER_LENGTH + ETHERNET_HEADER_LENGTH;
        let ec_packet = EtherCATHeader::new_unchecked(&buffer.as_ref()[ETHERNET_HEADER_LENGTH..]);
        let length = ec_packet.length();
        Self {
            buffer,
            free_offset: header_length + length as usize,
            index: 0,
        }
    }

    #[inline]
    pub fn packet<'a>(&'a self) -> &'a [u8] {
        &self.buffer.as_ref()[..self.free_offset]
    }

    #[inline]
    pub fn iter_dlpdu_offsets(&self) -> EtherCATPDUOffsets<&B> {
        EtherCATPDUOffsets::new_for_ethercat_frame(&self.buffer, self.buffer.as_ref().len())
    }

    #[inline]
    pub fn iter_dlpdu<'a>(&'a self) -> EtherCATPDUs<'a> {
        EtherCATPDUs::new_for_ethercat_frame(self.buffer.as_ref(), self.buffer.as_ref().len())
    }
}

impl<B: AsRef<[u8]> + AsMut<[u8]>> EtherCATFrame<B> {
    pub fn init(&mut self) {
        self.buffer.as_mut().iter_mut().for_each(|d| *d = 0);

        {
            let mut ethernet_frame = EthernetHeader(&mut self.buffer);
            ethernet_frame.set_ethercat_default();
        }
        {
            let mut ethercat_frame =
                EtherCATHeader(&mut self.buffer.as_mut()[ETHERNET_HEADER_LENGTH..]);
            ethercat_frame.set_length(0);
            ethercat_frame.set_ethercat_type(1);
        }
        self.free_offset = ETHERNET_HEADER_LENGTH + ETHERCAT_HEADER_LENGTH;
        self.index = 0;
    }

    #[inline]
    pub fn packet_mut<'a>(&'a mut self) -> &'a mut [u8] {
        &mut self.buffer.as_mut()[..self.free_offset]
    }

    pub fn add_command(
        &mut self,
        command: CommandType,
        adp: u16,
        ado: u16,
        data: &[u8],
        index: Option<u8>,
    ) -> bool {
        let data_len = data.len();
        let dlpdu_len = data_len + ETHERCATPDU_HEADER_LENGTH + WKC_LENGTH;
        if dlpdu_len > self.buffer.as_ref().len() - self.free_offset {
            return false;
        }

        //最後のEtherCATPDUを変更
        if let Some(pre_dlpdu_offset) = self.iter_dlpdu_offsets().last() {
            if let Some(mut pre_dlpdu_frame) =
                EtherCATPDU::new(&mut self.buffer.as_mut()[pre_dlpdu_offset..])
            {
                pre_dlpdu_frame.set_has_next(true);
            }
        }

        let mut dlpdu_frame =
            EtherCATPDU::new(&mut self.buffer.as_mut()[self.free_offset..]).unwrap();

        dlpdu_frame.set_command_type(command as u8);
        dlpdu_frame.set_adp(adp);
        dlpdu_frame.set_ado(ado);
        dlpdu_frame.set_index(index.unwrap_or(self.index));
        dlpdu_frame.set_is_circulated(false);
        dlpdu_frame.set_has_next(false);
        dlpdu_frame.set_irq(0);
        dlpdu_frame.set_length(data_len as u16);

        for (i, d) in data.iter().enumerate() {
            self.buffer.as_mut()[self.free_offset + ETHERCATPDU_HEADER_LENGTH + i] = *d;
        }

        //wkcを0にする
        self.buffer.as_mut()[self.free_offset + ETHERCATPDU_HEADER_LENGTH + data_len] = 0;
        self.buffer.as_mut()[self.free_offset + ETHERCATPDU_HEADER_LENGTH + data_len + 1] = 0;

        //EtherCatヘッダーのlengthフィールドを更新する。
        let mut ethercat_frame =
            EtherCATHeader::new(&mut self.buffer.as_mut()[ETHERNET_HEADER_LENGTH..]).unwrap();
        let ec_frame_len = ethercat_frame.length();
        let datagrams_length = ec_frame_len as usize + dlpdu_len;
        ethercat_frame.set_length(datagrams_length as u16);

        self.free_offset += dlpdu_len;
        true
    }
}

#[inline]
fn divide_address(adr: u32) -> (u16, u16) {
    ((adr & 0x0000_ffff) as u16, (adr >> 16) as u16)
}

#[derive(Debug)]
pub struct EtherCATPDUOffsets<B> {
    buffer: B,
    offset: usize,
    length: usize,
}

impl<B: AsRef<[u8]>> EtherCATPDUOffsets<B> {
    fn new_for_ethercat_frame(buffer: B, length: usize) -> Self {
        let offset = ETHERCAT_HEADER_LENGTH + ETHERNET_HEADER_LENGTH;
        Self::new(buffer, length, offset)
    }

    pub fn new(buffer: B, length: usize, frame_header_size: usize) -> Self {
        Self {
            buffer,
            length,
            offset: frame_header_size,
        }
    }
}

impl<B: AsRef<[u8]>> Iterator for EtherCATPDUOffsets<B> {
    type Item = usize;
    fn next(&mut self) -> Option<Self::Item> {
        self.buffer.as_ref().get(self.offset)?;
        let dlpdu = EtherCATPDU::new(&self.buffer.as_ref()[self.offset..])?;
        let len = dlpdu.length();
        if len == 0 {
            return None;
        }
        if self.offset < self.length {
            let b = self.offset;
            self.offset += ETHERCATPDU_HEADER_LENGTH + len as usize + WKC_LENGTH;
            Some(b)
        } else {
            None
        }
    }
}

#[derive(Debug)]
pub struct EtherCATPDUs<'a> {
    buffer: &'a [u8],
    offset: usize,
    length: usize,
}

impl<'a> EtherCATPDUs<'a> {
    fn new_for_ethercat_frame(buffer: &'a [u8], length: usize) -> Self {
        let offset = ETHERCAT_HEADER_LENGTH + ETHERNET_HEADER_LENGTH;
        Self::new(buffer, length, offset)
    }

    pub fn new(buffer: &'a [u8], length: usize, frame_header_size: usize) -> Self {
        Self {
            buffer,
            length,
            offset: frame_header_size,
        }
    }
}

impl<'a> Iterator for EtherCATPDUs<'a> {
    type Item = EtherCATPDU<&'a [u8]>;
    fn next(&mut self) -> Option<Self::Item> {
        self.buffer.as_ref().get(self.offset)?;
        let dlpdu = EtherCATPDU::new(&self.buffer.as_ref()[self.offset..])?;
        let len = dlpdu.length();
        if len == 0 {
            return None;
        }
        let start = self.offset;
        if self.offset < self.length {
            self.offset += ETHERCATPDU_HEADER_LENGTH + len as usize + WKC_LENGTH;
            Some(EtherCATPDU::new_unchecked(
                &self.buffer.as_ref()[start..self.offset],
            ))
        } else {
            None
        }
    }
}
