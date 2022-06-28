use crate::arch::*;
use crate::cyclic::al_state_reader::*;
use crate::cyclic::al_state_transfer::*;
use crate::cyclic::mailbox::MailboxUnit;
use crate::cyclic::network_initilizer::*;
use crate::cyclic::ram_access_unit::RamAccessUnit;
use crate::cyclic::sdo::*;
use crate::cyclic::sii_reader;
use crate::cyclic::sii_reader::SiiReader;
use crate::cyclic::*;
use crate::error::EcError;
use crate::interface;
use crate::interface::Command;
use crate::interface::EtherCatInterface;
use crate::interface::SlaveAddress;
use crate::interface::TargetSlave;
use crate::network::*;
use crate::register::datalink::SiiData;
use crate::slave::AlState;
use crate::slave::PdoMapping;
use crate::slave::Slave;
use core::time::Duration;
use paste::paste;

#[derive(Debug)]
pub struct EtherCatMaster<'packet, 'units, 'mb, 'slave, 'pdo_mapping, 'pdo_entry, D, T>
where
    D: Device,
    T: CountDown,
{
    cyclic: CyclicUnits<'packet, 'units, D, CyclicUnitType<'mb>, T>,
    network: NetworkDescription<'slave, 'pdo_mapping, 'pdo_entry>,
}

impl<'packet, 'units, 'mb, 'slave, 'pdo_mapping, 'pdo_entry, D, T>
    EtherCatMaster<'packet, 'units, 'mb, 'slave, 'pdo_mapping, 'pdo_entry, D, T>
