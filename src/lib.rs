use std::fs;
use std::io;

use nix::{ioctl_none, ioctl_readwrite, ioctl_write_int};

pub const IB_IOCTL_MAGIC: u8 = 0x1b as u8;
pub const IB_IOCTL_REG_AGENT: u64 = 1;
pub const IB_IOCTL_UNREG_AGENT: u64 = 2;
pub const IB_IOCTL_EN_PKEY: u8 = 3;
pub const IB_IOCTL_REG_AGENT2: u64 = 4;

pub const SYS_INFINIBAND: &str = "/sys/class/infiniband";

#[derive(Debug, Clone)]
#[repr(C)]
#[allow(non_camel_case_types)]
pub struct ib_user_mad_reg_req {
    pub id: u32,
    pub method_mask: [u32; 4],
    pub qpn: u8,
    pub mgmt_class: u8,
    pub mgmt_class_version: u8,
    pub oui: [u8; 3],
    pub rmpp_version: u8, 
}

#[derive(Debug, Clone)]
#[repr(C)]
#[allow(non_camel_case_types)]
pub struct ib_user_mad_reg_req2 {
    pub id: u32,
    pub qpn: u32,
    pub mgmt_class: u8,
    pub mgmt_class_version: u8,
    pub res: u16,
    pub flags: u32,
    pub method_mask: [u64; 2],
    pub oui: u32,
    pub rmpp_version: u8,
    pub reserved: [u8; 3],
}

ioctl_readwrite!(ib_user_mad_register_agent, IB_IOCTL_MAGIC, IB_IOCTL_REG_AGENT, ib_user_mad_reg_req);
ioctl_readwrite!(ib_user_mad_register_agent2, IB_IOCTL_MAGIC, IB_IOCTL_REG_AGENT2, ib_user_mad_reg_req2);
ioctl_write_int!(ib_user_mad_unregister_agent, IB_IOCTL_MAGIC, IB_IOCTL_UNREG_AGENT);
ioctl_none!(ib_user_mad_enable_pkey, IB_IOCTL_MAGIC, IB_IOCTL_EN_PKEY);


pub fn get_cas_names() -> Result<Vec<String>, std::io::Error> {
    let mut cas: Vec<String> = Vec::new();

    match fs::exists(SYS_INFINIBAND) {
        Ok(r) => {
            match r {
                true => {
                    for entry in fs::read_dir(SYS_INFINIBAND)? {
                        let entry = entry?;
                        let file_name = entry.file_name().into_string().unwrap();
                        cas.push(
                            file_name
                        );
                    }
                }
                false => {
                    let err = std::io::Error::new(
                        io::ErrorKind::NotFound, 
                        io::Error::other("Directory does not exist".to_string())
                    );
                    return Err(err)
                }
            }
        }
        Err(e) => {
            let err = std::io::Error::new(io::ErrorKind::Other, e);
            return Err(err)
        }
    }


    Ok(cas)
}