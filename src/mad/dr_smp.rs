use std::mem::MaybeUninit;

#[derive(Debug, Copy, Clone)]
#[repr(C, packed)]
#[allow(non_camel_case_types)]
pub struct dr_smp_mad {
    pub m_key: u64,
    pub drslid: u16,
    pub drdlid: u16,
    pub reserved: [u8; 28],
    pub attr_layout: [u8; 64],
    pub initial_path: [u8; 64],
    pub return_path: [u8; 64],
}

impl dr_smp_mad {
    pub fn to_bytes(&self) -> Vec<u8> {
        unsafe {
            std::slice::from_raw_parts(
                self as *const dr_smp_mad as *const u8,
                std::mem::size_of::<dr_smp_mad>(),
            )
            .to_vec()
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < std::mem::size_of::<dr_smp_mad>() {
            return None;
        }
        let mut val = MaybeUninit::<dr_smp_mad>::uninit();
        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                val.as_mut_ptr() as *mut u8,
                std::mem::size_of::<dr_smp_mad>(),
            );
            Some(val.assume_init())
        }
    }
}
