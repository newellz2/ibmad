use std::mem::MaybeUninit;

#[repr(C, packed)]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
#[allow(non_camel_case_types)]
pub struct node_info {
    pub base_version: u8,
    pub class_version: u8,
    pub node_type: u8,
    pub nports: u8,
    pub system_guid: u64,
    pub node_guid: u64,
    pub port_guid: u64,
    pub partition_cap: u16,
    pub device_id: u16,
    pub revision: u32,
    pub local_port: u8,
    pub vendor_id: [u8; 3],
    pub reserved: [u8; 24],
}

impl node_info {
    pub fn to_bytes(&self) -> Vec<u8> {
        unsafe {
            std::slice::from_raw_parts(
                self as *const node_info as *const u8,
                std::mem::size_of::<node_info>(),
            )
            .to_vec()
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < std::mem::size_of::<node_info>() {
            return None;
        }
        let mut val = MaybeUninit::<node_info>::uninit();
        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                val.as_mut_ptr() as *mut u8,
                std::mem::size_of::<node_info>(),
            );
            Some(val.assume_init())
        }
    }
}