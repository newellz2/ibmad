
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path;
use std::path::PathBuf;

use log;

pub const SYS_INFINIBAND: &str = "/sys/class/infiniband";


//CA
const SYS_CA_BOARD_ID: &str = "board_id";
const SYS_CA_NODE_TYPE: &str = "node_type";
const SYS_CA_FW_VERS: &str = "fw_ver";
const SYS_CA_HW_VERS: &str = "hw_rev";
const SYS_CA_TYPE: &str = "hca_type";
const SYS_CA_NODE_GUID: &str = "node_guid";
const SYS_CA_SYS_GUID: &str = "sys_image_guid";
const SYS_CA_NODE_DESC: &str = "node_desc";

//CA Port
const SYS_PORT_LMC: &str = "lid_mask_count";
const SYS_PORT_SMLID: &str = "sm_lid";
const SYS_PORT_SMSL: &str = "sm_sl";
const SYS_PORT_LID: &str ="lid";
const SYS_PORT_STATE: &str = "state"; // Logical State
const SYS_PORT_PHY_STATE: &str = "phys_state";
const SYS_PORT_CAPMASK: &str = "cap_mask";
const SYS_PORT_RATE: &str = "rate";
const SYS_PORT_GID: &str= "gids/0";
const SYS_PORT_LINK_LAYER: &str = "link_layer";

const SYS_CA_UMAD_PATH: &str = "device/infiniband_mad";
const DEV_CA_UMAD_PATH: &str = "/dev/infiniband";

const SYS_CA_PROPERTIES: [&str; 8] =  [
    SYS_CA_BOARD_ID, SYS_CA_NODE_TYPE, SYS_CA_FW_VERS,
    SYS_CA_HW_VERS, SYS_CA_TYPE, SYS_CA_NODE_GUID,
    SYS_CA_SYS_GUID, SYS_CA_NODE_DESC,
];

const SYS_CA_PORT_PROPERTIES: [&str; 10] =  [
    SYS_PORT_LMC, SYS_PORT_SMLID, SYS_PORT_SMSL,
    SYS_PORT_LID, SYS_PORT_STATE, SYS_PORT_PHY_STATE,
    SYS_PORT_CAPMASK, SYS_PORT_RATE,
    SYS_PORT_GID, SYS_PORT_LINK_LAYER
];

const SYS_CA_PORT_COUNTERS_DIR: &str = "counters";
const SYS_CA_PORT_HW_COUNTERS_DIR: &str = "hw_counters";


#[derive(Debug)]
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

#[derive(Debug)]
pub enum IbPortLinkLayerState {
    Unknown = -1,
    Nop = 0,
    Down= 1,
    Init = 2,
    Armed = 3,
    Active = 4,
    ActiveDeferred = 5,
}

#[derive(Debug)]
pub struct IbCaPort {
    pub path: String,
    pub number: u32,
    pub phy_state: IbPortPhyState,
    pub link_layer: Option<String>,
    pub rate: Option<String>,
    pub sm_lid: u32,
    pub sm_sl: u8,
    pub state: IbPortLinkLayerState,
    pub lid: u32,
    pub lmc: u32,
    pub cap_mask: u32,
    pub gid: u128,
    pub pkeys: Vec<u64>,
}


#[derive(Debug)]
pub struct IbCaDevPaths {
    pub umad_dev_path: Option<PathBuf>,
    pub issm_dev_path: Option<PathBuf>,
}

#[derive(Debug)]
pub struct IbCa {
    pub name: String,
    pub ports: Vec<IbCaPort>,
    pub board_id: Option<String>,
    pub fw_ver: Option<String>,
    pub hca_type: Option<String>,
    pub hw_rev: Option<String>,
    pub node_desc: Option<String>,
    pub node_guid: Option<String>,
    pub node_type: Option<String>,
    pub sys_image_guid: Option<String>,
    pub dev_paths: Option<IbCaDevPaths>,
}

impl IbCaPort  {
    pub fn get_counters(&self) -> Result<HashMap<String,u64>, io::Error> {
        let mut counters = HashMap::new();

        let counters_path = PathBuf::from(&self.path).join(SYS_CA_PORT_COUNTERS_DIR);
        if counters_path.exists() {
            for entry in fs::read_dir(counters_path)? {
                let entry = entry?;

                let name = entry.file_name().into_string().unwrap();

                if let Ok(data) = fs::read_to_string(entry.path()) {
                    if let Ok(v) = data.trim().parse::<u64>() {
                        counters.insert(name, v);
                    }
                }

            }
        }

        Ok(counters)

    }

