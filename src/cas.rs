
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
pub struct IbPort {
    number: u32,
    phy_state: IbPortPhyState,
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

                        let data  = fs::read(phy_state_path)?;
                        let phy_state_str = String::from_utf8_lossy(&data);
                        let phy_state_str = phy_state_str.trim();

                        log::trace!("get_ib_ports_info - Path: {:?}, PhyState File Value: '{}'", entry.path(), phy_state_str.trim());

                        match phy_state_str.split(':').next().unwrap_or("-1") {
                            "5" => {
                                log::trace!("get_ib_ports_info - Port '{}', has LinkUp state.", phy_state_str);
                                port.phy_state = IbPortPhyState::LinkUp;    
                            }
                            _ => {
                                log::trace!("get_ib_ports_info - Port '{}', has unkown state.",phy_state_str);
                                port.phy_state = IbPortPhyState::Unknown;
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

                        log::trace!("get_cas - adding ca to return vec: {:?}", ib_ca);
                        cas.push(ib_ca);
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