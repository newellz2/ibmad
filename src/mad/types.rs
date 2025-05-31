use std::mem::MaybeUninit;

#[derive(Debug, Copy, Clone)]
#[repr(C, packed)]
#[allow(non_camel_case_types)]
pub struct ib_mad {
    pub base_version: u8,
    pub mgmt_class: u8,
    pub class_version: u8,
    pub method: u8,
    pub status: u16,
    pub hop_ptr: u8,
    pub hop_cnt: u8,
    pub tid: u64,
    pub attr_id: u16,
    pub additional_status: u16,
    pub attr_mod: u32,
    pub data: [u8; 232],
}

impl ib_mad {
    pub fn to_bytes(&self) -> Vec<u8> {
        unsafe {
            std::slice::from_raw_parts(
                self as *const ib_mad as *const u8,
                std::mem::size_of::<ib_mad>(),
            )
            .to_vec()
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < std::mem::size_of::<ib_mad>() {
            return None;
        }
        let mut val = MaybeUninit::<ib_mad>::uninit();
        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                val.as_mut_ptr() as *mut u8,
                std::mem::size_of::<ib_mad>(),
            );
            Some(val.assume_init())
        }
    }
}

#[derive(Debug, Copy, Clone)]
#[repr(C, packed)]
#[allow(non_camel_case_types)]
pub struct ib_user_mad {
    pub agent_id: u32,
    pub status: u32,
    pub timeout_ms: u32,
    pub retries: u32,
    pub length: u32,
    pub addr: ib_mad_addr,
    pub data: [u8; 256],
}

impl ib_user_mad {
    pub fn to_bytes(&self) -> Vec<u8> {
        unsafe {
            std::slice::from_raw_parts(
                self as *const ib_user_mad as *const u8,
                std::mem::size_of::<ib_user_mad>(),
            )
            .to_vec()
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < std::mem::size_of::<ib_user_mad>() {
            return None;
        }
        let mut val = MaybeUninit::<ib_user_mad>::uninit();
        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                val.as_mut_ptr() as *mut u8,
                std::mem::size_of::<ib_user_mad>(),
            );
            Some(val.assume_init())
        }
    }
}

#[derive(Debug, Copy, Clone)]
#[repr(C, packed)]
#[allow(non_camel_case_types)]
pub struct ib_mad_addr {
    pub qpn: u32,
    pub qkey: u32,
    pub lid: u16,
    pub sl: u8,
    pub path_bits: u8,
    pub grh_present: u8,
    pub gid_index: u8,
    pub hop_limit: u8,
    pub traffic_class: u8,
    pub gid: [u8; 16],
    pub flow_label: u32,
    pub pkey_index: u16,
    pub reserved: [u8; 6],
}

impl ib_mad_addr {
    pub fn to_bytes(&self) -> Vec<u8> {
        unsafe {
            std::slice::from_raw_parts(
                self as *const ib_mad_addr as *const u8,
                std::mem::size_of::<ib_mad_addr>(),
            )
            .to_vec()
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < std::mem::size_of::<ib_mad_addr>() {
            return None;
        }
        let mut val = MaybeUninit::<ib_mad_addr>::uninit();
        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                val.as_mut_ptr() as *mut u8,
                std::mem::size_of::<ib_mad_addr>(),
            );
            Some(val.assume_init())
        }
    }
}
