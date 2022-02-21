//https://infosys.beckhoff.com/english.php?content=../content/1033/tc3_io_intro/1257993099.html

use crate::error::*;
use crate::frame::ethercat::*;
use crate::util::*;

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct EtherCATFrame<B> {
    pub(crate) buffer: B,
    pub free_offset: usize,
    pub index: u8,
}

impl<B: AsRef<[u8]>> EtherCATFrame<B> {
    pub fn new(buffer: B) -> Result<Self, Error> {
        let header_length = ETHERCAT_HEADER_LENGTH + ETHERNET_HEADER_LENGTH;

        if buffer.as_ref().len() < header_length {
            return Err(Error::SmallBuffer);
        }
        let ec_packet = EtherCATHeader::new(&buffer.as_ref()[ETHERNET_HEADER_LENGTH..])
            .ok_or(Error::SmallBuffer)?;
        let length = ec_packet.length();
        Ok(Self {
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
    pub fn drop(self) -> B {
        self.buffer
    }

    #[inline]
    pub fn dlpdu_header_offsets(&self) -> EtherCATPDUOffsets<&B> {
        EtherCATPDUOffsets::new(&self.buffer, self.free_offset, 0)
    }

    #[inline]
    pub fn dlpdu_payload_offsets(&self) -> EtherCATPDUOffsets<&B> {
        EtherCATPDUOffsets::new(&self.buffer, self.free_offset, EtherCATPDU_HEADER_LENGTH)
    }
}

impl<B: AsRef<[u8]> + AsMut<[u8]>> EtherCATFrame<B> {
    pub fn init(&mut self) {
        //self.buffer.as_mut().iter_mut().for_each(|b| *b = 0);
        clear_buffer(self.buffer.as_mut());
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
        //println!("{:?}", self.buffer.as_ref());
        self.free_offset = ETHERNET_HEADER_LENGTH + ETHERCAT_HEADER_LENGTH;
        self.index = 0;
    }

    #[inline]
    pub fn packet_mut<'a>(&'a mut self) -> &'a mut [u8] {
        &mut self.buffer.as_mut()[..self.free_offset]
    }

    fn add_command(
        &mut self,
        command: CommandType,
        adp: u16,
        ado: u16,
        data: &[u8],
    ) -> Result<(), Error> {
        let data_len = data.len();
        if data_len > 1486 {
            //dbg!(data_len);
            return Err(Error::LargeData);
        }
        let dlpdu_len = data_len + EtherCATPDU_HEADER_LENGTH + WKC_LENGTH;
        if dlpdu_len > self.buffer.as_ref().len() - self.free_offset {
            return Err(Error::SmallBuffer);
        }
        {
            //最後のEtherCATPDUを変更
            if let Some(pre_dlpdu_offset) = self.dlpdu_header_offsets().last() {
                //dbg!(pre_dlpdu_offset);
                let mut pre_dlpdu_frame =
                    EtherCATPDU::new(&mut self.buffer.as_mut()[pre_dlpdu_offset..])
                        .ok_or(Error::SmallBuffer)?;
                pre_dlpdu_frame.set_has_next(true);
            }
        }
        {
            let mut dlpdu_frame = EtherCATPDU::new(&mut self.buffer.as_mut()[self.free_offset..])
                .ok_or(Error::SmallBuffer)?;
            dlpdu_frame.set_command_type(command as u8);
            dlpdu_frame.set_adp(adp);
            dlpdu_frame.set_ado(ado);
            dlpdu_frame.set_index(self.index);
            dlpdu_frame.set_is_circulated(false);
            dlpdu_frame.set_has_next(false);
            dlpdu_frame.set_irq(0);
            dlpdu_frame.set_length(data_len as u16);
        }

        for (i, d) in data.iter().enumerate() {
            self.buffer.as_mut()[self.free_offset + EtherCATPDU_HEADER_LENGTH + i] = *d;
        }

        {
            //wkcを0にする
            self.buffer.as_mut()[self.free_offset + EtherCATPDU_HEADER_LENGTH + data_len] = 0;
            self.buffer.as_mut()[self.free_offset + EtherCATPDU_HEADER_LENGTH + data_len + 1] = 0;
        }
        {
            //EtherCatヘッダーのlengthフィールドを更新する。
            let mut ethercat_frame =
                EtherCATHeader::new(&mut self.buffer.as_mut()[ETHERNET_HEADER_LENGTH..])
                    .ok_or(Error::SmallBuffer)?;
            let ec_frame_len = ethercat_frame.length();
            let datagrams_length = ec_frame_len as usize + dlpdu_len;
            if datagrams_length > 1498 {
                return Err(Error::LargeData);
            }
            ethercat_frame.set_length(datagrams_length as u16);
        }
        self.free_offset += dlpdu_len;

        Ok(())
    }

    #[inline]
    pub fn add_aprd(&mut self, adp: u16, ado: u16, data: &[u8]) -> Result<(), Error> {
        self.add_command(CommandType::APRD, adp, ado, data)
    }

    #[inline]
    pub fn add_fprd(&mut self, adp: u16, ado: u16, data: &[u8]) -> Result<(), Error> {
        self.add_command(CommandType::FPRD, adp, ado, data)
    }

    #[inline]
    pub fn add_brd(&mut self, adp: u16, ado: u16, data: &[u8]) -> Result<(), Error> {
        self.add_command(CommandType::BRD, adp, ado, data)
    }

    #[inline]
    pub fn add_lrd(&mut self, adr: u32, data: &[u8]) -> Result<(), Error> {
        let (adp, ado) = divide_address(adr);
        self.add_command(CommandType::LRD, adp, ado, data)
    }

    #[inline]
    pub fn add_apwr(&mut self, adp: u16, ado: u16, data: &[u8]) -> Result<(), Error> {
        self.add_command(CommandType::APWR, adp, ado, data)
    }

    #[inline]
    pub fn add_fpwr(&mut self, adp: u16, ado: u16, data: &[u8]) -> Result<(), Error> {
        self.add_command(CommandType::FPWR, adp, ado, data)
    }

    #[inline]
    pub fn add_bwr(&mut self, adp: u16, ado: u16, data: &[u8]) -> Result<(), Error> {
        self.add_command(CommandType::BWR, adp, ado, data)
    }

    #[inline]
    pub fn add_lwr(&mut self, adr: u32, data: &[u8]) -> Result<(), Error> {
        let (adp, ado) = divide_address(adr);
        self.add_command(CommandType::LWR, adp, ado, data)
    }

    #[inline]
    pub fn add_aprw(&mut self, adp: u16, ado: u16, data: &[u8]) -> Result<(), Error> {
        self.add_command(CommandType::APRW, adp, ado, data)
    }

    #[inline]
    pub fn add_fprw(&mut self, adp: u16, ado: u16, data: &[u8]) -> Result<(), Error> {
        self.add_command(CommandType::FPRW, adp, ado, data)
    }

    #[inline]
    pub fn add_brw(&mut self, adp: u16, ado: u16, data: &[u8]) -> Result<(), Error> {
        self.add_command(CommandType::BRW, adp, ado, data)
    }

    #[inline]
    pub fn add_lrw(&mut self, adr: u32, data: &[u8]) -> Result<(), Error> {
        let (adp, ado) = divide_address(adr);
        self.add_command(CommandType::LRW, adp, ado, data)
    }

    #[inline]
    pub fn add_armw(&mut self, adp: u16, ado: u16, data: &[u8]) -> Result<(), Error> {
        self.add_command(CommandType::ARMW, adp, ado, data)
    }

    #[inline]
    pub fn add_frmw(&mut self, adp: u16, ado: u16, data: &[u8]) -> Result<(), Error> {
        self.add_command(CommandType::FRMW, adp, ado, data)
    }

    #[inline]
    pub fn add_aprd_all_slave(
        &mut self,
        ado: u16,
        data: &[u8],
        num_slaves: u16,
    ) -> Result<(), Error> {
        for i in 0..num_slaves {
            self.add_aprd(get_ap_adp(i), ado, data)?;
        }
        Ok(())
    }

    #[inline]
    pub fn add_apwr_all_slave(
        &mut self,
        ado: u16,
        data: &[u8],
        num_slaves: u16,
    ) -> Result<(), Error> {
        for i in 0..num_slaves {
            self.add_apwr(get_ap_adp(i), ado, data)?;
        }
        Ok(())
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
    delta: usize,
}

impl<B: AsRef<[u8]>> EtherCATPDUOffsets<B> {
    fn new(buffer: B, length: usize, delta: usize) -> Self {
        Self {
            buffer,
            length,
            offset: ETHERCAT_HEADER_LENGTH + ETHERNET_HEADER_LENGTH,
            delta,
        }
    }
}

impl<B: AsRef<[u8]>> Iterator for EtherCATPDUOffsets<B> {
    type Item = usize;
    fn next(&mut self) -> Option<Self::Item> {
        self.buffer.as_ref().get(self.offset)?;
        let dlpdu = EtherCATPDU::new(&self.buffer.as_ref()[self.offset..])?;
        let len = dlpdu.length();
        if self.offset < self.length {
            let b = self.offset;
            self.offset += EtherCATPDU_HEADER_LENGTH + len as usize + WKC_LENGTH;
            Some(b + self.delta)
        } else {
            None
        }
    }
}