where
    D: Device,
    T: CountDown,
{
    pub fn initilize(
        iface: EtherCatInterface<'packet, D, T>,
        slave_buf: &'slave mut [Option<Slave<'pdo_mapping, 'pdo_entry>>],
        units_buf: &'units mut [Unit<CyclicUnitType<'mb>>],
    ) -> Result<Self, EcError<network_initilizer::Error>> {
        let mut units_tmp = [Unit::default()];
        let mut cyclic = CyclicUnits::new(iface, &mut units_tmp);
        let mut network = NetworkDescription::new(slave_buf);
        let handle = cyclic
            .add_unit(NetworkInitializer::new(&mut network))
            .unwrap();
        cyclic
            .get_unit(&handle)
            .unwrap()
            //.network_initilizer()
            .start();

        let mut count = 0;
        loop {
            cyclic.poll(EtherCatSystemTime(count), Duration::from_millis(1000))?;
            let net_init = cyclic.get_unit(&handle).unwrap(); //.network_initilizer();
            match net_init.wait() {
                None => {}
                Some(Err(err)) => return Err(err),
                Some(Ok(_)) => break,
            }
            count += 1000;
        }
        cyclic.remove_unit(handle).unwrap();
        let (iface, _) = cyclic.take_resources();
        Ok(Self {
            cyclic: CyclicUnits::new(iface, units_buf),
            network,
        })
    }

    pub fn poll<I: Into<Duration>>(
        &mut self,
        sys_time: EtherCatSystemTime,
        recv_timeout: I,
    ) -> Result<(), interface::Error> {
        self.cyclic.poll(sys_time, recv_timeout)
    }

    pub fn network(&self) -> &NetworkDescription<'slave, 'pdo_mapping, 'pdo_entry> {
        &self.network
    }

    pub fn read_sii(
        &mut self,
        slave_address: SlaveAddress,
        sii_address: u16,
    ) -> Result<(SiiData<[u8; SiiData::SIZE]>, usize), EcError<sii_reader::Error>> {
        let mut unit = SiiReader::new();
        unit.start(slave_address, sii_address);
        let handle = self.add_sii_reader(unit).unwrap();
        let mut count = 0;
        loop {
            if let Err(err) = self.poll(EtherCatSystemTime(count), Duration::from_millis(1000)) {
                self.remove_sii_reader(handle).unwrap();
                return Err(err.into());
            }
            let sii_reader = self.get_sii_reader(&handle).unwrap();
            match sii_reader.wait() {
                Some(Ok(data)) => {
                    self.remove_sii_reader(handle).unwrap();
                    return Ok(data);
                }
                None => {}
                Some(Err(other)) => {
                    self.remove_sii_reader(handle).unwrap();
                    return Err(other);
                }
            }
            count += 1000;
        }
    }

    pub fn read_al_state(
        &mut self,
        slave_address: TargetSlave,
    ) -> Result<(AlState, Option<AlStatusCode>), EcError<()>> {
        let mut unit = AlStateReader::new();
        unit.start(slave_address);
        let handle = self.add_al_state_reader(unit).unwrap();
        let mut count = 0;
        loop {
            if let Err(err) = self.poll(EtherCatSystemTime(count), Duration::from_millis(1000)) {
                self.remove_al_state_reader(handle).unwrap();
                return Err(err.into());
            }
            let al_state_reader = self.get_al_state_reader(&handle).unwrap();
            match al_state_reader.wait() {
                Some(Ok(data)) => {
                    self.remove_al_state_reader(handle).unwrap();
                    return Ok(data);
                }
                None => {}
                Some(Err(other)) => return Err(other),
            }
            count += 1000;
        }
    }

    pub fn transfer_al_state(
        &mut self,
        slave_address: TargetSlave,
        target_al_state: AlState,
    ) -> Result<AlState, EcError<al_state_transfer::Error>> {
        let mut unit = AlStateTransfer::new();
        unit.start(slave_address, target_al_state);
        let handle = self.add_al_state_transfer(unit).unwrap();
        let mut count = 0;
        loop {
            if let Err(err) = self.poll(EtherCatSystemTime(count), Duration::from_millis(1000)) {
                self.remove_al_state_transfer(handle).unwrap();
                return Err(err.into());
            }
            let al_state_transfer = self.get_al_state_transfer(&handle).unwrap();
            match al_state_transfer.wait() {
                Some(Ok(data)) => {
                    self.remove_al_state_transfer(handle).unwrap();
                    return Ok(data);
                }
                None => {}
                Some(Err(other)) => return Err(other),
            }
            count += 1000;
        }
    }

    pub fn read_sdo(
        &mut self,
        handle: &SdoUnitHandle,
        slave_address: SlaveAddress,
        index: u16,
        sub_index: u8,
    ) -> Result<(), EcError<sdo::Error>> {
        let Self {
            network, cyclic, ..
        } = self;
        let slave = network.slave(slave_address).unwrap().info();
        let sdo_unit = cyclic.get_sdo_unit(handle).unwrap();
        sdo_unit.start_to_read(slave, index, sub_index);
        let mut count = 0;
        loop {
            cyclic.poll(EtherCatSystemTime(count), Duration::from_millis(1000))?;
            let sdo_uploader = cyclic.get_sdo_unit(handle).unwrap();
            match sdo_uploader.wait() {
                Some(Ok(_)) => {
                    break;
                }
                None => {}
                Some(Err(other)) => return Err(other),
            }
            count += 1000;
        }
        Ok(())
    }

    pub fn write_sdo(
        &mut self,
        handle: &SdoUnitHandle,
        slave_address: SlaveAddress,
        index: u16,
        sub_index: u8,
        data: &[u8],
    ) -> Result<(), EcError<sdo::Error>> {
        let Self {
            network, cyclic, ..
        } = self;
        let slave = network.slave(slave_address).unwrap().info();
        let sdo_unit = cyclic.get_sdo_unit(handle).unwrap();
        sdo_unit.start_to_write(slave, index, sub_index, data);
        let mut count = 0;
        loop {
            cyclic.poll(EtherCatSystemTime(count), Duration::from_millis(1000))?;
            let sdo_downloader = cyclic.get_sdo_unit(handle).unwrap();
            match sdo_downloader.wait() {
                Some(Ok(_)) => {
                    break;
                }
                None => {}
                Some(Err(other)) => return Err(other),
            }
            count += 1000;
        }
        Ok(())
    }

    pub fn configure_pdo_mappings(
        &mut self,
        _sdo_unit_handle: &SdoUnitHandle,
    ) -> Result<(), EcError<sdo::Error>> {
        let Self { network, .. } = self;
        network.calculate_pdo_entry_positions_in_pdo_image();
        for pdo_maps in network
            .slaves()
            .into_iter()
            .filter_map(|s| s.pdo_mappings())
        {
            //PDOエントリーの割り当て
            for rx_pdo_map in pdo_maps.rx_mapping.iter() {
                let PdoMapping {
                    is_fixed: _,
                    index: _,
                    entries: _,
                } = rx_pdo_map;
                //self.write_sdo(sdo_unit_handle, SlaveAddress::SlavePosition(0), *index, 0, &[0])?;
            }
        }

        todo!()
    }
}

