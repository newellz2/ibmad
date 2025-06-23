#[derive(Debug, Clone, PartialEq)]
pub enum IbPortPhyState {
    Sleep = 1,
    Polling = 2,
    Disabled = 3,
    PortConfigurationTraining = 4,
    LinkUp = 5,
    LinkErrorRecovery = 6,
    PhyTest = 7,
}

impl TryFrom<u8> for IbPortPhyState {
    type Error = (); // You could define a more specific error type here if needed

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(IbPortPhyState::Sleep),
            2 => Ok(IbPortPhyState::Polling),
            3 => Ok(IbPortPhyState::Disabled),
            4 => Ok(IbPortPhyState::PortConfigurationTraining),
            5 => Ok(IbPortPhyState::LinkUp),
            6 => Ok(IbPortPhyState::LinkErrorRecovery),
            7 => Ok(IbPortPhyState::PhyTest),
            _ => Err(()), // Return an error for unknown integer values
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum IbPortLinkLayerState {
    Nop = 0,
    Down= 1,
    Init = 2,
    Armed = 3,
    Active = 4,
    ActiveDeferred = 5,
}

impl TryFrom<u8> for IbPortLinkLayerState {
    type Error = (); // You could define a more specific error type here if needed

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(IbPortLinkLayerState::Nop),
            1 => Ok(IbPortLinkLayerState::Down),
            2 => Ok(IbPortLinkLayerState::Init),
            3 => Ok(IbPortLinkLayerState::Armed),
            4 => Ok(IbPortLinkLayerState::Active),
            5 => Ok(IbPortLinkLayerState::ActiveDeferred),
            _ => Err(()), // Return an error for unknown integer values
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum IbNodeType {
	CA = 1,
	Switch = 2,
	Router = 3,
	Rnic = 4,
}

impl TryFrom<u8> for IbNodeType {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(IbNodeType::CA),
            2 => Ok(IbNodeType::Switch),
            3 => Ok(IbNodeType::Router),
            4 => Ok(IbNodeType::Rnic),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone)]
pub enum MadClasses {
    DirecteRoute = 0x81,
}

#[derive(Debug, Clone)]
pub enum SmiAttrID {
    NodeDesc = 0x10,
    NodeInfo = 0x11,
    PortInfo = 0x15,
}