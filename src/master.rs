use crate::cyclic_task::{*, tasks::*};
use crate::error::EcError;
use crate::hal::*;
use crate::register::AlStatusCode;
use crate::register::SiiData;
use crate::register::SyncManagerActivation;
use crate::register::SyncManagerControl;
use crate::slave_network::AlState;
use crate::slave_network::PdoMapping;
use crate::slave_network::Slave;
use crate::slave_network::SlaveInfo;
use crate::slave_network::NetworkDescription;
use core::time::Duration;
use paste::paste;

#[derive(Debug)]
pub struct EtherCatMaster<'packet, 'tasks, 'mb, 'slave, 'pdo_mapping, 'pdo_entry, D, T>
where
    D: for<'d> Device<'d>,
    T: CountDown,
{
    cyclic: Option<CyclicTasks<'packet, 'tasks, D, CyclicTaskType<'mb>, T>>,
    network: NetworkDescription<'slave, 'pdo_mapping, 'pdo_entry>,
}

impl<'packet, 'tasks, 'mb, 'slave, 'pdo_mapping, 'pdo_entry, D, T>
    EtherCatMaster<'packet, 'tasks, 'mb, 'slave, 'pdo_mapping, 'pdo_entry, D, T>
where
    D: for<'d> Device<'d>,
    T: CountDown,
{
    pub fn initilize(
        iface: CommandInterface<'packet, D, T>,
        slave_buf: &'slave mut [Option<Slave<'pdo_mapping, 'pdo_entry>>],
        tasks_buf: &'tasks mut [TaskOption<CyclicTaskType<'mb>>],
    ) -> Result<Self, EcError<NetworkInitializerError>> {
        let mut tasks_tmp = [TaskOption::default()];
        let mut cyclic_tmp = CyclicTasks::new(iface, &mut tasks_tmp);
        let mut network = NetworkDescription::new(slave_buf);
        let handle = cyclic_tmp
            .add_task(NetworkInitializer::new(&mut network))
            .unwrap();
        cyclic_tmp.get_task(&handle).unwrap().start();

        let mut count = 0;
        loop {
            cyclic_tmp.poll(EtherCatSystemTime(count), Duration::from_millis(1000))?;
            let net_init = cyclic_tmp.get_task(&handle).unwrap();
            match net_init.wait() {
                None => {}
                Some(Err(err)) => return Err(err),
                Some(Ok(_)) => break,
            }
            count += 1000;
        }
        cyclic_tmp.remove_task(handle).unwrap();
        let (iface, _) = cyclic_tmp.take_resources();
        Ok(Self {
            cyclic: Some(CyclicTasks::new(iface, tasks_buf)),
            network,
        })
    }

    pub fn setup_dc(&mut self) -> Result<(), EcError<()>> {
        let cyclic = core::mem::take(&mut self.cyclic);
        let (iface, tasks) = cyclic.unwrap().take_resources();
        let mut tasks_tmp = [TaskOption::default()];
        let mut cyclic_tmp = CyclicTasks::new(iface, &mut tasks_tmp);
        let handle = cyclic_tmp
            .add_task(DcInitializer::new(&self.network))
            .unwrap();
        cyclic_tmp.get_task(&handle).unwrap().start();
        let mut count = 0;
        loop {
            cyclic_tmp.poll(EtherCatSystemTime(count), Duration::from_millis(1000))?;
            let dc_init = cyclic_tmp.get_task(&handle).unwrap();
            match dc_init.wait() {
                None => {}
                Some(Err(err)) => {
                    let (iface, _) = cyclic_tmp.take_resources();
                    self.cyclic = Some(CyclicTasks::new(iface, tasks));
                    return Err(err);
                }
                Some(Ok(_)) => break,
            }
            count += 1000;
        }
        cyclic_tmp.remove_task(handle).unwrap();
        let (iface, _) = cyclic_tmp.take_resources();
        self.cyclic = Some(CyclicTasks::new(iface, tasks));
        Ok(())
    }

    pub fn poll<I: Into<Duration>>(
        &mut self,
        sys_time: EtherCatSystemTime,
        recv_timeout: I,
    ) -> Result<(), CommandInterfaceError> {
        self.cyclic.as_mut().unwrap().poll(sys_time, recv_timeout)
    }

    pub fn network(&self) -> &NetworkDescription<'slave, 'pdo_mapping, 'pdo_entry> {
        &self.network
    }

    pub fn read_sii(
        &mut self,
        slave_address: SlaveAddress,
        sii_address: u16,
    ) -> Result<(SiiData<[u8; SiiData::SIZE]>, usize), EcError<SiiTaskError>> {
        self.cyclic
            .as_mut()
            .unwrap()
            .read_sii(slave_address, sii_address)
    }

    pub fn read_al_state(
        &mut self,
        slave_address: TargetSlave,
    ) -> Result<(AlState, Option<AlStatusCode>), EcError<()>> {
        self.cyclic.as_mut().unwrap().read_al_state(slave_address)
    }

    pub fn transfer_al_state(
        &mut self,
        slave_address: TargetSlave,
        target_al_state: AlState,
    ) -> Result<AlState, EcError<AlStateTransferError>> {
        self.cyclic
            .as_mut()
            .unwrap()
            .transfer_al_state(slave_address, target_al_state)
    }

    pub fn read_sdo(
        &mut self,
        handle: &SdoTaskHandle,
        slave_address: SlaveAddress,
        index: u16,
        sub_index: u8,
    ) -> Result<(), EcError<SdoTaskError>> {
        let slave_info = self.network.slave(slave_address).unwrap().info();
        self.cyclic
            .as_mut()
            .unwrap()
            .read_sdo(handle, slave_info, index, sub_index)
    }

    pub fn write_sdo(
        &mut self,
        handle: &SdoTaskHandle,
        slave_address: SlaveAddress,
        index: u16,
        sub_index: u8,
        data: &[u8],
    ) -> Result<(), EcError<SdoTaskError>> {
        let slave_info = self.network.slave(slave_address).unwrap().info();
        self.cyclic
            .as_mut()
            .unwrap()
            .write_sdo(handle, slave_info, index, sub_index, data)
    }

    pub fn configure_pdo_mappings(
        &mut self,
        sdo_task_handle: &SdoTaskHandle,
    ) -> Result<(), EcError<SdoTaskError>> {
        let Self {
            network, cyclic, ..
        } = self;
        let cyclic = cyclic.as_mut().unwrap();
        network.calculate_pdo_entry_positions_in_pdo_image();
        for slave in network.slaves() {
            //PDOマップが無ければ終わり
            if slave.pdo_mappings.is_none() {
                continue;
            }
            let slave_info = slave.info();

            ////////
            // RX
            ////////
            let mut rx_pdo_map_size = 0;
            if let Some(rx_sm_number) = slave_info.process_data_rx_sm_number() {
                let rx_sm_assign = 0x1c10 + rx_sm_number as u16;
                // SMへのPDOマップ割り当てをクリア
                cyclic.write_sdo(sdo_task_handle, slave_info, rx_sm_assign, 0, &[0])?;

                let pdo_maps = slave.pdo_mappings().unwrap();
                let mut map_index = 0;
                for rx_pdo_map in pdo_maps.rx_mapping.iter() {
                    if rx_pdo_map.entries.is_empty() {
                        continue;
                    }
                    map_index += 1;
                    let PdoMapping {
                        is_fixed,
                        index: pdo_index,
                        entries,
                    } = rx_pdo_map;
                    //SMへPDOマップを割り当て
                    cyclic.write_sdo(
                        sdo_task_handle,
                        slave_info,
                        rx_sm_assign,
                        map_index,
                        &pdo_index.to_le_bytes(),
                    )?;
                    if *is_fixed {
                        continue;
                    }
                    // PDOマップへのPDOエントリー割り当てをクリア
                    cyclic.write_sdo(sdo_task_handle, slave_info, *pdo_index, 0, &[0])?;
                    let mut entry_index = 0;
                    for entry in entries.iter() {
                        let mut data: u32 = 0;
                        data |= (entry.index as u32) << 16;
                        data |= (entry.sub_index as u32) << 8;
                        data |= entry.bit_length as u32;
                        entry_index += 1;
                        // PDOマップへPDOエントリーを割り当て
                        cyclic.write_sdo(
                            sdo_task_handle,
                            slave_info,
                            *pdo_index,
                            entry_index,
                            &data.to_le_bytes(),
                        )?;
                        let bit_diff = entry.bit_length() * 8 - entry.bit_length;
                        //パディング
                        if bit_diff != 0 {
                            entry_index += 1;
                            cyclic.write_sdo(
                                sdo_task_handle,
                                slave_info,
                                *pdo_index,
                                entry_index,
                                &(bit_diff as u32).to_le_bytes(),
                            )?;
                        }
                        rx_pdo_map_size += entry.byte_length();
                    }
                    //PDOマップに何個のエントリーを割り当てたか？
                    cyclic.write_sdo(
                        sdo_task_handle,
                        slave_info,
                        *pdo_index,
                        0,
                        &entry_index.to_le_bytes(),
                    )?;
                }

                //SMに何個のPDOを割り当てたか？
                cyclic.write_sdo(
                    sdo_task_handle,
                    slave_info,
                    rx_sm_assign,
                    0,
                    &map_index.to_be_bytes(),
                )?;

                //SMの設定
                let mut sm_control = SyncManagerControl::new();
                sm_control.set_physical_start_address(slave_info.pdo_start_address.unwrap());
                sm_control.set_length(rx_pdo_map_size);
                sm_control.set_buffer_type(0b00); //buffer mode
                sm_control.set_direction(1); //pdi read access
                sm_control.set_dls_user_event_enable(true);
                cyclic
                    .write_register(
                        slave_info.slave_address().into(),
                        SyncManagerControl::ADDRESS + 0x08 * rx_sm_number as u16,
                        &sm_control.0,
                    )
                    .unwrap(); //unwrap for now
                let mut sm_active = SyncManagerActivation::new();
                sm_active.set_channel_enable(true);
                sm_active.set_repeat(false);
                cyclic
                    .write_register(
                        slave_info.slave_address().into(),
                        SyncManagerActivation::ADDRESS + 0x08 * rx_sm_number as u16,
                        &sm_active.0,
                    )
                    .unwrap(); //unwrap for now
            }

            ////////
            // TX
            ////////
            let mut tx_pdo_map_size = 0;
            if let Some(tx_sm_number) = slave_info.process_data_tx_sm_number() {
                let tx_sm_assign = 0x1c10 + tx_sm_number as u16;
                //smへのPDOマップの割り当てをクリア
                cyclic.write_sdo(sdo_task_handle, slave_info, tx_sm_assign, 0, &[0])?;

                let pdo_maps = slave.pdo_mappings().unwrap();
                //PDOマップにエントリーを割り当てる
                let mut map_index = 0;
                for tx_pdo_map in pdo_maps.tx_mapping.iter() {
                    if tx_pdo_map.entries.is_empty() {
                        continue;
                    }
                    map_index += 1;
                    let PdoMapping {
                        is_fixed,
                        index: pdo_index,
                        entries,
                    } = tx_pdo_map;
                    // SMにPDOマップを割り当てる
                    cyclic.write_sdo(
                        sdo_task_handle,
                        slave_info,
                        tx_sm_assign,
                        map_index,
                        &pdo_index.to_le_bytes(),
                    )?;
                    if *is_fixed {
                        continue;
                    }
                    //まずsub_index=0を0でクリアする。
                    cyclic.write_sdo(sdo_task_handle, slave_info, *pdo_index, 0, &[0])?;
                    let mut entry_index = 0;
                    for entry in entries.iter() {
                        let mut data: u32 = 0;
                        data |= (entry.index as u32) << 16;
                        data |= (entry.sub_index as u32) << 8;
                        data |= entry.bit_length as u32;
                        entry_index += 1;
                        cyclic.write_sdo(
                            sdo_task_handle,
                            slave_info,
                            *pdo_index,
                            entry_index,
                            &data.to_le_bytes(),
                        )?;
                        let bit_diff = entry.bit_length() * 8 - entry.bit_length;
                        //パディング
                        if bit_diff != 0 {
                            entry_index += 1;
                            cyclic.write_sdo(
                                sdo_task_handle,
                                slave_info,
                                *pdo_index,
                                entry_index,
                                &(bit_diff as u32).to_le_bytes(),
                            )?;
                        }
                        tx_pdo_map_size += entry.byte_length();
                    }
                    //PDOマップに何個のエントリーを割り当てたか？
                    cyclic.write_sdo(
                        sdo_task_handle,
                        slave_info,
                        *pdo_index,
                        0,
                        &entry_index.to_le_bytes(),
                    )?;
                }

                //SMに何個のPDOを割り当てたか？
                cyclic.write_sdo(
                    sdo_task_handle,
                    slave_info,
                    tx_sm_assign,
                    0,
                    &map_index.to_be_bytes(),
                )?;
                assert!(rx_pdo_map_size * 3 + tx_pdo_map_size * 3 <= slave_info.pdo_ram_size);
                //SMの設定
                let mut sm_control = SyncManagerControl::new();
                sm_control.set_physical_start_address(
                    slave_info.pdo_start_address.unwrap() + rx_pdo_map_size * 3,
                );
                sm_control.set_length(tx_pdo_map_size);
                sm_control.set_buffer_type(0b00); //buffer mode
                sm_control.set_direction(0); //pdi write access
                sm_control.set_dls_user_event_enable(true);
                cyclic
                    .write_register(
                        slave_info.slave_address().into(),
                        SyncManagerControl::ADDRESS + 0x08 * tx_sm_number as u16,
                        &sm_control.0,
                    )
                    .unwrap(); //unwrap for now
                let mut sm_active = SyncManagerActivation::new();
                sm_active.set_channel_enable(true);
                sm_active.set_repeat(false);
                cyclic
                    .write_register(
                        slave_info.slave_address().into(),
                        SyncManagerActivation::ADDRESS + 0x08 * tx_sm_number as u16,
                        &sm_active.0,
                    )
                    .unwrap(); //unwrap for now
            }
        }
        Ok(())
    }
}

