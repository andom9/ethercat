use core::time;

use bit_field::BitField;

use crate::{
    frame::MAX_PDU_DATAGRAM,
    interface::{PduSocket, RawEthernetDevice, SocketHandle, SocketInterface},
    register::{
        od::OdPdoEntry, CyclicOperationStartTime, DcActivation, DcSystemTime, FmmuRegister,
        Sync0CycleTime, Sync1CycleTime, SyncManagerActivation, SyncManagerControl,
    },
    slave::{CycleTime, Direction, FmmuConfig, PdoMapping, Slave, SlaveConfig, SyncMode},
};

use super::*;

impl<'frame, 'socket, 'slave, 'pdo_mapping, 'pdo_entry, D>
    EtherCatMaster<'frame, 'socket, 'slave, 'pdo_mapping, 'pdo_entry, D>
where
    D: RawEthernetDevice,
{
    /// If the buffer size is smaller than the image size, return false.
    pub fn register_process_data_buffer(&mut self, buf: &'socket mut [u8]) -> bool {
        if buf.len() < self.process_data_size() {
            return false;
        }
        let process_data_handle = self.sif.add_socket(PduSocket::new(buf)).unwrap();
        self.process_data_handle = Some(process_data_handle);
        true
    }

    /// Return process data size
    pub fn process_data_size(&self) -> usize {
        self.process_data_task.image_size()
    }

    /// Easy setup API. Use this in PreOperational state.
    pub fn configure_slaves_for_operation(&mut self) -> Result<(), ConfigError> {
        self.configure_pdo_image()?;
        self.configure_sync_mode()?;
        Ok(())
    }

    /// Easy API for configuration of PDO mappings
    fn configure_pdo_image(&mut self) -> Result<(), ConfigError> {
        self.set_pdo_config_to_od()?;
        self.set_pdo_to_sm()?;
        let (image_size, expected_wkc) = self.configure_fmmu()?;
        self.process_data_task.set_image_size(image_size);
        self.process_data_task.set_expected_wkc(expected_wkc);
        self.set_logical_address_to_pdo_entry_config();
        Ok(())
    }

    /// Set the PDO map config to object dictionary.
    fn set_pdo_config_to_od(&mut self) -> Result<(), ConfigError> {
        let Self {
            network,
            sif,
            gp_socket_handle,
            ..
        } = self;
        let handle = &gp_socket_handle;
        for (slave, slave_config) in network.slaves_mut() {
            set_pdo_config_to_od_utility(slave, slave_config, sif, handle, true)?;
            set_pdo_config_to_od_utility(slave, slave_config, sif, handle, false)?;
        }
        Ok(())
    }

    /// Assign the PDO map to the sync manager.
    fn set_pdo_to_sm(&mut self) -> Result<(), ConfigError> {
        let Self {
            network,
            sif,
            gp_socket_handle,
            ..
        } = self;
        let handle = &gp_socket_handle;
        for (slave, _) in network.slaves_mut() {
            if let Some(ram_address) = slave.info().process_data_physical_start_address() {
                let next_ram_address =
                    set_pdo_to_sm_utility(slave, sif, handle, Direction::Output, ram_address)?;
                set_pdo_to_sm_utility(slave, sif, handle, Direction::Input, next_ram_address)?;
            }
        }
        Ok(())
    }

    /// Return image size and expected wkc.
    fn set_logical_address_to_fmmu_config(&mut self) -> (usize, u16) {
        let mut expected_wkc = 0;
        let mut bit_address = (LOGICAL_START_ADDRESS * 8) as u64;
        for (slave, _) in self.network.slaves_mut() {
            let mut has_tx_data = false;
            let mut has_rx_data = false;

            for fmmu_config in slave
                .fmmu_config_mut()
                .iter_mut()
                .filter_map(|f| f.as_mut())
                .filter(|f| f.bit_length() != 0)
            {
                fmmu_config.set_logical_address(Some((bit_address >> 3) as u32));
                fmmu_config.set_start_bit((bit_address % 8) as u8);
                bit_address += fmmu_config.bit_length() as u64;

                match fmmu_config.direction() {
                    Direction::Output => has_rx_data = true,
                    Direction::Input => has_tx_data = true,
                }
            }

            if has_tx_data {
                expected_wkc += 1;
            }
            if has_rx_data {
                expected_wkc += 2;
            }
        }
        let size = if bit_address % 8 == 0 {
            (bit_address >> 3) - LOGICAL_START_ADDRESS as u64
        } else {
            (bit_address >> 3) + 1 - LOGICAL_START_ADDRESS as u64
        };
        assert!(
            size <= MAX_PDU_DATAGRAM as u64,
            "process data size must be less than or equal to 1468 for now"
        );
        (size as usize, expected_wkc)
    }

    fn set_logical_address_to_pdo_entry_config(&mut self) {
        for (slave, config) in self.network.slaves_mut() {
            // i=0 -> RX
            // i=1 -> TX
            for i in 0..2 {
                if slave.fmmu_config()[i].is_none()
                    || slave.fmmu_config()[i]
                        .as_ref()
                        .unwrap()
                        .logical_address()
                        .is_none()
                {
                    continue;
                }
                let fmmu_config = slave.fmmu_config()[i].as_ref().unwrap();
                let start_address = fmmu_config.logical_address().unwrap();
                let start_bit = fmmu_config.start_bit() as u64;
                let mut total_bits = (start_address * 8) as u64 + start_bit;
                let pdo_maps = if i == 0 {
                    config.output_process_data_mappings_mut()
                } else {
                    config.input_process_data_mappings_mut()
                };
                if pdo_maps.is_empty() {
                    continue;
                }
                for pdo_map in pdo_maps {
                    for pdo_entry in pdo_map.entries.iter_mut() {
                        let mod8 = total_bits % 8;
                        let (addr, s_bit) = if mod8 == 0 {
                            (total_bits >> 3, 0)
                        } else {
                            ((total_bits >> 3) + 1, mod8)
                        };
                        pdo_entry.set_start_bit(s_bit as u8);
                        pdo_entry.set_logical_address(Some(addr as u32));
                        total_bits += pdo_entry.bit_length() as u64;
                    }
                }
                assert_eq!(
                    fmmu_config.bit_length() as u64,
                    total_bits - ((start_address * 8) as u64 + start_bit)
                );
            }
        }
    }

    /// Set the logical address, physical address, and size for each slave FMMU.
    /// Return process data_image size and expected wkc.
    fn configure_fmmu(&mut self) -> Result<(usize, u16), ConfigError> {
        let (image_size, expected_wkc) = self.set_logical_address_to_fmmu_config();
        let Self {
            network,
            sif,
            gp_socket_handle,
            ..
        } = self;
        for (slave, _) in network.slaves() {
            for (i, fmmu) in slave
                .fmmu_config()
                .iter()
                .enumerate()
                .filter(|(_, f)| f.is_some())
            {
                let fmmu = fmmu.as_ref().unwrap();
                if fmmu.logical_address().is_none() || fmmu.byte_length() == 0 {
                    continue;
                }
                let mut fmmu_reg = FmmuRegister::new();
                fmmu_reg.set_logical_start_address(fmmu.logical_address().unwrap());
                let byte_length = fmmu.byte_length();
                let end_bit = fmmu.end_bit();
                fmmu_reg.set_length(byte_length);
                fmmu_reg.set_logical_end_bit(end_bit);
                fmmu_reg.set_physical_start_address(fmmu.physical_address());
                fmmu_reg.set_physical_start_bit(0);
                match fmmu.direction() {
                    Direction::Output => {
                        fmmu_reg.set_read_enable(false);
                        fmmu_reg.set_write_enable(true);
                    }
                    Direction::Input => {
                        fmmu_reg.set_read_enable(true);
                        fmmu_reg.set_write_enable(false);
                    }
                }
                fmmu_reg.set_enable(true);
                let addr = FmmuRegister::ADDRESS + (i as u16) * FmmuRegister::SIZE as u16;
                sif.write_register(
                    gp_socket_handle,
                    slave.info().slave_address().into(),
                    addr,
                    &fmmu_reg.0,
                )
                .map_err(|err| ConfigError {
                    slave_address: slave.info().slave_address(),
                    kind: ConfigErrorKind::SetFmmuRegister(RegisterError {
                        address: addr,
                        error: err,
                    }),
                })?;
            }
        }
        Ok((image_size, expected_wkc))
    }

    fn configure_sync_mode(&mut self) -> Result<(), ConfigError> {
        let Self {
            sif,
            network,
            gp_socket_handle,
            ..
        } = self;

        for (slave, config) in network.slaves().filter(|(s, _)| s.info().support_coe()) {
            // Set Operation Mode
            let index = 0x1C00;
            let sub_index = slave.info().process_data_rx_sm_number().unwrap() + 1;
            let has_rx_sm = match sif
                .read_sdo(&gp_socket_handle, slave, index, sub_index)
                .map_err(|err| ConfigError {
                    slave_address: slave.info().slave_address(),
                    kind: ConfigErrorKind::GetSyncManagerCommunicationType(SdoError {
                        index,
                        sub_index,
                        error: err,
                    }),
                })?[0]
            {
                0 => false,
                3 => true,
                _ => panic!("unsupported sm type"),
            };
            let rx_cycle_time_ns = if has_rx_sm {
                let addr = 0x1C30 + slave.info().process_data_rx_sm_number().unwrap() as u16;

                // get cycle time
                let cycle_time_ns = match config.cycle_time_ns {
                    CycleTime::DefaultValue => {
                        let time =
                            sif.read_sdo(&gp_socket_handle, slave, addr, 2)
                                .map_err(|err| ConfigError {
                                    slave_address: slave.info().slave_address(),
                                    kind: ConfigErrorKind::GetSyncManagerCycleTime(SdoError {
                                        index: addr,
                                        sub_index: 2,
                                        error: err,
                                    }),
                                })?;
                        u32::from_le_bytes([time[0], time[1], time[2], time[4]])
                    }
                    CycleTime::SpecifiedValue(time) => time,
                };

                //check cupported sync type
                let support_sync_type =
                    sif.read_sdo(&gp_socket_handle, slave, addr, 4)
                        .map_err(|err| ConfigError {
                            slave_address: slave.info().slave_address(),
                            kind: ConfigErrorKind::GetSyncManagerSyncType(SdoError {
                                index: addr,
                                sub_index: 4,
                                error: err,
                            }),
                        })?;
                match config.sync_mode {
                    SyncMode::FreeRun => {
                        if !support_sync_type[0].get_bit(0) {
                            return Err(ConfigError {
                                slave_address: slave.info().slave_address(),
                                kind: ConfigErrorKind::SyncModeNotSupported(SyncMode::FreeRun),
                            });
                        }
                    }
                    SyncMode::SyncManagerEvent => {
                        if !support_sync_type[0].get_bit(1) {
                            return Err(ConfigError {
                                slave_address: slave.info().slave_address(),
                                kind: ConfigErrorKind::SyncModeNotSupported(
                                    SyncMode::SyncManagerEvent,
                                ),
                            });
                        }
                    }
                    SyncMode::Sync0Event => {
                        if !support_sync_type[0].get_bit(2) {
                            return Err(ConfigError {
                                slave_address: slave.info().slave_address(),
                                kind: ConfigErrorKind::SyncModeNotSupported(SyncMode::Sync0Event),
                            });
                        }
                    }
                    SyncMode::Sync1Event => {
                        if !support_sync_type[0].get_bit(3) {
                            return Err(ConfigError {
                                slave_address: slave.info().slave_address(),
                                kind: ConfigErrorKind::SyncModeNotSupported(SyncMode::Sync1Event),
                            });
                        }
                    }
                }

                //check minimum cycle time
                let min_cycle_time_ns =
                    sif.read_sdo(&gp_socket_handle, slave, addr, 5)
                        .map_err(|err| ConfigError {
                            slave_address: slave.info().slave_address(),
                            kind: ConfigErrorKind::GetMinimumCycleTime(SdoError {
                                index: addr,
                                sub_index: 5,
                                error: err,
                            }),
                        })?;
                let min_cycle_time_ns = u32::from_le_bytes([
                    min_cycle_time_ns[0],
                    min_cycle_time_ns[1],
                    min_cycle_time_ns[2],
                    min_cycle_time_ns[3],
                ]);
                if cycle_time_ns < min_cycle_time_ns {
                    return Err(ConfigError {
                        slave_address: slave.info().slave_address(),
                        kind: ConfigErrorKind::CycleTimeTooSmall(min_cycle_time_ns),
                    });
                }

                sif.write_sdo(
                    &gp_socket_handle,
                    slave,
                    addr,
                    1,
                    &[config.sync_mode as u8, 0],
                )
                .map_err(|err| ConfigError {
                    slave_address: slave.info().slave_address(),
                    kind: ConfigErrorKind::SetSyncManagerSyncType(SdoError {
                        index: addr,
                        sub_index: 1,
                        error: err,
                    }),
                })?;
                // Cycle time
                if let CycleTime::SpecifiedValue(_) = config.cycle_time_ns {
                    sif.write_sdo(
                        &gp_socket_handle,
                        slave,
                        addr,
                        2,
                        &cycle_time_ns.to_le_bytes(),
                    )
                    .map_err(|err| ConfigError {
                        slave_address: slave.info().slave_address(),
                        kind: ConfigErrorKind::SetSyncManagerCycleTime(SdoError {
                            index: addr,
                            sub_index: 2,
                            error: err,
                        }),
                    })?;
                }

                // reset the rx error counters
                match config.sync_mode {
                    SyncMode::FreeRun => {}
                    _ => {
                        sif.write_sdo(&gp_socket_handle, slave, addr, 8, &[1, 0])
                            .map_err(|err| ConfigError {
                                slave_address: slave.info().slave_address(),
                                kind: ConfigErrorKind::ResetSyncError(SdoError {
                                    index: addr,
                                    sub_index: 8,
                                    error: err,
                                }),
                            })?;
                    }
                }
                cycle_time_ns
            } else {
                0
            };

            let index = 0x1C00;
            let sub_index = slave.info().process_data_tx_sm_number().unwrap() + 1;
            let has_tx_sm = match sif
                .read_sdo(&gp_socket_handle, slave, index, sub_index)
                .map_err(|err| ConfigError {
                    slave_address: slave.info().slave_address(),
                    kind: ConfigErrorKind::GetSyncManagerCommunicationType(SdoError {
                        index,
                        sub_index,
                        error: err,
                    }),
                })?[0]
            {
                0 => false,
                4 => true,
                _ => panic!("unsupproted sm type"),
            };
            let tx_cycle_time_ns = if has_tx_sm {
                let addr = 0x1C30 + slave.info().process_data_tx_sm_number().unwrap() as u16;

                // get cycle time
                let cycle_time_ns = match config.cycle_time_ns {
                    CycleTime::DefaultValue => {
                        let time =
                            sif.read_sdo(&gp_socket_handle, slave, addr, 2)
                                .map_err(|err| ConfigError {
                                    slave_address: slave.info().slave_address(),
                                    kind: ConfigErrorKind::GetSyncManagerCycleTime(SdoError {
                                        index: addr,
                                        sub_index: 2,
                                        error: err,
                                    }),
                                })?;
                        u32::from_le_bytes([time[0], time[1], time[2], time[4]])
                    }
                    CycleTime::SpecifiedValue(time) => time,
                };

                // check supported sync type
                let support_sync_type =
                    sif.read_sdo(&gp_socket_handle, slave, addr, 4)
                        .map_err(|err| ConfigError {
                            slave_address: slave.info().slave_address(),
                            kind: ConfigErrorKind::GetSyncManagerSyncType(SdoError {
                                index: addr,
                                sub_index: 4,
                                error: err,
                            }),
                        })?;
                match config.sync_mode {
                    SyncMode::FreeRun => {
                        if !support_sync_type[0].get_bit(0) {
                            return Err(ConfigError {
                                slave_address: slave.info().slave_address(),
                                kind: ConfigErrorKind::SyncModeNotSupported(SyncMode::FreeRun),
                            });
                        }
                    }
                    SyncMode::SyncManagerEvent => {
                        if !support_sync_type[0].get_bit(1) {
                            return Err(ConfigError {
                                slave_address: slave.info().slave_address(),
                                kind: ConfigErrorKind::SyncModeNotSupported(
                                    SyncMode::SyncManagerEvent,
                                ),
                            });
                        }
                    }
                    SyncMode::Sync0Event => {
                        if !support_sync_type[0].get_bit(2) {
                            return Err(ConfigError {
                                slave_address: slave.info().slave_address(),
                                kind: ConfigErrorKind::SyncModeNotSupported(SyncMode::Sync0Event),
                            });
                        }
                    }
                    SyncMode::Sync1Event => {
                        if !support_sync_type[0].get_bit(3) {
                            return Err(ConfigError {
                                slave_address: slave.info().slave_address(),
                                kind: ConfigErrorKind::SyncModeNotSupported(SyncMode::Sync1Event),
                            });
                        }
                    }
                }

                //check minimum cycle time
                let min_cycle_time_ns =
                    sif.read_sdo(&gp_socket_handle, slave, addr, 5)
                        .map_err(|err| ConfigError {
                            slave_address: slave.info().slave_address(),
                            kind: ConfigErrorKind::GetMinimumCycleTime(SdoError {
                                index: addr,
                                sub_index: 5,
                                error: err,
                            }),
                        })?;
                let min_cycle_time_ns = u32::from_le_bytes([
                    min_cycle_time_ns[0],
                    min_cycle_time_ns[1],
                    min_cycle_time_ns[2],
                    min_cycle_time_ns[3],
                ]);
                if cycle_time_ns < min_cycle_time_ns {
                    return Err(ConfigError {
                        slave_address: slave.info().slave_address(),
                        kind: ConfigErrorKind::CycleTimeTooSmall(min_cycle_time_ns),
                    });
                }

                sif.write_sdo(
                    &gp_socket_handle,
                    slave,
                    addr,
                    1,
                    &[config.sync_mode as u8, 0],
                )
                .map_err(|err| ConfigError {
                    slave_address: slave.info().slave_address(),
                    kind: ConfigErrorKind::SetSyncManagerSyncType(SdoError {
                        index: addr,
                        sub_index: 1,
                        error: err,
                    }),
                })?;
                // Cycle time
                if let CycleTime::SpecifiedValue(_) = config.cycle_time_ns {
                    sif.write_sdo(
                        &gp_socket_handle,
                        slave,
                        addr,
                        2,
                        &cycle_time_ns.to_le_bytes(),
                    )
                    .map_err(|err| ConfigError {
                        slave_address: slave.info().slave_address(),
                        kind: ConfigErrorKind::SetSyncManagerCycleTime(SdoError {
                            index: addr,
                            sub_index: 2,
                            error: err,
                        }),
                    })?;
                }

                // reset the tx error counters
                match config.sync_mode {
                    SyncMode::FreeRun => {}
                    _ => {
                        sif.write_sdo(&gp_socket_handle, slave, addr, 8, &[1, 0])
                            .map_err(|err| ConfigError {
                                slave_address: slave.info().slave_address(),
                                kind: ConfigErrorKind::ResetSyncError(SdoError {
                                    index: addr,
                                    sub_index: 8,
                                    error: err,
                                }),
                            })?;
                    }
                }
                cycle_time_ns
            } else {
                0
            };

            // set sync error limit
            match config.sync_mode {
                SyncMode::FreeRun => {}
                _ => {
                    sif.write_sdo(&gp_socket_handle, slave, 0x10F1, 2, &[0x9, 0])
                        .map_err(|err| ConfigError {
                            slave_address: slave.info().slave_address(),
                            kind: ConfigErrorKind::ResetSyncError(SdoError {
                                index: 0x10F1,
                                sub_index: 2,
                                error: err,
                            }),
                        })?;
                }
            }

            let cycle_time_ns = match config.cycle_time_ns {
                CycleTime::DefaultValue => rx_cycle_time_ns.max(tx_cycle_time_ns),
                CycleTime::SpecifiedValue(time) => time,
            };

            // Set interval of sync signal
            sif.write_register(
                &gp_socket_handle,
                slave.info().slave_address().into(),
                Sync0CycleTime::ADDRESS,
                &cycle_time_ns.to_le_bytes(),
            )
            .map_err(|err| ConfigError {
                slave_address: slave.info().slave_address(),
                kind: ConfigErrorKind::SetSync0CycleTime(RegisterError {
                    address: Sync0CycleTime::ADDRESS,
                    error: err,
                }),
            })?;

            sif.write_register(
                &gp_socket_handle,
                slave.info().slave_address().into(),
                Sync1CycleTime::ADDRESS,
                &(cycle_time_ns >> 1).to_le_bytes(),
            )
            .map_err(|err| ConfigError {
                slave_address: slave.info().slave_address(),
                kind: ConfigErrorKind::SetSync1CycleTime(RegisterError {
                    address: Sync1CycleTime::ADDRESS,
                    error: err,
                }),
            })?;

            // Start sync Signal
            let sys_time = sif
                .read_register(
                    &gp_socket_handle,
                    slave.info().slave_address().into(),
                    DcSystemTime::ADDRESS,
                    DcSystemTime::SIZE,
                )
                .map_err(|err| ConfigError {
                    slave_address: slave.info().slave_address(),
                    kind: ConfigErrorKind::SetSyncSignalStartTime(RegisterError {
                        address: DcSystemTime::ADDRESS,
                        error: err,
                    }),
                })?;
            let sys_time = DcSystemTime(sys_time).local_system_time();
            sif.write_register(
                &gp_socket_handle,
                slave.info().slave_address().into(),
                CyclicOperationStartTime::ADDRESS,
                &(sys_time + 10_000_000).to_le_bytes(),
            )
            .map_err(|err| ConfigError {
                slave_address: slave.info().slave_address(),
                kind: ConfigErrorKind::SetSyncSignalStartTime(RegisterError {
                    address: CyclicOperationStartTime::ADDRESS,
                    error: err,
                }),
            })?;

            // cycle permission
            let mut dc_actiation = DcActivation::new();
            if let SyncMode::Sync0Event | SyncMode::Sync1Event = config.sync_mode {
                dc_actiation.set_cyclic_operation_enable(true);
                dc_actiation.set_sync0_activate(true);
                dc_actiation.set_sync1_activate(true);
            } else {
                dc_actiation.set_cyclic_operation_enable(false);
                dc_actiation.set_sync0_activate(false);
                dc_actiation.set_sync1_activate(false);
            }

            sif.write_register(
                &gp_socket_handle,
                slave.info().slave_address().into(),
                DcActivation::ADDRESS,
                &dc_actiation.0,
            )
            .map_err(|err| ConfigError {
                slave_address: slave.info().slave_address(),
                kind: ConfigErrorKind::ActivateDc(RegisterError {
                    address: DcActivation::ADDRESS,
                    error: err,
                }),
            })?;

            let start_time = sif
                .read_register(
                    &gp_socket_handle,
                    slave.info().slave_address().into(),
                    CyclicOperationStartTime::ADDRESS,
                    DcSystemTime::SIZE,
                )
                .map_err(|err| ConfigError {
                    slave_address: slave.info().slave_address(),
                    kind: ConfigErrorKind::SetSyncSignalStartTime(RegisterError {
                        address: DcSystemTime::ADDRESS,
                        error: err,
                    }),
                })?;
            let start_time = DcSystemTime(start_time).local_system_time();
            loop {
                let sys_time = sif
                    .read_register(
                        &gp_socket_handle,
                        slave.info().slave_address().into(),
                        DcSystemTime::ADDRESS,
                        DcSystemTime::SIZE,
                    )
                    .map_err(|err| ConfigError {
                        slave_address: slave.info().slave_address(),
                        kind: ConfigErrorKind::SetSyncSignalStartTime(RegisterError {
                            address: DcSystemTime::ADDRESS,
                            error: err,
                        }),
                    })?;
                let sys_time = DcSystemTime(sys_time).local_system_time();
                if start_time < sys_time {
                    break;
                }
            }
        }
        Ok(())
    }
}

