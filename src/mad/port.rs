use std::mem::MaybeUninit;

#[repr(C, packed)]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
#[allow(non_camel_case_types)]
pub struct port_info {
    pub mkey: u64,
    pub gid_prefix: u64,
    pub lid: u16,
    pub sm_lid: u16,
    pub cap_mask: u32,
    pub diag_code: u16,
    pub mkey_lease_period: u16,
    pub local_port: u8,
    pub link_width_enabled: u8,
    pub link_width_supported: u8,
    pub link_width_active: u8,
    pub port_state: u8,
    pub phys_state: u8,
    pub reserved: [u8; 30],
}

impl port_info {
    pub fn to_bytes(&self) -> Vec<u8> {
        unsafe {
            std::slice::from_raw_parts(
                self as *const port_info as *const u8,
                std::mem::size_of::<port_info>(),
            )
            .to_vec()
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < std::mem::size_of::<port_info>() {
            return None;
        }
        let mut val = MaybeUninit::<port_info>::uninit();
        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                val.as_mut_ptr() as *mut u8,
                std::mem::size_of::<port_info>(),
            );
            Some(val.assume_init())
        }
    }
}