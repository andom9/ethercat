use crate::arch::*;
use crate::cyclic::al_state_reader::*;
use crate::cyclic::al_state_transfer::*;
use crate::cyclic::mailbox_reader::*;
use crate::cyclic::mailbox_writer::*;
use crate::cyclic::network_initilizer::*;
use crate::cyclic::sdo_downloader::*;
use crate::cyclic::sdo_uploader::*;
use crate::cyclic::sii_reader;
use crate::cyclic::sii_reader::SiiReader;
use crate::cyclic::*;
use crate::error::EcError;
use crate::interface;
use crate::interface::Command;
use crate::interface::EtherCatInterface;
use crate::interface::SlaveAddress;
use crate::network::*;
use crate::register::datalink::SiiData;
use crate::slave::AlState;
use crate::slave::Slave;
use core::time::Duration;
use paste::paste;

#[derive(Debug)]
pub struct EtherCatMaster<'a, D, T>
where
    D: Device,
    T: CountDown,
{
    cyclic: CyclicUnits<'a, D, CyclicUnitType<'a>, T, 4>,
    network: NetworkDescription<'a>,
}

impl<'a, D, T> EtherCatMaster<'a, D, T>
where
    D: Device,
    T: CountDown,
{
    pub fn initilize(
        iface: &'a mut EtherCatInterface<'a, D, T>,
        slave_buf: &'a mut [Option<Slave>],
    ) -> Result<Self, EcError<network_initilizer::Error>> {
        let mut cyclic = CyclicUnits::new(iface);
        let mut network = NetworkDescription::new(slave_buf);
        let handle = cyclic
            .add_unit(CyclicUnitType::NetworkInitilizer(NetworkInitilizer::new()))
            .unwrap();
        cyclic
            .get_unit(&handle)
            .unwrap()
            .network_initilizer()
            .start();

        let mut count = 0;
        loop {
            cyclic.poll(
                &mut network,
                EtherCatSystemTime(count),
                Duration::from_millis(1000),
            )?;
            let net_init = cyclic.get_unit(&handle).unwrap().network_initilizer();
            match net_init.wait() {
                None => {}
                Some(Err(err)) => return Err(err),
                Some(Ok(_)) => break,
            }
            count += 1000;
        }
        cyclic.remove_unit(handle).unwrap();
        Ok(Self { cyclic, network })
    }

    pub fn poll<I: Into<Duration>>(
        &mut self,
        sys_time: EtherCatSystemTime,
        recv_timeout: I,
    ) -> Result<(), interface::Error> {
        self.cyclic.poll(&mut self.network, sys_time, recv_timeout)
    }

    pub fn slaves(&self) -> &NetworkDescription {
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
            self.poll(EtherCatSystemTime(count), Duration::from_millis(1000))?;
            let sii_reader = self.get_sii_reader(&handle).unwrap();
            match sii_reader.wait() {
                Some(Ok(data)) => return Ok(data),
                None => {}
                Some(Err(other)) => return Err(other.into()),
            }
            count += 1000;
        }
    }

    pub fn read_al_state(&mut self, slave_address: Option<SlaveAddress>) -> Result<(AlState, Option<AlStatusCode>), EcError<()>>{
        let mut unit = AlStateReader::new();
        unit.start(slave_address);
        let handle = self.add_al_state_reader(unit).unwrap();
        let mut count = 0;
        loop {
            self.poll(EtherCatSystemTime(count), Duration::from_millis(1000))?;
            let al_state_reader = self.get_al_state_reader(&handle).unwrap();
            match al_state_reader.wait() {
                Some(Ok(data)) => return Ok(data),
                None => {}
                Some(Err(other)) => return Err(other.into()),
            }
            count += 1000;
        }
    }

    pub fn transfer_al_state(&mut self, slave_address: Option<SlaveAddress>, target_al_state: AlState) -> Result<AlState, EcError<al_state_transfer::Error>>{
        let mut unit = AlStateTransfer::new();
        unit.start(slave_address, target_al_state);
        let handle = self.add_al_state_transfer(unit).unwrap();
        let mut count = 0;
        loop {
            self.poll(EtherCatSystemTime(count), Duration::from_millis(1000))?;
            let al_state_transfer = self.get_al_state_transfer(&handle).unwrap();
            match al_state_transfer.wait() {
                Some(Ok(data)) => return Ok(data),
                None => {}
                Some(Err(other)) => return Err(other.into()),
            }
            count += 1000;
        }
    }
}

