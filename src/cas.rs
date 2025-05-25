
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path;
use std::path::PathBuf;

use log;

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
    Sleep = 1,
    Polling = 2,
    Disabled = 3,
    PortConfigurationTraining = 4,
    LinkUp = 5,
    LinkErrorRecovery = 6,
    PhyTest = 7,
}

#[derive(Debug, Default, Clone)]
pub struct IbPortCounters {
    // Using u64 as counters are typically large unsigned integers
    // Using Option as some counters might not be present
    pub port_xmit_data: Option<u64>,
    pub port_rcv_data: Option<u64>,
    pub port_xmit_packets: Option<u64>,
    pub port_rcv_packets: Option<u64>,
    // Add other common counters
}

#[derive(Debug, Default, Clone)]
pub struct IbPortHwCounters {
    // Similar to IbPortCounters, specific counter names would be needed
    // For now, let's assume a couple of examples
    pub hw_verrors: Option<u64>,
    pub hw_recv_errors: Option<u64>,
    // Add other common HW counters
}


#[derive(Debug)]
pub struct IbPort {
    pub number: u32,
    pub phy_state: IbPortPhyState,
    pub link_layer: Option<String>,
    pub rate: Option<String>,
    pub sm_lid: u32,
    pub sm_sl: u8,
    pub state: IbPortLinkLayerState,
    pub lid: u32,
    pub counters: Option<HashMap<String,u64>>,
    pub hw_counters: Option<HashMap<String,u64>>,
    pub pkeys: Vec<u64>,
}

#[derive(Debug)]
pub struct IbCa {
    name: String,
    ports: Vec<IbPort>,
}

pub fn get_cas_names() -> Result<Vec<String>, std::io::Error> {
    log::debug!("get_cas_names called");
    let mut cas: Vec<String> = Vec::new();

    log::debug!("Reading directory: {}", crate::SYS_INFINIBAND);
    match fs::exists(crate::SYS_INFINIBAND) {
        Ok(r) => {
            match r {
                true => {
                    log::debug!("{} directory exists", crate::SYS_INFINIBAND);
                    for entry in fs::read_dir(crate::SYS_INFINIBAND)? {
                        let entry = entry?;
                        let file_name = entry.file_name().into_string().unwrap();
                        log::trace!("Found entry, path={:?} filename={}", entry.path(), file_name);
                        cas.push(
                            file_name
                        );
                    }
                }
                false => {
                    log:: error!("Directory '{}' does not exist", crate::SYS_INFINIBAND);
                    let err = std::io::Error::new(
                        io::ErrorKind::NotFound, 
                        io::Error::other("Directory does not exist".to_string())
                    );
                    return Err(err)
                }
            }
        }
        Err(e) => {
            log::error!("Error checking if {} exists: {}", crate::SYS_INFINIBAND, e);
            let err = std::io::Error::new(io::ErrorKind::Other, e);
            return Err(err)
        }
    }

    log::debug!("get_cas_names successfully returned {} entries", cas.len());
    Ok(cas)
}

