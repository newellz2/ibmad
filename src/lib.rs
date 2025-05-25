use nix::{ioctl_none, ioctl_readwrite, ioctl_write_int};

pub const IB_IOCTL_MAGIC: u8 = 0x1b as u8;
pub const IB_IOCTL_REG_AGENT: u64 = 1;
pub const IB_IOCTL_UNREG_AGENT: u64 = 2;
pub const IB_IOCTL_EN_PKEY: u8 = 3;

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
    pub qpn: u8,
    pub mgmt_class: u8,
    pub mgmt_class_version: u8,
    pub res: u16,
    pub flags: u32,
    pub method_mask: [u64; 2],
    pub oui: u32,
    pub rmpp_version: u8,
    pub reserved: [u8; 3],
}


ioctl_readwrite!(ib_register_agent, IB_IOCTL_MAGIC, IB_IOCTL_REG_AGENT, ib_user_mad_reg_req);
ioctl_readwrite!(ib_register_agent2, IB_IOCTL_MAGIC, IB_IOCTL_REG_AGENT, ib_user_mad_reg_req2);
ioctl_write_int!(ib_unregister_agent, IB_IOCTL_MAGIC, IB_IOCTL_UNREG_AGENT);
ioctl_none!(ib_enable_pkey, IB_IOCTL_MAGIC, IB_IOCTL_EN_PKEY);