#[derive(Debug)]
pub enum CyclicUnitType<'a> {
    NetworkInitilizer(NetworkInitilizer),
    SiiReader(SiiReader),
    AlStateReader(AlStateReader),
    AlStateTransfer(AlStateTransfer),
    MailboxReader(MailboxReader<'a>),
    MailboxWriter(MailboxWriter<'a>),
    SdoDownloader(SdoDownloader<'a>),
    SdoUploader(SdoUploader<'a>),
}

impl<'a> CyclicProcess for CyclicUnitType<'a> {
    fn next_command(
        &mut self,
        desc: &mut NetworkDescription,
        sys_time: EtherCatSystemTime,
    ) -> Option<(Command, &[u8])> {
        match self {
            Self::AlStateReader(unit) => unit.next_command(desc, sys_time),
            Self::AlStateTransfer(unit) => unit.next_command(desc, sys_time),
            Self::MailboxReader(unit) => unit.next_command(desc, sys_time),
            Self::MailboxWriter(unit) => unit.next_command(desc, sys_time),
            Self::NetworkInitilizer(unit) => unit.next_command(desc, sys_time),
            Self::SdoDownloader(unit) => unit.next_command(desc, sys_time),
            Self::SdoUploader(unit) => unit.next_command(desc, sys_time),
            Self::SiiReader(unit) => unit.next_command(desc, sys_time),
        }
    }

    fn recieve_and_process(
        &mut self,
        recv_data: Option<ReceivedData>,
        desc: &mut NetworkDescription,
        sys_time: EtherCatSystemTime,
    ) {
        match self {
            Self::AlStateReader(unit) => unit.recieve_and_process(recv_data, desc, sys_time),
            Self::AlStateTransfer(unit) => unit.recieve_and_process(recv_data, desc, sys_time),
            Self::MailboxReader(unit) => unit.recieve_and_process(recv_data, desc, sys_time),
            Self::MailboxWriter(unit) => unit.recieve_and_process(recv_data, desc, sys_time),
            Self::NetworkInitilizer(unit) => unit.recieve_and_process(recv_data, desc, sys_time),
            Self::SdoDownloader(unit) => unit.recieve_and_process(recv_data, desc, sys_time),
            Self::SdoUploader(unit) => unit.recieve_and_process(recv_data, desc, sys_time),
            Self::SiiReader(unit) => unit.recieve_and_process(recv_data, desc, sys_time),
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

            impl<'a, D, T> EtherCatMaster<'a, D, T>
            where
                D: Device,
                T: CountDown,
            {
                pub fn [<add_ $unit_name_snake>](&mut self, $unit_name_snake: $unit_name_pascal) -> Option<[<$unit_name_pascal Handle>]>{
                    match self.cyclic.add_unit($unit_name_snake.into()){
                        Ok(handle) => Some([<$unit_name_pascal Handle>](handle)),
                        Err(_) => None
                    }
                }

                pub fn [<get_ $unit_name_snake>](&mut self, handle: &[<$unit_name_pascal Handle>]) -> Option<&mut $unit_name_pascal>{
                    self.cyclic.get_unit(&handle.0).map(|unit| unit.$unit_name_snake())
                }

                pub fn [<remove_ $unit_name_snake>](&mut self, handle: [<$unit_name_pascal Handle>]) -> Option<$unit_name_pascal>{
                    self.cyclic.remove_unit(handle.0).map(|unit| unit.[<into_ $unit_name_snake>]())
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
                fn $unit_name_snake(&'a mut self) -> &mut $unit_name_pascal {
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

            impl<'a, D, T> EtherCatMaster<'a, D, T>
            where
                D: Device,
                T: CountDown,
            {
                pub fn [<add_ $unit_name_snake>](&mut self, $unit_name_snake: $unit_name_pascal<'a>) -> Option<[<$unit_name_pascal Handle>]>{
                    match self.cyclic.add_unit($unit_name_snake.into()){
                        Ok(handle) => Some([<$unit_name_pascal Handle>](handle)),
                        Err(_) => None
                    }
                }

                pub fn [<get_ $unit_name_snake>](&'a mut self, handle: &[<$unit_name_pascal Handle>]) -> Option<&mut $unit_name_pascal<'a>>{
                    self.cyclic.get_unit(&handle.0).map(|unit| unit.$unit_name_snake())
                }

                pub fn [<remove_ $unit_name_snake>](&mut self, handle: [<$unit_name_pascal Handle>]) -> Option<$unit_name_pascal<'a>>{
                    self.cyclic.remove_unit(handle.0).map(|unit| unit.[<into_ $unit_name_snake>]())
                }
            }
        }
    };
}

define_cyclic_unit!(network_initilizer, NetworkInitilizer);
define_cyclic_unit!(sii_reader, SiiReader);
define_cyclic_unit!(al_state_transfer, AlStateTransfer);
define_cyclic_unit!(al_state_reader, AlStateReader);
define_cyclic_unit_with_lifetime!(mailbox_reader, MailboxReader);
define_cyclic_unit_with_lifetime!(mailbox_writer, MailboxWriter);
define_cyclic_unit_with_lifetime!(sdo_downloader, SdoDownloader);
define_cyclic_unit_with_lifetime!(sdo_uploader, SdoUploader);