    pub fn get_hw_counters(&self) -> Result<HashMap<String,u64>, io::Error> {
        let mut counters = HashMap::new();

        let counters_path = PathBuf::from(&self.path).join(SYS_CA_PORT_HW_COUNTERS_DIR);
        if counters_path.exists() {
            for entry in fs::read_dir(counters_path)? {
                let entry = entry?;

                let name = entry.file_name().into_string().unwrap();

                if let Ok(data) = fs::read_to_string(entry.path()) {
                    if let Ok(v) = data.trim().parse::<u64>() {
                        counters.insert(name, v);
                    }
                }

            }
        }

        Ok(counters)

    }

}

pub fn get_cas_names() -> Result<Vec<String>, std::io::Error> {
    log::debug!("get_cas_names called");
    let mut cas: Vec<String> = Vec::new();

    log::debug!("Reading directory: {}", SYS_INFINIBAND);
    match fs::exists(SYS_INFINIBAND) {
        Ok(r) => {
            match r {
                true => {
                    log::debug!("{} directory exists", SYS_INFINIBAND);
                    for entry in fs::read_dir(SYS_INFINIBAND)? {
                        let entry = entry?;
                        let file_name = entry.file_name().into_string().unwrap();
                        log::trace!("Found entry, path={:?} filename={}", entry.path(), file_name);
                        cas.push(
                            file_name
                        );
                    }
                }
                false => {
                    log::debug!("Directory '{}' does not exist", SYS_INFINIBAND);
                    let err = std::io::Error::new(
                        io::ErrorKind::NotFound, 
                        io::Error::other("Directory does not exist".to_string())
                    );
                    return Err(err)
                }
            }
        }
        Err(e) => {
            log::debug!("Error checking if {} exists: {}", SYS_INFINIBAND, e);
            let err = std::io::Error::new(io::ErrorKind::Other, e);
            return Err(err)
        }
    }

    log::debug!("get_cas_names successfully returned {} entries", cas.len());
    Ok(cas)
}


