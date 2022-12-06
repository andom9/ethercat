use crate::{
    interface::{RawEthernetDevice, SlaveAddress},
    task::{SdoErrorKind, TaskError},
    EtherCatMaster,
};

impl<'frame, 'socket, 'slave, 'pdo_mapping, 'pdo_entry, D>
    EtherCatMaster<'frame, 'socket, 'slave, 'pdo_mapping, 'pdo_entry, D>
where
    D: RawEthernetDevice,
{
    pub fn read_sdo_as_bool(
        &mut self,
        slave_address: SlaveAddress,
        index: u16,
        sub_index: u8,
    ) -> Result<bool, TaskError<SdoErrorKind>> {
        let buf = self.read_sdo(slave_address, index, sub_index)?;
        Ok(buf[0] & 1 == 1)
    }

    pub fn read_sdo_as_u8(
        &mut self,
        slave_address: SlaveAddress,
        index: u16,
        sub_index: u8,
    ) -> Result<u8, TaskError<SdoErrorKind>> {
        let buf = self.read_sdo(slave_address, index, sub_index)?;
        Ok(u8::from_le_bytes([buf[0]]))
    }

    pub fn read_sdo_as_i8(
        &mut self,
        slave_address: SlaveAddress,
        index: u16,
        sub_index: u8,
    ) -> Result<i8, TaskError<SdoErrorKind>> {
        let buf = self.read_sdo(slave_address, index, sub_index)?;
        Ok(i8::from_le_bytes([buf[0]]))
    }

    pub fn read_sdo_as_u16(
        &mut self,
        slave_address: SlaveAddress,
        index: u16,
        sub_index: u8,
    ) -> Result<u16, TaskError<SdoErrorKind>> {
        let buf = self.read_sdo(slave_address, index, sub_index)?;
        Ok(u16::from_le_bytes([buf[0], buf[1]]))
    }

    pub fn read_sdo_as_i16(
        &mut self,
        slave_address: SlaveAddress,
        index: u16,
        sub_index: u8,
    ) -> Result<i16, TaskError<SdoErrorKind>> {
        let buf = self.read_sdo(slave_address, index, sub_index)?;
        Ok(i16::from_le_bytes([buf[0], buf[1]]))
    }

    pub fn read_sdo_as_u32(
        &mut self,
        slave_address: SlaveAddress,
        index: u16,
        sub_index: u8,
    ) -> Result<u32, TaskError<SdoErrorKind>> {
        let buf = self.read_sdo(slave_address, index, sub_index)?;
        Ok(u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]))
    }

    pub fn read_sdo_as_i32(
        &mut self,
        slave_address: SlaveAddress,
        index: u16,
        sub_index: u8,
    ) -> Result<i32, TaskError<SdoErrorKind>> {
        let buf = self.read_sdo(slave_address, index, sub_index)?;
        Ok(i32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]))
    }

    pub fn read_sdo_as_u64(
        &mut self,
        slave_address: SlaveAddress,
        index: u16,
        sub_index: u8,
    ) -> Result<u64, TaskError<SdoErrorKind>> {
        let buf = self.read_sdo(slave_address, index, sub_index)?;
        Ok(u64::from_le_bytes([
            buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7],
        ]))
    }

    pub fn read_sdo_as_i64(
        &mut self,
        slave_address: SlaveAddress,
        index: u16,
        sub_index: u8,
    ) -> Result<i64, TaskError<SdoErrorKind>> {
        let buf = self.read_sdo(slave_address, index, sub_index)?;
        Ok(i64::from_le_bytes([
            buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7],
        ]))
    }

    pub fn write_sdo_as_bool(
        &mut self,
        slave_address: SlaveAddress,
        index: u16,
        sub_index: u8,
        data: bool,
    ) -> Result<(), TaskError<SdoErrorKind>> {
        let buf = [data as u8];
        let (slave, _) = self.network.slave(slave_address).expect("slave not found");
        self.sif
            .write_sdo(&self.gp_socket_handle, slave, index, sub_index, &buf)
    }

    pub fn write_sdo_as_u8(
        &mut self,
        slave_address: SlaveAddress,
        index: u16,
        sub_index: u8,
        data: u8,
    ) -> Result<(), TaskError<SdoErrorKind>> {
        let buf = data.to_le_bytes();
        let (slave, _) = self.network.slave(slave_address).expect("slave not found");
        self.sif
            .write_sdo(&self.gp_socket_handle, slave, index, sub_index, &buf)
    }

    pub fn write_sdo_as_i8(
        &mut self,
        slave_address: SlaveAddress,
        index: u16,
        sub_index: u8,
        data: i8,
    ) -> Result<(), TaskError<SdoErrorKind>> {
        let buf = data.to_le_bytes();
        let (slave, _) = self.network.slave(slave_address).expect("slave not found");
        self.sif
            .write_sdo(&self.gp_socket_handle, slave, index, sub_index, &buf)
    }

    pub fn write_sdo_as_u16(
        &mut self,
        slave_address: SlaveAddress,
        index: u16,
        sub_index: u8,
        data: u16,
    ) -> Result<(), TaskError<SdoErrorKind>> {
        let buf = data.to_le_bytes();
        let (slave, _) = self.network.slave(slave_address).expect("slave not found");
        self.sif
            .write_sdo(&self.gp_socket_handle, slave, index, sub_index, &buf)
    }

    pub fn write_sdo_as_i16(
        &mut self,
        slave_address: SlaveAddress,
        index: u16,
        sub_index: u8,
        data: i16,
    ) -> Result<(), TaskError<SdoErrorKind>> {
        let buf = data.to_le_bytes();
        let (slave, _) = self.network.slave(slave_address).expect("slave not found");
        self.sif
            .write_sdo(&self.gp_socket_handle, slave, index, sub_index, &buf)
    }

    pub fn write_sdo_as_u32(
        &mut self,
        slave_address: SlaveAddress,
        index: u16,
        sub_index: u8,
        data: u32,
    ) -> Result<(), TaskError<SdoErrorKind>> {
        let buf = data.to_le_bytes();
        let (slave, _) = self.network.slave(slave_address).expect("slave not found");
        self.sif
            .write_sdo(&self.gp_socket_handle, slave, index, sub_index, &buf)
    }

    pub fn write_sdo_as_i32(
        &mut self,
        slave_address: SlaveAddress,
        index: u16,
        sub_index: u8,
        data: i32,
    ) -> Result<(), TaskError<SdoErrorKind>> {
        let buf = data.to_le_bytes();
        let (slave, _) = self.network.slave(slave_address).expect("slave not found");
        self.sif
            .write_sdo(&self.gp_socket_handle, slave, index, sub_index, &buf)
    }

    pub fn write_sdo_as_u64(
        &mut self,
        slave_address: SlaveAddress,
        index: u16,
        sub_index: u8,
        data: u64,
    ) -> Result<(), TaskError<SdoErrorKind>> {
        let buf = data.to_le_bytes();
        let (slave, _) = self.network.slave(slave_address).expect("slave not found");
        self.sif
            .write_sdo(&self.gp_socket_handle, slave, index, sub_index, &buf)
    }

    pub fn write_sdo_as_i64(
        &mut self,
        slave_address: SlaveAddress,
        index: u16,
        sub_index: u8,
        data: i64,
    ) -> Result<(), TaskError<SdoErrorKind>> {
        let buf = data.to_le_bytes();
        let (slave, _) = self.network.slave(slave_address).expect("slave not found");
        self.sif
            .write_sdo(&self.gp_socket_handle, slave, index, sub_index, &buf)
    }

    pub fn read_pdo_as_bool(
        &self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
    ) -> Option<bool> {
        let mut buf = [0];
        self.read_pdo(slave_address, pdo_map_index, pdo_entry_index, &mut buf)?;
        Some(buf[0] & 1 == 1)
    }

    pub fn read_pdo_as_u8(
        &self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
    ) -> Option<u8> {
        let mut buf = [0];
        self.read_pdo(slave_address, pdo_map_index, pdo_entry_index, &mut buf)?;
        Some(u8::from_le_bytes(buf))
    }

    pub fn read_pdo_as_i8(
        &self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
    ) -> Option<i8> {
        let mut buf = [0];
        self.read_pdo(slave_address, pdo_map_index, pdo_entry_index, &mut buf)?;
        Some(i8::from_le_bytes(buf))
    }

    pub fn read_pdo_as_u16(
        &self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
    ) -> Option<u16> {
        let mut buf = [0; 2];
        self.read_pdo(slave_address, pdo_map_index, pdo_entry_index, &mut buf)?;
        Some(u16::from_le_bytes(buf))
    }

    pub fn read_pdo_as_i16(
        &self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
    ) -> Option<i16> {
        let mut buf = [0; 2];
        self.read_pdo(slave_address, pdo_map_index, pdo_entry_index, &mut buf)?;
        Some(i16::from_le_bytes(buf))
    }

    pub fn read_pdo_as_u32(
        &self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
    ) -> Option<u32> {
        let mut buf = [0; 4];
        self.read_pdo(slave_address, pdo_map_index, pdo_entry_index, &mut buf)?;
        Some(u32::from_le_bytes(buf))
    }

    pub fn read_pdo_as_i32(
        &self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
    ) -> Option<i32> {
        let mut buf = [0; 4];
        self.read_pdo(slave_address, pdo_map_index, pdo_entry_index, &mut buf)?;
        Some(i32::from_le_bytes(buf))
    }

    pub fn read_pdo_as_u64(
        &self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
    ) -> Option<u64> {
        let mut buf = [0; 8];
        self.read_pdo(slave_address, pdo_map_index, pdo_entry_index, &mut buf)?;
        Some(u64::from_le_bytes(buf))
    }

    pub fn read_pdo_as_i64(
        &self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
    ) -> Option<i64> {
        let mut buf = [0; 8];
        self.read_pdo(slave_address, pdo_map_index, pdo_entry_index, &mut buf)?;
        Some(i64::from_le_bytes(buf))
    }

    pub fn write_pdo_as_bool(
        &mut self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
        data: bool,
    ) -> Option<()> {
        let buf = [data as u8];
        self.write_pdo(slave_address, pdo_map_index, pdo_entry_index, &buf)
    }

    pub fn write_pdo_as_u8(
        &mut self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
        data: u8,
    ) -> Option<()> {
        let buf = data.to_le_bytes();
        self.write_pdo(slave_address, pdo_map_index, pdo_entry_index, &buf)
    }

    pub fn write_pdo_as_i8(
        &mut self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
        data: i8,
    ) -> Option<()> {
        let buf = data.to_le_bytes();
        self.write_pdo(slave_address, pdo_map_index, pdo_entry_index, &buf)
    }

    pub fn write_pdo_as_u16(
        &mut self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
        data: u16,
    ) -> Option<()> {
        let buf = data.to_le_bytes();
        self.write_pdo(slave_address, pdo_map_index, pdo_entry_index, &buf)
    }

    pub fn write_pdo_as_i16(
        &mut self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
        data: i16,
    ) -> Option<()> {
        let buf = data.to_le_bytes();
        self.write_pdo(slave_address, pdo_map_index, pdo_entry_index, &buf)
    }

    pub fn write_pdo_as_u32(
        &mut self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
        data: u32,
    ) -> Option<()> {
        let buf = data.to_le_bytes();
        self.write_pdo(slave_address, pdo_map_index, pdo_entry_index, &buf)
    }

    pub fn write_pdo_as_i32(
        &mut self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
        data: i32,
    ) -> Option<()> {
        let buf = data.to_le_bytes();
        self.write_pdo(slave_address, pdo_map_index, pdo_entry_index, &buf)
    }

    pub fn write_pdo_as_u64(
        &mut self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
        data: u64,
    ) -> Option<()> {
        let buf = data.to_le_bytes();
        self.write_pdo(slave_address, pdo_map_index, pdo_entry_index, &buf)
    }

    pub fn write_pdo_as_i64(
        &mut self,
        slave_address: SlaveAddress,
        pdo_map_index: usize,
        pdo_entry_index: usize,
        data: i64,
    ) -> Option<()> {
        let buf = data.to_le_bytes();
        self.write_pdo(slave_address, pdo_map_index, pdo_entry_index, &buf)
    }
}
