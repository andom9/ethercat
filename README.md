# EtherCAT Master
This crate provides a `no_std` and no `alloc` API for EtherCAT master devices.

## Documentaion
WIP

## Usage
WIP

## Supported Features
It is difficult to implement and test all features of an EtherCAT master. This section describes the features that are or will be supported and those that are not intended to be supported.

Please refer to the [ETG.1500](https://www.ethercat.org/download/documents/ETG1500_V1i0i2_D_R_MasterClasses.pdf) for more information on the features listed below.

*legend*:<br>
🔳  Supported.<br>
⬜  Not supported but will be addressed.<br>
🚫  No plans to support.<br>

**Basic Features**
- 🔳 Service Commands
- 🚫 IRQ field in datagram
- 🔳 Slave with Device Emulation
- 🔳 EtherCAT State Machine
- 🚫 VLAN
- 🔳 EtherCAT Frame Types
- 🚫 UDP Frame Types

**Error Detections**
- 🔳 Checking Working Counter
- 🔳 Checking AL Status Code and EtherCAT State
- ⬜ Setting SM Watchdog
- 🚫 Checking PDO State
- ⬜ Checking Lost Slaves
- 🔳 Checking Lost Frames
- 🔳 Checking Error Counter in Slaves
- 🚫 Checking Sync Error Flag(0x1C33:20)

**Process Data Exchenge**
- ⬜ Cyclic PDO
- 🚫 Cyclic PDO using LRD and LWR
- 🚫 Multiple Tasks
- 🚫 Frame repetition

**Network Configuration**
- ⬜ Online SII Scanning
- ⬜ Reading ENI
- ⬜ Compare Network configuration
- 🚫 Explicit Device Identification
- ⬜ Station Alias Addressing
- 🔳 Reading SII(EEPROM)
- ⬜ Writing SII(EEPROM)

**Mailbox Features**
- 🔳 Mailbox
- 🔳 Mailbox Resilient Layer
- 🚫 Multiple Mailbox Channels
- ⬜ Mailbox Polling

**CoE**
- 🔳 CoE
  - 🔳 SDO Up/Donwload
  - ⬜ Segmented Transfer
  - ⬜ Complete Access
  - ⬜ SDO info service
  - ⬜ Emergency Message

**EoE**
- 🚫 EoE

**SoE**
- 🚫 SoE

**AoE**
- 🚫 AoE

**FoE**
- ⬜ FoE
- ⬜ Boot State

**Synchronization with Distributed Clocks**
- ⬜ DC Support
- ⬜ Continous Propagation Delay compemsation
- ⬜ Sync Window monitoring

**Slave-to-Slave Communication**
- ⬜ via Master

**Master Information**
- 🚫 Master Object Dictionary

**FP Cable Redundancy**
- 🚫 Cable redundancy
- 🚫 Hot Connect

**Other Slave Options**
- 🚫 UseLrdLwr
- ⬜ SM:OpOnly
- 🚫 SeparateSu
- 🚫 SeparateFrame
- 🚫 FrameRepeatSupport
- 🚫 AssignToPdi
- ⬜ InitCmd
- 🚫 UnknownFRMW
- 🚫 Unknown64Bit
- 🚫 Reg0108
- 🚫 Reg0400
- 🚫 Reg0410
- 🚫 Reg0420
- 🚫 StateMachine:Behavior

## License

Licensed under either of

 * [Apache License, Version 2.0](http://www.apache.org/licenses/LICENSE-2.0)
 * [MIT license](http://opensource.org/licenses/MIT)

at your option.