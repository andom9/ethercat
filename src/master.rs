use crate::arch::Device;
use crate::cyclic::al_state_transfer::*;
use crate::cyclic::network_initilizer::*;
use crate::cyclic::sii::*;
use crate::cyclic::*;
use crate::error::CommonError;
use crate::interface::Command;
use crate::network::*;
use embedded_hal::timer::*;
use fugit::*;

enum CyclicUnit<'a, T, const N: usize>
where
    T: CountDown<Time = MicrosDurationU32>,
{
    SIIReader(SIIReader<'a, T>),
    ALStateTransfer(ALStateTransfer<'a, T>),
    NetworkInitilizer(NetworkInitilizer<'a, T, N>),
}

impl<'a, T, const N: usize> Cyclic for CyclicUnit<'a, T, N>
where
    T: CountDown<Time = MicrosDurationU32>,
{
    fn next_command(&mut self) -> Option<(Command, &[u8])> {
        todo!()
    }
    fn recieve_and_process(&mut self, command: Command, data: &[u8], wkc: u16) -> bool {
        todo!()
    }
}

pub struct EtherCatMaster<'a, D, T, const N: usize>
where
    D: Device,
    T: CountDown<Time = MicrosDurationU32>,
{
    cyclic: CyclicProcess<'a, D, T, CyclicUnit<'a, T, N>, 4>,
    network_initilizer_handle: UnitHandle,
    network: Option<EtherCATNetwork<N>>,
    sii_reader_handle: UnitHandle,
    al_state_transfer_handle: UnitHandle,
}

impl<'a, D, T, const N: usize> EtherCatMaster<'a, D, T, N>
where
    D: Device,
    T: CountDown<Time = MicrosDurationU32>,
{
    pub fn poll<I: Into<MicrosDurationU32> + Clone>(
        &mut self,
        recv_timeout: I,
    ) -> Result<(), CommonError> {
        self.cyclic.poll(recv_timeout)
    }

    pub fn start_init(&mut self) {
        let unit = self.network_initilizer();
        unit.start();
        self.network = None;
    }

    pub fn wait_init(&mut self) -> nb::Result<(), NetworkInitError> {
        let unit = self.network_initilizer();
        let network = unit.wait()?;
        if let Some(network) = network{
            self.network = Some(network);
        }
        Ok(())
    }

    fn network_initilizer(&mut self) -> &mut NetworkInitilizer<'a, T, N> {
        let network_initilizer = self
            .cyclic
            .unit_mut(self.network_initilizer_handle)
            .unwrap();
        if let CyclicUnit::NetworkInitilizer(ref mut unit) = network_initilizer {
            unit
        } else {
            unreachable!()
        }
    }

    pub fn sii_reader(&mut self) -> &mut SIIReader<'a, T> {
        let sii_reader = self.cyclic.unit_mut(self.sii_reader_handle).unwrap();
        if let CyclicUnit::SIIReader(ref mut unit) = sii_reader {
            unit
        } else {
            unreachable!()
        }
    }

    pub fn al_state_transfer(&mut self) -> &mut ALStateTransfer<'a, T> {
        let al_state_transfer = self.cyclic.unit_mut(self.al_state_transfer_handle).unwrap();
        if let CyclicUnit::ALStateTransfer(ref mut unit) = al_state_transfer {
            unit
        } else {
            unreachable!()
        }
    }
}