pub fn get_ib_ports_info(path: &path::PathBuf) -> Result<Vec<IbPort>, io::Error> {

    let mut ports: Vec<IbPort> = Vec::new();

    let ports_path = path.join(
        PathBuf::from("ports")
    );

    match fs::exists(&ports_path) {
        Ok(r) => {
            match r {
                true => {
                    for entry in fs::read_dir(&ports_path)? {
                        let mut port = IbPort{
                            number: 0,
                            phy_state: IbPortPhyState::Unknown,
                            link_layer: None,
                            rate: None,
                            sm_lid: 0,
                            sm_sl: 0,
                            state: IbPortLinkLayerState::Unknown,
                            lid: 0,
                            counters: None,
                            hw_counters: None,
                            pkeys: Vec::new(),
                        };

                        let entry = entry?;
                        let file_name = entry.file_name().into_string().unwrap();

                        log::trace!("get_ib_ports_info - Found port, Path: {:?}, Filename: {}", entry.path(), file_name);

                        match file_name.parse::<u32>() {
                            Ok(num) => {
                                log::trace!("get_ib_ports_info - Parsed port number: {}", num);
                                port.number = num
                            },
                            Err(e) => {
                                log::trace!("get_ib_ports_info - Failed to parse port number: {:?}", e);
                            }
                        }

                        let phy_state_path = entry.path().join("phys_state");
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

                        let link_layer_path = entry.path().join("link_layer");
                        if link_layer_path.exists() {
                            if let Ok(data) = fs::read_to_string(link_layer_path) {
                                port.link_layer = Some(data.trim().to_string());
                            }
                        }

                        let rate_path = entry.path().join("rate");
                        if rate_path.exists() {
                            if let Ok(data) = fs::read_to_string(rate_path) {
                                port.rate = Some(data.trim().to_string());
                            }
                        }

                        let sm_lid_path = entry.path().join("sm_lid");
                        if sm_lid_path.exists() {
                            if let Ok(data) = fs::read_to_string(sm_lid_path) {
                                if let Ok(v) = data.trim().parse::<u32>() {
                                    port.sm_lid = v;
                                }
                            }
                        }

                        let sm_sl_path = entry.path().join("sm_sl");
                        if sm_sl_path.exists() {
                            if let Ok(data) = fs::read_to_string(sm_sl_path) {
                                if let Ok(v) = data.trim().parse::<u8>() {
                                    port.sm_sl = v;
                                }
                            }
                        }

                        let state_path = entry.path().join("state");
                        if state_path.exists() {
                            if let Ok(data) = fs::read_to_string(state_path) {
                                let state_str = data.trim();
                                match state_str.split(':').next().unwrap_or("-1") {
                                    "5" => port.state = IbPortLinkLayerState::LinkUp,
                                    _ => port.state = IbPortLinkLayerState::Unknown,
                                }
                            }
                        }

                        let lid_path = entry.path().join("lid");
                        if lid_path.exists() {
                            if let Ok(data) = fs::read_to_string(lid_path) {
                                if let Ok(v) = data.trim().parse::<u32>() {
                                    port.lid = v;
                                }
                            }
                        }

                        let counters_path = entry.path().join("counters");
                        if counters_path.exists() {
                            let mut counters = HashMap::new();
                            for ctr_entry in fs::read_dir(counters_path)? {
                                let ctr_entry = ctr_entry?;
                                let name = ctr_entry.file_name().into_string().unwrap();
                                if let Ok(data) = fs::read_to_string(ctr_entry.path()) {
                                    if let Ok(v) = data.trim().parse::<u64>() {
                                        counters.insert(name, v);
                                    }
                                }
                            }
                            port.counters = Some(counters);
                        }

                        let hw_counters_path = entry.path().join("hw_counters");
                        if hw_counters_path.exists() {
                            let mut hw_counters = HashMap::new();
                            for ctr_entry in fs::read_dir(hw_counters_path)? {
                                let ctr_entry = ctr_entry?;
                                let name = ctr_entry.file_name().into_string().unwrap();
                                if let Ok(data) = fs::read_to_string(ctr_entry.path()) {
                                    if let Ok(v) = data.trim().parse::<u64>() {
                                        hw_counters.insert(name, v);
                                    }
                                }
                            }
                            port.hw_counters = Some(hw_counters);
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
            log::error!("get_ib_ports_info - Error checking if {} exists: {}", crate::SYS_INFINIBAND, e);
            let err = std::io::Error::new(io::ErrorKind::Other, e);
            return Err(err)
        }
    }

    Ok(ports)
    
}

pub fn get_cas() -> Result<Vec<IbCa>, std::io::Error> {
    log::debug!("get_linkup_cas_names called");
    let mut cas: Vec<IbCa> = Vec::new();

    log::debug!("Reading directory: {}", crate::SYS_INFINIBAND);
    match fs::exists(crate::SYS_INFINIBAND) {
        Ok(r) => {
            match r {
                true => {
                    log::debug!("{} directory exists", crate::SYS_INFINIBAND);
                    for entry in fs::read_dir(crate::SYS_INFINIBAND)? {
                        let entry = entry?;
                        let file_name = entry.file_name().into_string().unwrap();
                        log::trace!("get_cas - Found entry, path={:?} filename={}", entry.path(), file_name);

                        let ib_ca = IbCa {
                            name: file_name,
                            ports: Vec::new(),
                        };
                        let r = get_ib_ports_info(&entry.path());

                        log::trace!("get_cas - get_ib_ports_info result:{:?}", r);

                        if let Ok(ports) = r {
                            let ib_ca = IbCa {
                                name: file_name,
                                ports,
                            };
                            log::trace!("get_cas - adding ca to return vec: {:?}", ib_ca);
                            cas.push(ib_ca);
                        }
                    }
                }
                false => {
                    log:: error!("Directory '{}' does not exist", crate::SYS_INFINIBAND);
                    let err = std::io::Error::new(
                        io::ErrorKind::NotFound, 
                        io::Error::other("Directory does not exist".to_string())
                    );
                    return Err(err)
                }
            }
        }
        Err(e) => {
            log::error!("Error checking if {} exists: {}", crate::SYS_INFINIBAND, e);
            let err = std::io::Error::new(io::ErrorKind::Other, e);
            return Err(err)
        }
    }

    log::debug!("get_linkup_cas_names successfully returned {} entries", cas.len());
    Ok(cas)
}