/// Set PDO map to obejct dictionary.
fn set_pdo_config_to_od_utility<'frame, 'socket, 'pdo_mapping, 'pdo_entry, D: RawEthernetDevice>(
    slave: &mut Slave,
    slave_config: &mut SlaveConfig<'pdo_mapping, 'pdo_entry>,
    sif: &mut SocketInterface<'frame, 'socket, D, NUM_SOCKETS>,
    handle: &SocketHandle,
    is_output: bool,
) -> Result<(), ConfigError> {
    let (pdo_mappings, sm_number) = if is_output {
        (
            slave_config.output_process_data_mappings(),
            slave.info().process_data_rx_sm_number(),
        )
    } else {
        (
            slave_config.input_process_data_mappings(),
            slave.info().process_data_tx_sm_number(),
        )
    };

    if let (Some(sm_number), pdo_mappings) = (sm_number, pdo_mappings) {
        let sm_assign = 0x1C10 + sm_number as u16;

        // Clear PDO mappings
        sif.write_sdo(handle, slave, sm_assign, 0, &[0])
            .map_err(|err| ConfigError {
                slave_address: slave.info().slave_address(),
                kind: ConfigErrorKind::ClearPdoMappings(SdoError {
                    index: sm_assign,
                    sub_index: 0,
                    error: err,
                }),
            })?;

        let mut map_index = 0;
        for pdo_map in pdo_mappings.iter() {
            if pdo_map.entries.is_empty() {
                continue;
            }
            map_index += 1;
            let PdoMapping {
                is_fixed,
                index: pdo_map_index,
                entries,
            } = pdo_map;
            // Assign pdo map to SM
            sif.write_sdo(
                handle,
                slave,
                sm_assign,
                map_index,
                &pdo_map_index.to_le_bytes(),
            )
            .map_err(|err| ConfigError {
                slave_address: slave.info().slave_address(),
                kind: ConfigErrorKind::AssignPdoMapToSyncManager(SdoError {
                    index: sm_assign,
                    sub_index: map_index,
                    error: err,
                }),
            })?;
            if *is_fixed {
                continue;
            }

            // Clear PDO entry of PDO map
            sif.write_sdo(handle, slave, *pdo_map_index, 0, &[0])
                .map_err(|err| ConfigError {
                    slave_address: slave.info().slave_address(),
                    kind: ConfigErrorKind::ClearPdoEntries(SdoError {
                        index: *pdo_map_index,
                        sub_index: 0,
                        error: err,
                    }),
                })?;
            let mut entry_index = 0;
            for entry in entries.iter() {
                let mut od_pdo_entry = OdPdoEntry::new();
                od_pdo_entry.set_index(entry.index());
                od_pdo_entry.set_sub_index(entry.sub_index());
                od_pdo_entry.set_bit_length(entry.bit_length());
                entry_index += 1;
                // Assign PDO entry to PDO map
                sif.write_sdo(handle, slave, *pdo_map_index, entry_index, &od_pdo_entry.0)
                    .map_err(|err| ConfigError {
                        slave_address: slave.info().slave_address(),
                        kind: ConfigErrorKind::AssignPdoEntryToPdoMap(SdoError {
                            index: *pdo_map_index,
                            sub_index: entry_index,
                            error: err,
                        }),
                    })?;
            }
            // How many entries were assigned to the PDO map?
            sif.write_sdo(handle, slave, *pdo_map_index, 0, &entry_index.to_le_bytes())
                .map_err(|err| ConfigError {
                    slave_address: slave.info().slave_address(),
                    kind: ConfigErrorKind::SetNumberOfPdoEntries(SdoError {
                        index: *pdo_map_index,
                        sub_index: 0,
                        error: err,
                    }),
                })?;
        }
        // How many PDO maps were assigned to the SM?
        sif.write_sdo(handle, slave, sm_assign, 0, &map_index.to_le_bytes())
            .map_err(|err| ConfigError {
                slave_address: slave.info().slave_address(),
                kind: ConfigErrorKind::SetNumberOfPdoMappings(SdoError {
                    index: sm_assign,
                    sub_index: 0,
                    error: err,
                }),
            })?;
    }
    Ok(())
}