impl<'packet, 'tasks, 'mb, D, T> CyclicTasks<'packet, 'tasks, D, CyclicTaskType<'mb>, T>
where
    D: for<'d> Device<'d>,
    T: CountDown,
{
    pub fn write_register(
        &mut self,
        slave_address: TargetSlave,
        ado: u16,
        data: &[u8],
    ) -> Result<(), EcError<()>> {
        let mut task = RamAccessTask::new();
        task.start_to_write(slave_address, ado, data);
        let handle = self.add_ram_access_task(task).unwrap();
        let mut count = 0;
        loop {
            if let Err(err) = self.poll(EtherCatSystemTime(count), Duration::from_millis(1000)) {
                self.remove_ram_access_task(handle).unwrap();
                return Err(err.into());
            }
            let sii_reader = self.get_ram_access_task(&handle).unwrap();
            match sii_reader.wait() {
                Some(Ok(_)) => {
                    self.remove_ram_access_task(handle).unwrap();
                    return Ok(());
                }
                None => {}
                Some(Err(other)) => {
                    self.remove_ram_access_task(handle).unwrap();
                    return Err(other);
                }
            }
            count += 1000;
        }
    }

    pub fn read_sii(
        &mut self,
        slave_address: SlaveAddress,
        sii_address: u16,
    ) -> Result<(SiiData<[u8; SiiData::SIZE]>, usize), EcError<SiiTaskError>> {
        let mut task = SiiReader::new();
        task.start(slave_address, sii_address);
        let handle = self.add_sii_reader(task).unwrap();
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
        let mut task = AlStateReader::new();
        task.start(slave_address);
        let handle = self.add_al_state_reader(task).unwrap();
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
    ) -> Result<AlState, EcError<AlStateTransferError>> {
        let mut task = AlStateTransfer::new();
        task.start(slave_address, target_al_state);
        let handle = self.add_al_state_transfer(task).unwrap();
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
        handle: &SdoTaskHandle,
        slave_info: &SlaveInfo,
        index: u16,
        sub_index: u8,
    ) -> Result<(), EcError<SdoTaskError>> {
        let sdo_task = self.get_sdo_task(handle).unwrap();
        sdo_task.start_to_read(slave_info, index, sub_index);
        let mut count = 0;
        loop {
            self.poll(EtherCatSystemTime(count), Duration::from_millis(1000))?;
            let sdo_uploader = self.get_sdo_task(handle).unwrap();
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
        handle: &SdoTaskHandle,
        slave_info: &SlaveInfo,
        index: u16,
        sub_index: u8,
        data: &[u8],
    ) -> Result<(), EcError<SdoTaskError>> {
        let sdo_task = self.get_sdo_task(handle).unwrap();
        sdo_task.start_to_write(slave_info, index, sub_index, data);
        let mut count = 0;
        loop {
            self.poll(EtherCatSystemTime(count), Duration::from_millis(1000))?;
            let sdo_downloader = self.get_sdo_task(handle).unwrap();
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
}

#[derive(Debug)]
pub enum CyclicTaskType<'a> {
    RamAccessTask(RamAccessTask),
    SiiReader(SiiReader),
    AlStateReader(AlStateReader),
    AlStateTransfer(AlStateTransfer),
    MailboxTask(MailboxTask<'a>),
    SdoTask(SdoTask<'a>),
}

impl<'a> CyclicProcess for CyclicTaskType<'a> {
    fn next_command(&mut self, sys_time: EtherCatSystemTime) -> Option<(Command, &[u8])> {
        match self {
            Self::RamAccessTask(task) => task.next_command(sys_time),
            Self::AlStateReader(task) => task.next_command(sys_time),
            Self::AlStateTransfer(task) => task.next_command(sys_time),
            Self::MailboxTask(task) => task.next_command(sys_time),
            Self::SdoTask(task) => task.next_command(sys_time),
            Self::SiiReader(task) => task.next_command(sys_time),
        }
    }

    fn recieve_and_process(
        &mut self,
        recv_data: Option<ReceivedData>,
        sys_time: EtherCatSystemTime,
    ) {
        match self {
            Self::RamAccessTask(task) => task.recieve_and_process(recv_data, sys_time),
            Self::AlStateReader(task) => task.recieve_and_process(recv_data, sys_time),
            Self::AlStateTransfer(task) => task.recieve_and_process(recv_data, sys_time),
            Self::MailboxTask(task) => task.recieve_and_process(recv_data, sys_time),
            Self::SdoTask(task) => task.recieve_and_process(recv_data, sys_time),
            Self::SiiReader(task) => task.recieve_and_process(recv_data, sys_time),
        }
    }
}

macro_rules! define_cyclic_task {
    ($task_name_snake: ident, $task_name_pascal: ident) =>{
        paste!{
            #[derive(Debug)]
            pub struct [<$task_name_pascal Handle>](TaskHandle);

            impl<'a> From<$task_name_pascal> for CyclicTaskType<'a>{
                fn from(task: $task_name_pascal) -> Self{
                    Self::$task_name_pascal(task)
                }
            }

            impl<'a> CyclicTaskType<'a> {
                fn $task_name_snake(&mut self) -> &mut $task_name_pascal {
                    if let CyclicTaskType::$task_name_pascal(ref mut task) = self {
                        task
                    } else {
                        panic!()
                    }
                }
                fn [<into_ $task_name_snake>](self) -> $task_name_pascal {
                    if let CyclicTaskType::$task_name_pascal(task) = self {
                        task
                    } else {
                        panic!()
                    }
                }
            }

            impl<'packet, 'tasks, 'mb, D, T> CyclicTasks<'packet, 'tasks, D, CyclicTaskType<'mb>, T>
            where
                D: for<'d> Device<'d>,
                T: CountDown,
            {
                pub fn [<add_ $task_name_snake>](&mut self, $task_name_snake: $task_name_pascal) -> Option<[<$task_name_pascal Handle>]>{
                    match self.add_task($task_name_snake.into()){
                        Ok(handle) => Some([<$task_name_pascal Handle>](handle)),
                        Err(_) => None
                    }
                }

                pub fn [<get_ $task_name_snake>](&mut self, handle: &[<$task_name_pascal Handle>]) -> Option<&mut $task_name_pascal>{
                    self.get_task(&handle.0).map(|task| task.$task_name_snake())
                }

                pub fn [<remove_ $task_name_snake>](&mut self, handle: [<$task_name_pascal Handle>]) -> Option<$task_name_pascal>{
                    self.remove_task(handle.0).map(|task| task.[<into_ $task_name_snake>]())
                }
            }

            impl<'packet, 'tasks, 'mb, 'slave, 'pdo_mapping, 'pdo_entry, D, T>
            EtherCatMaster<'packet, 'tasks, 'mb, 'slave, 'pdo_mapping, 'pdo_entry, D, T>
            where
                D: for<'d> Device<'d>,
                T: CountDown,
            {
                pub fn [<add_ $task_name_snake>](&mut self, $task_name_snake: $task_name_pascal) -> Option<[<$task_name_pascal Handle>]>{
                    self.cyclic.as_mut().unwrap().[<add_ $task_name_snake>]($task_name_snake)
                }
                pub fn [<get_ $task_name_snake>](&mut self, handle: &[<$task_name_pascal Handle>]) -> Option<&mut $task_name_pascal>{
                    self.cyclic.as_mut().unwrap().[<get_ $task_name_snake>](handle)
                }
                pub fn [<remove_ $task_name_snake>](&mut self, handle: [<$task_name_pascal Handle>]) -> Option<$task_name_pascal>{
                    self.cyclic.as_mut().unwrap().[<remove_ $task_name_snake>](handle)
                }
            }

        }
    };
}

macro_rules! define_cyclic_task_with_lifetime {
    ($task_name_snake: ident, $task_name_pascal: ident) =>{
        paste!{
            #[derive(Debug)]
            pub struct [<$task_name_pascal Handle>](TaskHandle);

            impl<'a> From<$task_name_pascal<'a>> for CyclicTaskType<'a>{
                fn from(task: $task_name_pascal<'a>) -> Self{
                    Self::$task_name_pascal(task)
                }
            }

            impl<'a> CyclicTaskType<'a> {
                fn $task_name_snake(&mut self) -> &mut $task_name_pascal<'a> {
                    if let CyclicTaskType::$task_name_pascal(ref mut task) = self {
                        task
                    } else {
                        panic!()
                    }
                }
                fn [<into_ $task_name_snake>](self) -> $task_name_pascal<'a> {
                    if let CyclicTaskType::$task_name_pascal(task) = self {
                        task
                    } else {
                        panic!()
                    }
                }
            }

            impl<'packet, 'tasks, 'mb, D, T> CyclicTasks<'packet, 'tasks, D, CyclicTaskType<'mb>, T>
            where
                D: for<'d> Device<'d>,
                T: CountDown,
            {
                pub fn [<add_ $task_name_snake>](&mut self, $task_name_snake: $task_name_pascal<'mb>) -> Option<[<$task_name_pascal Handle>]>{
                    match self.add_task($task_name_snake.into()){
                        Ok(handle) => Some([<$task_name_pascal Handle>](handle)),
                        Err(_) => None
                    }
                }

                pub fn [<get_ $task_name_snake>](&mut self, handle: &[<$task_name_pascal Handle>]) -> Option<&mut $task_name_pascal<'mb>>{
                    self.get_task(&handle.0).map(|task| task.$task_name_snake())
                }

                pub fn [<remove_ $task_name_snake>](&mut self, handle: [<$task_name_pascal Handle>]) -> Option<$task_name_pascal<'mb>>{
                    self.remove_task(handle.0).map(|task| task.[<into_ $task_name_snake>]())
                }
            }

            impl<'packet, 'tasks, 'mb, 'slave, 'pdo_mapping, 'pdo_entry, D, T>
                EtherCatMaster<'packet, 'tasks, 'mb, 'slave, 'pdo_mapping, 'pdo_entry, D, T>
            where
                D: for<'d> Device<'d>,
                T: CountDown,
            {
                pub fn [<add_ $task_name_snake>](&mut self, $task_name_snake: $task_name_pascal<'mb>) -> Option<[<$task_name_pascal Handle>]>{
                    self.cyclic.as_mut().unwrap().[<add_ $task_name_snake>]($task_name_snake)
                }
                pub fn [<get_ $task_name_snake>](&mut self, handle: &[<$task_name_pascal Handle>]) -> Option<&mut $task_name_pascal<'mb>>{
                    self.cyclic.as_mut().unwrap().[<get_ $task_name_snake>](handle)
                }
                pub fn [<remove_ $task_name_snake>](&mut self, handle: [<$task_name_pascal Handle>]) -> Option<$task_name_pascal<'mb>>{
                    self.cyclic.as_mut().unwrap().[<remove_ $task_name_snake>](handle)
                }
            }
        }
    };
}

define_cyclic_task!(sii_reader, SiiReader);
define_cyclic_task!(al_state_transfer, AlStateTransfer);
define_cyclic_task!(al_state_reader, AlStateReader);
define_cyclic_task!(ram_access_task, RamAccessTask);
define_cyclic_task_with_lifetime!(mailbox_task, MailboxTask);
define_cyclic_task_with_lifetime!(sdo_task, SdoTask);
