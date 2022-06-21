//https://infosys.beckhoff.com/english.php?content=../content/1033/tc3_io_intro/1257993099.html

use crate::packet::ethercat::*;

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct EtherCatFrame<B> {
    pub(crate) buffer: B,
    pub free_offset: usize,
    pub index: u8,
}

impl<B: AsRef<[u8]>> EtherCatFrame<B> {
    pub fn new(buffer: B) -> Option<Self> {
        let header_length = EtherCatHeader::SIZE + EthernetHeader::SIZE;

        if buffer.as_ref().len() < header_length {
            return None;
        }
        let ec_packet = EtherCatHeader(&buffer.as_ref()[EthernetHeader::SIZE..]);
        let length = ec_packet.length();
        Some(Self {
            buffer,
            free_offset: header_length + length as usize,
            index: 0,
        })
    }

    pub fn new_unchecked(buffer: B) -> Self {
        let header_length = EtherCatHeader::SIZE + EthernetHeader::SIZE;
        let ec_packet = EtherCatHeader(&buffer.as_ref()[EthernetHeader::SIZE..]);
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
    pub fn iter_dlpdu_offsets(&self) -> EtherCatPduOffsets<&B> {
        EtherCatPduOffsets::new_for_ethercat_frame(&self.buffer, self.buffer.as_ref().len())
    }

    #[inline]
    pub fn iter_dlpdu<'a>(&'a self) -> EtherCatPdus<'a> {
        EtherCatPdus::new_for_ethercat_frame(self.buffer.as_ref(), self.buffer.as_ref().len())
    }
}

impl<B: AsRef<[u8]> + AsMut<[u8]>> EtherCatFrame<B> {
    pub fn init(&mut self) {
        self.buffer.as_mut().iter_mut().for_each(|d| *d = 0);

        {
            let mut ethernet_frame = EthernetHeader(&mut self.buffer);
            ethernet_frame.set_ethercat_default();
        }
        {
            let mut ethercat_frame =
                EtherCatHeader(&mut self.buffer.as_mut()[EthernetHeader::SIZE..]);
            ethercat_frame.set_length(0);
            ethercat_frame.set_ethercat_type(1);
        }
        self.free_offset = EthernetHeader::SIZE + EtherCatHeader::SIZE;
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
        let dlpdu_len = data_len + EtherCatPduHeader::SIZE + WKC_LENGTH;
        if dlpdu_len > self.buffer.as_ref().len() - self.free_offset {
            return false;
        }

        //最後のEtherCatPduを変更
        if let Some(pre_dlpdu_offset) = self.iter_dlpdu_offsets().last() {
            if self.buffer.as_ref()[pre_dlpdu_offset..]
                .get(EtherCatPduHeader::SIZE - 1)
                .is_some()
            {
                EtherCatPduHeader(&mut self.buffer.as_mut()[pre_dlpdu_offset..]).set_has_next(true)
            }
        }

        let mut dlpdu_frame = EtherCatPduHeader(&mut self.buffer.as_mut()[self.free_offset..]);

        dlpdu_frame.set_command_type(command as u8);
        dlpdu_frame.set_adp(adp);
        dlpdu_frame.set_ado(ado);
        dlpdu_frame.set_index(index.unwrap_or(self.index));
        dlpdu_frame.set_is_circulated(false);
        dlpdu_frame.set_has_next(false);
        dlpdu_frame.set_irq(0);
        dlpdu_frame.set_length(data_len as u16);

        for (i, d) in data.iter().enumerate() {
            self.buffer.as_mut()[self.free_offset + EtherCatPduHeader::SIZE + i] = *d;
        }

        //wkcを0にする
        self.buffer.as_mut()[self.free_offset + EtherCatPduHeader::SIZE + data_len] = 0;
        self.buffer.as_mut()[self.free_offset + EtherCatPduHeader::SIZE + data_len + 1] = 0;

        //EtherCatヘッダーのlengthフィールドを更新する。
        let mut ethercat_frame = EtherCatHeader(&mut self.buffer.as_mut()[EthernetHeader::SIZE..]);
        let ec_frame_len = ethercat_frame.length();
        let datagrams_length = ec_frame_len as usize + dlpdu_len;
        ethercat_frame.set_length(datagrams_length as u16);

        self.free_offset += dlpdu_len;
        true
    }
}

#[derive(Debug)]
pub struct EtherCatPduOffsets<B> {
    buffer: B,
    offset: usize,
    length: usize,
}

impl<B: AsRef<[u8]>> EtherCatPduOffsets<B> {
    fn new_for_ethercat_frame(buffer: B, length: usize) -> Self {
        let offset = EtherCatHeader::SIZE + EthernetHeader::SIZE;
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

impl<B: AsRef<[u8]>> Iterator for EtherCatPduOffsets<B> {
    type Item = usize;
    fn next(&mut self) -> Option<Self::Item> {
        self.buffer.as_ref().get(self.offset)?;
        self.buffer.as_ref()[self.offset..].get(EtherCatPduHeader::SIZE - 1)?;
        let dlpdu = EtherCatPduHeader(&self.buffer.as_ref()[self.offset..]);
        let len = dlpdu.length();
        if len == 0 {
            return None;
        }
        if self.offset < self.length {
            let b = self.offset;
            self.offset += EtherCatPduHeader::SIZE + len as usize + WKC_LENGTH;
            Some(b)
        } else {
            None
        }
    }
}

#[derive(Debug)]
pub struct EtherCatPdus<'a> {
    buffer: &'a [u8],
    offset: usize,
    length: usize,
}

impl<'a> EtherCatPdus<'a> {
    fn new_for_ethercat_frame(buffer: &'a [u8], length: usize) -> Self {
        let offset = EtherCatHeader::SIZE + EthernetHeader::SIZE;
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

impl<'a> Iterator for EtherCatPdus<'a> {
    type Item = EtherCatPduHeader<&'a [u8]>;
    fn next(&mut self) -> Option<Self::Item> {
        self.buffer.as_ref().get(self.offset)?;
        self.buffer[self.offset..].get(EtherCatPduHeader::SIZE - 1)?;
        let dlpdu = EtherCatPduHeader(&self.buffer[self.offset..]);
        let len = dlpdu.length();
        if len == 0 {
            return None;
        }
        let start = self.offset;
        if self.offset < self.length {
            self.offset += EtherCatPduHeader::SIZE + len as usize + WKC_LENGTH;
            Some(EtherCatPduHeader(&self.buffer[start..self.offset]))
        } else {
            None
        }
    }
}