/// Assign PDO map to SM.
/// Return next pdo ram address
/// NOTE: output = RX of slave.
fn set_pdo_to_sm_utility<'frame, 'socket, 'pdo_mapping, 'pdo_entry, D: RawEthernetDevice>(
    slave: &mut Slave,
    sif: &mut SocketInterface<'frame, 'socket, D, NUM_SOCKETS>,
    handle: &SocketHandle,
    direction: Direction,
    start_ram_address: u16,
) -> Result<u16, ConfigError> {
    let num_sm_comm = sif
        .read_sdo(handle, slave, 0x1C00, 0)
        .map_err(|err| ConfigError {
            slave_address: slave.info().slave_address(),
            kind: ConfigErrorKind::GetNumberOfSyncManagerChannel(SdoError {
                index: 0x1C00,
                sub_index: 0,
                error: err,
            }),
        })?;
    assert!(4 <= num_sm_comm[0]);

    let sm_number = match direction {
        Direction::Output => slave.info().process_data_rx_sm_number(),
        Direction::Input => slave.info().process_data_tx_sm_number(),
    };

    if let Some(sm_number) = sm_number {
        // Read SM type
        let is_pdo_map_none = if let Direction::Output = direction {
            let sm_type = sif
                .read_sdo(handle, slave, 0x1C00, sm_number + 1)
                .map_err(|err| ConfigError {
                    slave_address: slave.info().slave_address(),
                    kind: ConfigErrorKind::GetSyncManagerCommunicationType(SdoError {
                        index: 0x1C00,
                        sub_index: sm_number + 1,
                        error: err,
                    }),
                })?[0];
            match sm_type {
                0 => true,
                3 => false,
                _ => panic!("unsupported sm type"),
            }
        } else {
            let sm_type = sif
                .read_sdo(handle, slave, 0x1C00, sm_number + 1)
                .map_err(|err| ConfigError {
                    slave_address: slave.info().slave_address(),
                    kind: ConfigErrorKind::GetSyncManagerCommunicationType(SdoError {
                        index: 0x1C00,
                        sub_index: sm_number + 1,
                        error: err,
                    }),
                })?[0];
            match sm_type {
                0 => true,
                4 => false,
                _ => panic!("unsupproted sm type"),
            }
        };

        let mut pdo_map_bit_length = 0;
        let sm_assign = 0x1C10 + sm_number as u16;
        if !is_pdo_map_none {
            // Read PDO Maps and Entries from Obeject Dictiory.
            let num_maps = sif
                .read_sdo(handle, slave, sm_assign, 0)
                .map_err(|err| ConfigError {
                    slave_address: slave.info().slave_address(),
                    kind: ConfigErrorKind::GetNumberOfPdoMappings(SdoError {
                        index: sm_assign,
                        sub_index: 0,
                        error: err,
                    }),
                })?[0] as usize;
            for index in 1..(num_maps + 1) {
                let map_address = sif
                    .read_sdo(handle, slave, sm_assign, index as u8)
                    .map_err(|err| ConfigError {
                        slave_address: slave.info().slave_address(),
                        kind: ConfigErrorKind::GetPdoMappingAddress(SdoError {
                            index: sm_assign,
                            sub_index: index as u8,
                            error: err,
                        }),
                    })?;
                let map_address = u16::from_le_bytes([map_address[0], map_address[1]]);
                let num_entry =
                    sif.read_sdo(handle, slave, map_address, 0)
                        .map_err(|err| ConfigError {
                            slave_address: slave.info().slave_address(),
                            kind: ConfigErrorKind::GetNumberOfPdoEntries(SdoError {
                                index: map_address,
                                sub_index: 0,
                                error: err,
                            }),
                        })?[0] as usize;
                for entry_index in 1..(num_entry + 1) {
                    let entry = sif
                        .read_sdo(handle, slave, map_address, entry_index as u8)
                        .map_err(|err| ConfigError {
                            slave_address: slave.info().slave_address(),
                            kind: ConfigErrorKind::GetPdoEntrtyAddress(SdoError {
                                index: map_address,
                                sub_index: entry_index as u8,
                                error: err,
                            }),
                        })?;
                    let entry = OdPdoEntry(entry);
                    pdo_map_bit_length += entry.bit_length() as u32;
                }
            }
        }
        let size = if pdo_map_bit_length % 8 == 0 {
            pdo_map_bit_length >> 3
        } else {
            (pdo_map_bit_length >> 3) + 1
        };
        let size = size as u16;

        // Configure sm control register
        let sm_control_address = SyncManagerControl::ADDRESS + 0x08 * sm_number as u16;
        let mut sm_control = SyncManagerControl::new();
        sm_control.set_physical_start_address(start_ram_address);
        sm_control.set_length(size);
        sm_control.set_buffer_type(0b00); //buffer mode
        if let Direction::Output = direction {
            sm_control.set_direction(1); //pdi read access
        } else {
            sm_control.set_direction(0); //pdi write access
        }
        sm_control.set_dls_user_event_enable(true);
        sif.write_register(
            handle,
            slave.info().slave_address().into(),
            sm_control_address,
            &sm_control.0,
        )
        .unwrap(); //unwrap for now

        // Configure sm activation register
        let sm_activation_address = SyncManagerActivation::ADDRESS + 0x08 * sm_number as u16;
        let mut sm_active = SyncManagerActivation::new();
        if size != 0 {
            sm_active.set_channel_enable(true);
        } else {
            sm_active.set_channel_enable(false);
        }
        sm_active.set_repeat(false);
        sif.write_register(
            handle,
            slave.info().slave_address().into(),
            sm_activation_address,
            &sm_active.0,
        )
        .unwrap(); //unwrap for now

        // Set FMMU config of slave struct
        let fmmu_config = FmmuConfig::new(start_ram_address, size * 8, direction);
        if let Direction::Output = direction {
            slave.fmmu_config_mut()[0] = Some(fmmu_config);
        } else {
            slave.fmmu_config_mut()[1] = Some(fmmu_config);
        }
        Ok(start_ram_address + size * 3)
    } else {
        Ok(start_ram_address)
    }
}