pub fn get_ib_ports_info(path: &path::PathBuf) -> Result<Vec<IbCaPort>, io::Error> {

    let mut ports: Vec<IbCaPort> = Vec::new();

    let ports_path = path.join(
        PathBuf::from("ports")
    );

    match fs::exists(&ports_path) {
        Ok(r) => {
            match r {
                true => {
                    for entry in fs::read_dir(&ports_path)? {

                        let entry: fs::DirEntry = entry?;
                        let file_name = entry.file_name().into_string().unwrap();

                        log::trace!("get_ib_ports_info - Found port, Path: {:?}, Filename: {}", entry.path(), file_name);

                        let mut port = IbCaPort{
                            path: entry.path().to_str().unwrap().to_owned(),
                            number: 0,
                            phy_state: IbPortPhyState::Unknown,
                            link_layer: None,
                            rate: None,
                            sm_lid: 0,
                            sm_sl: 0,
                            state: IbPortLinkLayerState::Unknown,
                            lid: 0,
                            lmc: 0,
                            cap_mask: 0,
                            gid: 0,
                            pkeys: Vec::new(),
                        };

                        match file_name.parse::<u32>() {
                            Ok(num) => {
                                log::trace!("get_ib_ports_info - Parsed port number: {}", num);
                                port.number = num
                            },
                            Err(e) => {
                                log::trace!("get_ib_ports_info - Failed to parse port number: {:?}", e);
                            }
                        }

                        let phy_state_path = entry.path().join(SYS_PORT_PHY_STATE);
                        log::trace!("get_ib_ports_info - Path: {:?}, phys_state Path: '{:?}'", entry.path(), phy_state_path);

                        let data = fs::read(phy_state_path)?;
                        let phy_state_str = String::from_utf8_lossy(&data);
                        let phy_state_str = phy_state_str.trim();

                        log::trace!("get_ib_ports_info - Path: {:?}, PhyState File Value: '{}'", entry.path(), phy_state_str);

                        match phy_state_str.split(':').next().unwrap_or("-1") {
                            "5" => {
                                log::trace!("get_ib_ports_info - Port '{}' has LinkUp state.", phy_state_str);
                                port.phy_state = IbPortPhyState::LinkUp;
                            }
                            _ => {
                                log::trace!("get_ib_ports_info - Port '{}' has unknown state.", phy_state_str);
                                port.phy_state = IbPortPhyState::Unknown;
                            }
                        }

                        for prop in SYS_CA_PORT_PROPERTIES.iter() {
                            let file_path = entry.path().join(prop);
                            if file_path.exists() {
                                if let Ok(data) = fs::read_to_string(&file_path) {
                                    match *prop {
                                        SYS_PORT_LINK_LAYER =>  port.link_layer = Some((&data.trim()).to_string()),
                                        SYS_PORT_RATE =>  port.rate = Some((&data.trim()).to_string()),
                                        SYS_PORT_SMLID => port.sm_lid = u32::from_str_radix(&data[2..].trim(), 16).unwrap(),
                                        SYS_PORT_SMSL => port.sm_sl = u8::from_str_radix(&data.trim(), 16).unwrap(),
                                        SYS_PORT_LID => port.lid = u32::from_str_radix(&data[2..].trim(), 16).unwrap(),
                                        SYS_PORT_LMC => port.lmc = u32::from_str_radix(&data.trim(), 16).unwrap(),
                                        SYS_PORT_CAPMASK => port.cap_mask = u32::from_str_radix(&data[2..].trim(), 16).unwrap(),
                                        SYS_PORT_GID => port.gid = u128::from_str_radix( &data.replace(":", "").trim(), 16).unwrap(),
                                        _ => {}, // Do Nothing
                                    }
                                }
                            }
                        };

                        let state_path = entry.path().join(SYS_PORT_STATE);
                        if state_path.exists() {
                            if let Ok(data) = fs::read_to_string(state_path) {
                                let state_str = data.trim();
                                match state_str.split(':').next().unwrap_or("-1") {
                                    "0" => port.state = IbPortLinkLayerState::Nop,
                                    "1" => port.state = IbPortLinkLayerState::Down,
                                    "2" => port.state = IbPortLinkLayerState::Init,
                                    "3" => port.state = IbPortLinkLayerState::Armed,
                                    "4" => port.state = IbPortLinkLayerState::Active,
                                    "5" => port.state = IbPortLinkLayerState::ActiveDeferred,
                                    _ => port.state = IbPortLinkLayerState::Unknown,
                                }
                            }
                        }

                        let lid_path = entry.path().join(SYS_PORT_LID);
                        if lid_path.exists() {
                            if let Ok(data) = fs::read_to_string(lid_path) {
                                if let Ok(v) = data.trim().parse::<u32>() {
                                    port.lid = v;
                                }
                            }
                        }

                        let pkeys_path = entry.path().join("pkeys");
                        if pkeys_path.exists() {
                            for pk_entry in fs::read_dir(pkeys_path)? {
                                let pk_entry = pk_entry?;
                                if let Ok(data) = fs::read_to_string(pk_entry.path()) {
                                    let val_str = data.trim().trim_start_matches("0x");
                                    if let Ok(v) = u64::from_str_radix(val_str, 16).or_else(|_| val_str.parse::<u64>()) {
                                        port.pkeys.push(v);
                                    }
                                }
                            }
                        }

                        log::trace!("get_ib_ports_info - Adding port to return vec: {:?}", port);
                        ports.push(port);
                    }
                }
                false => {
                    log::trace!("get_ib_ports_info - Failed to find port path: {:?}", &ports_path);
                }
            }
        }

        Err(e) => {
            log::debug!("get_ib_ports_info - Error checking if {} exists: {}", SYS_INFINIBAND, e);
            let err = std::io::Error::new(io::ErrorKind::Other, e);
            return Err(err)
        }
    }

    Ok(ports)
    
}

pub fn get_ca_dev_paths(path: &path::PathBuf) -> Option<IbCaDevPaths> {
    let mut ib_ca_dev_paths = IbCaDevPaths {
        umad_dev_path: None,
        issm_dev_path: None,
    };

    let sys_path = path.join(SYS_CA_UMAD_PATH);

    log::debug!("get_ca_dev_paths - Checking sys path {:?}", sys_path);
    match sys_path.exists() {
        true =>{
            for entry in fs::read_dir(sys_path).ok()? {
                let entry = entry.ok()?;
                match entry.file_name().to_str() {
                    Some(file_name) => {

                        let file_path = path::PathBuf::from(DEV_CA_UMAD_PATH).join(file_name);

                        log::debug!("get_ca_dev_paths - Checking for device path: {:?}", file_path);
                        if file_name.starts_with("umad") {
                            if file_path.exists() {
                                log::debug!("get_ca_dev_paths - Found device path :{:?}", file_path);
                                ib_ca_dev_paths.umad_dev_path = Some(file_path);
                            }

                        } else if file_name.starts_with("issm") {
                            if file_path.exists() {
                                log::debug!("get_ca_dev_paths - Found device path: {:?}", file_path);
                                ib_ca_dev_paths.issm_dev_path = Some(file_path);
                            }

                        }
                    }
                    _ => {}, // Do Nothing

                }

            }
        },
        false => {
            log::debug!("get_ca_dev_paths - sys path '{:?}' does not exist.", sys_path);
        },
    }
    Some(ib_ca_dev_paths)
}

