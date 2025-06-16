#[derive(Debug, Clone)]
pub enum IbPortPhyState {
    Unknown = -1,
    Sleep = 1,
    Polling = 2,
    Disabled = 3,
    PortConfigurationTraining = 4,
    LinkUp = 5,
    LinkErrorRecovery = 6,
    PhyTest = 7,
}

#[derive(Debug, Clone)]
pub enum IbPortLinkLayerState {
    Unknown = -1,
    Nop = 0,
    Down= 1,
    Init = 2,
    Armed = 3,
    Active = 4,
    ActiveDeferred = 5,
}

#[derive(Debug, Clone)]
pub enum IbNodeType {
	CA = 1,
	Switch = 2,
	Router = 3,
	Rnic = 4,
}

#[derive(Debug, Clone)]
pub enum MadClasses {
    DirecteRoute = 0x81,
}

#[derive(Debug, Clone)]
pub enum SmiAttrID {
    NodeInfo = 0x11,
    PortInfo = 0x15,
}