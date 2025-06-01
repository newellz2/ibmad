#[derive(Debug, Copy, Clone)]
#[repr(C, packed)]
#[allow(non_camel_case_types)]
pub struct perf_mad {
    pub pm_key: u64,
    pub reserved: [u8; 32],
    pub data: [u8; 192],
}

