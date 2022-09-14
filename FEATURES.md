# Supported Features
It is difficult to implement all the features of an EtherCAT master. It is also difficult to test with supported slaves. This section describes the features that are supported or planned to be supported and, conversely, those that are not.

Please refer to the ETG1500 for more information on the features listed below.

#### Basic Features
- (yes) Service Commands
- (no) IRQ field in datagram
- (yes) Slave with Device Emulation
- (yes) EtherCAT State Machine
- (no) VLAN
- (yes) EtherCAT Frame Types
- (no) UDP Frame Types
#### Error Detections
- (yes) Checking Working Counter
- (planned) Checking AL Status Code and EtherCAT State
- (planned) Setting SM Watchdog
- (no) Checking PDO State
- (planned) Checking Lost Slaves
- (planned) Checking Lost Frames
- (planned) Checking Error Counter in Slaves
- (no) Checking Sync Error Flag(0x1C33:20)
#### Process Data Exchenge
- (planned) Cyclic PDO
- (no) Cyclic PDO using LRD and LWR
- (no) Multiple Tasks
- (no) Frame repetition
#### Network Configuration
- (planned) Online SII Scanning
- (planned) Reading ENI
- (planned) Compare Network configuration
- (no) Explicit Device Identification
- (planned) Station Alias Addressing
- (yes) Reading SII
- (planned) Writing SII
#### Mailbox Features
- (yes) Mailbox
- (yes) Mailbox Resilient Layer
- (no) Multiple Mailbox Channels
- (planned) Mailbox Polling in OPRATIONAL state
  - (planned) SDO Emergency Message
  - (planned) Intermediary for slave-to-slave cmmunication
#### CoE
- (yes) CoE
  - (yes) SDO Up/Donwload
    - (yes) SDO Download Normal Request
    - (no) SDO Download Expedited Request
    - (yes) SDO Download Response
    - (yes) SDO Upload Request
    - (yes) SDO Upload Normal Response
    - (yes) SDO Upload Expedited Response
    - (yes) SDO Abort Response
  - (no) Segmented Transfer
  - (planned) Complete Access
  - (planned) SDO info service
  - (planned) Emergency Message
#### EoE
- (no) EoE
#### SoE
- (no) SoE
#### AoE
- (no) AoE
#### FoE
- (planned) FoE
- (planned) Boot State
#### Synchronization with Distributed Clocks
- (planned) DC Support
- (planned) Continous Propagation Delay compemsation
- (planned) Sync Window monitoring
#### Slave-to-Slave Communication
- (planned) via Master
#### Master Information
- (no) Master Object Dictionary
#### FP Cable Redundancy
- (no) Cable redundancy
- (no) Hot Connect

### Other Slave Options
- (no) UseLrdLwr
- (planned) SM:OpOnly
- (no) SeparateSu
- (no) SeparateFrame
- (no) FrameRepeatSupport
- (no) AssignToPdi
- (planned) InitCmd
- (no) UnknownFRMW
- (no) Unknown64Bit
- (no) Reg0108
- (no) Reg0400
- (no) Reg0410
- (no) Reg0420
- (no) StateMachine:Behavior