#[derive(Debug)]
pub enum CyclicUnitType<'a> {
    RamAccessUnit(RamAccessUnit),
    SiiReader(SiiReader),
    AlStateReader(AlStateReader),
    AlStateTransfer(AlStateTransfer),
    MailboxUnit(MailboxUnit<'a>),
    SdoUnit(SdoUnit<'a>),
}

impl<'a> CyclicProcess for CyclicUnitType<'a> {
    fn next_command(
        &mut self,
        //desc: &mut NetworkDescription,
        sys_time: EtherCatSystemTime,
    ) -> Option<(Command, &[u8])> {
        match self {
            Self::RamAccessUnit(unit) => unit.next_command(sys_time),
            Self::AlStateReader(unit) => unit.next_command(sys_time),
            Self::AlStateTransfer(unit) => unit.next_command(sys_time),
            Self::MailboxUnit(unit) => unit.next_command(sys_time),
            Self::SdoUnit(unit) => unit.next_command(sys_time),
            Self::SiiReader(unit) => unit.next_command(sys_time),
        }
    }

    fn recieve_and_process(
        &mut self,
        recv_data: Option<ReceivedData>,
        //desc: &mut NetworkDescription,
        sys_time: EtherCatSystemTime,
    ) {
        match self {
            Self::RamAccessUnit(unit) => unit.recieve_and_process(recv_data, sys_time),
            Self::AlStateReader(unit) => unit.recieve_and_process(recv_data, sys_time),
            Self::AlStateTransfer(unit) => unit.recieve_and_process(recv_data, sys_time),
            Self::MailboxUnit(unit) => unit.recieve_and_process(recv_data, sys_time),
            Self::SdoUnit(unit) => unit.recieve_and_process(recv_data, sys_time),
            Self::SiiReader(unit) => unit.recieve_and_process(recv_data, sys_time),
        }
    }
}

macro_rules! define_cyclic_unit {
    ($unit_name_snake: ident, $unit_name_pascal: ident) =>{
        paste!{
            #[derive(Debug)]
            pub struct [<$unit_name_pascal Handle>](UnitHandle);

            impl<'a> From<$unit_name_pascal> for CyclicUnitType<'a>{
                fn from(unit: $unit_name_pascal) -> Self{
                    Self::$unit_name_pascal(unit)
                }
            }

            impl<'a> CyclicUnitType<'a> {
                fn $unit_name_snake(&mut self) -> &mut $unit_name_pascal {
                    if let CyclicUnitType::$unit_name_pascal(ref mut unit) = self {
                        unit
                    } else {
                        panic!()
                    }
                }
                fn [<into_ $unit_name_snake>](self) -> $unit_name_pascal {
                    if let CyclicUnitType::$unit_name_pascal(unit) = self {
                        unit
                    } else {
                        panic!()
                    }
                }
            }

            impl<'packet, 'units, 'mb, D, T> CyclicUnits<'packet, 'units, D, CyclicUnitType<'mb>, T>
            where
                D: Device,
                T: CountDown,
            {
                pub fn [<add_ $unit_name_snake>](&mut self, $unit_name_snake: $unit_name_pascal) -> Option<[<$unit_name_pascal Handle>]>{
                    match self.add_unit($unit_name_snake.into()){
                        Ok(handle) => Some([<$unit_name_pascal Handle>](handle)),
                        Err(_) => None
                    }
                }

                pub fn [<get_ $unit_name_snake>](&mut self, handle: &[<$unit_name_pascal Handle>]) -> Option<&mut $unit_name_pascal>{
                    self.get_unit(&handle.0).map(|unit| unit.$unit_name_snake())
                }

                pub fn [<remove_ $unit_name_snake>](&mut self, handle: [<$unit_name_pascal Handle>]) -> Option<$unit_name_pascal>{
                    self.remove_unit(handle.0).map(|unit| unit.[<into_ $unit_name_snake>]())
                }
            }

            impl<'packet, 'units, 'mb, 'slave, 'pdo_mapping, 'pdo_entry, D, T>
            EtherCatMaster<'packet, 'units, 'mb, 'slave, 'pdo_mapping, 'pdo_entry, D, T>
            where
                D: Device,
                T: CountDown,
            {
                pub fn [<add_ $unit_name_snake>](&mut self, $unit_name_snake: $unit_name_pascal) -> Option<[<$unit_name_pascal Handle>]>{
                    self.cyclic.[<add_ $unit_name_snake>]($unit_name_snake)
                }
                pub fn [<get_ $unit_name_snake>](&mut self, handle: &[<$unit_name_pascal Handle>]) -> Option<&mut $unit_name_pascal>{
                    self.cyclic.[<get_ $unit_name_snake>](handle)
                }
                pub fn [<remove_ $unit_name_snake>](&mut self, handle: [<$unit_name_pascal Handle>]) -> Option<$unit_name_pascal>{
                    self.cyclic.[<remove_ $unit_name_snake>](handle)
                }
            }

        }
    };
}

macro_rules! define_cyclic_unit_with_lifetime {
    ($unit_name_snake: ident, $unit_name_pascal: ident) =>{
        paste!{
            #[derive(Debug)]
            pub struct [<$unit_name_pascal Handle>](UnitHandle);

            impl<'a> From<$unit_name_pascal<'a>> for CyclicUnitType<'a>{
                fn from(unit: $unit_name_pascal<'a>) -> Self{
                    Self::$unit_name_pascal(unit)
                }
            }

            impl<'a> CyclicUnitType<'a> {
                fn $unit_name_snake(&mut self) -> &mut $unit_name_pascal<'a> {
                    if let CyclicUnitType::$unit_name_pascal(ref mut unit) = self {
                        unit
                    } else {
                        panic!()
                    }
                }
                fn [<into_ $unit_name_snake>](self) -> $unit_name_pascal<'a> {
                    if let CyclicUnitType::$unit_name_pascal(unit) = self {
                        unit
                    } else {
                        panic!()
                    }
                }
            }

            impl<'packet, 'units, 'mb, D, T> CyclicUnits<'packet, 'units, D, CyclicUnitType<'mb>, T>
            where
                D: Device,
                T: CountDown,
            {
                pub fn [<add_ $unit_name_snake>](&mut self, $unit_name_snake: $unit_name_pascal<'mb>) -> Option<[<$unit_name_pascal Handle>]>{
                    match self.add_unit($unit_name_snake.into()){
                        Ok(handle) => Some([<$unit_name_pascal Handle>](handle)),
                        Err(_) => None
                    }
                }

                pub fn [<get_ $unit_name_snake>](&mut self, handle: &[<$unit_name_pascal Handle>]) -> Option<&mut $unit_name_pascal<'mb>>{
                    self.get_unit(&handle.0).map(|unit| unit.$unit_name_snake())
                }

                pub fn [<remove_ $unit_name_snake>](&mut self, handle: [<$unit_name_pascal Handle>]) -> Option<$unit_name_pascal<'mb>>{
                    self.remove_unit(handle.0).map(|unit| unit.[<into_ $unit_name_snake>]())
                }
            }

            impl<'packet, 'units, 'mb, 'slave, 'pdo_mapping, 'pdo_entry, D, T>
                EtherCatMaster<'packet, 'units, 'mb, 'slave, 'pdo_mapping, 'pdo_entry, D, T>
            where
                D: Device,
                T: CountDown,
            {
                pub fn [<add_ $unit_name_snake>](&mut self, $unit_name_snake: $unit_name_pascal<'mb>) -> Option<[<$unit_name_pascal Handle>]>{
                    self.cyclic.[<add_ $unit_name_snake>]($unit_name_snake)
                }
                pub fn [<get_ $unit_name_snake>](&mut self, handle: &[<$unit_name_pascal Handle>]) -> Option<&mut $unit_name_pascal<'mb>>{
                    self.cyclic.[<get_ $unit_name_snake>](handle)
                }
                pub fn [<remove_ $unit_name_snake>](&mut self, handle: [<$unit_name_pascal Handle>]) -> Option<$unit_name_pascal<'mb>>{
                    self.cyclic.[<remove_ $unit_name_snake>](handle)
                }
            }
        }
    };
}

define_cyclic_unit!(sii_reader, SiiReader);
define_cyclic_unit!(al_state_transfer, AlStateTransfer);
define_cyclic_unit!(al_state_reader, AlStateReader);
define_cyclic_unit!(ram_access_unit, RamAccessUnit);
define_cyclic_unit_with_lifetime!(mailbox_unit, MailboxUnit);
define_cyclic_unit_with_lifetime!(sdo_unit, SdoUnit);