pub fn get_ca (hca_name: &str) -> Result<IbCa, std::io::Error> {
    let hca_path = path::PathBuf::from(SYS_INFINIBAND).join(hca_name);

    log::debug!("get_ca - hca_path: {:?}", hca_path);

    match hca_path.exists(){
        true => {
            let ports = get_ib_ports_info(&hca_path)?;
            log::trace!("get_ca - get_ib_ports_info result:{:?}", ports);

            let mut ib_ca = IbCa {
                name: hca_name.to_owned(),
                board_id: None,
                fw_ver: None,
                hca_type: None,
                hw_rev: None,
                node_desc: None,
                node_guid: None,
                node_type: None,
                sys_image_guid: None,
                dev_paths: get_ca_dev_paths(&hca_path),
                ports,
            };

            for prop in SYS_CA_PROPERTIES.iter() {
                let file_path = hca_path.join(prop);
                if file_path.exists() {
                    if let Ok(data) = fs::read_to_string(&file_path) {
                        match *prop {
                            SYS_CA_BOARD_ID => ib_ca.board_id = Some(data.trim().to_owned()),
                            SYS_CA_TYPE=> ib_ca.hca_type = Some(data.trim().to_owned()),
                            SYS_CA_FW_VERS => ib_ca.fw_ver = Some(data.trim().to_owned()),
                            SYS_CA_HW_VERS => ib_ca.hw_rev = Some(data.trim().to_owned()),
                            SYS_CA_NODE_GUID => ib_ca.node_guid = Some(data.trim().to_owned()),
                            SYS_CA_NODE_TYPE => ib_ca.node_type = Some(data.trim().to_owned()),
                            SYS_CA_SYS_GUID => ib_ca.sys_image_guid = Some(data.trim().to_owned()),
                            SYS_CA_NODE_DESC => ib_ca.node_desc = Some(data.trim().to_owned()),
                            _ => {}, //Do Nothing
                        }
                    }
                }
            };

            Ok(ib_ca)
        },
        false => {
            log::debug!("Directory '{:?}' does not exist", hca_path);
            let err = std::io::Error::new(
                io::ErrorKind::NotFound, 
                io::Error::other("Directory does not exist".to_string())
            );
            return Err(err)
        }
    }
}

pub fn get_cas() -> Result<Vec<IbCa>, std::io::Error> {
    log::debug!("get_linkup_cas_names called");
    let mut cas: Vec<IbCa> = Vec::new();

    log::debug!("Reading directory: {}", SYS_INFINIBAND);
    match fs::exists(SYS_INFINIBAND) {
        Ok(r) => {
            match r {
                true => {
                    log::debug!("{} directory exists", SYS_INFINIBAND);
                    for entry in fs::read_dir(SYS_INFINIBAND)? {
                        let entry = entry?;
                        let file_name = entry.file_name().into_string().unwrap();

                        let ib_ca = get_ca(&file_name)?;
                        log::trace!("get_cas - Found entry, path={:?} filename={}", entry.path(), file_name);


                        log::trace!("get_cas - adding ca to return vec: {:?}", ib_ca);
                        cas.push(ib_ca);
                    }
                }
                false => {
                    log::debug!("Directory '{}' does not exist", SYS_INFINIBAND);
                    let err = std::io::Error::new(
                        io::ErrorKind::NotFound, 
                        io::Error::other("Directory does not exist".to_string())
                    );
                    return Err(err)
                }
            }
        }
        Err(e) => {
            log::debug!("Error checking if {} exists: {}", SYS_INFINIBAND, e);
            let err = std::io::Error::new(io::ErrorKind::Other, e);
            return Err(err)
        }
    }

    log::debug!("get_linkup_cas_names successfully returned {} entries", cas.len());
    Ok(cas)
}