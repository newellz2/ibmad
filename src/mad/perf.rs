use std::mem::MaybeUninit;

use crate::mad::helpers::{get_bitfield, set_bitfield};

macro_rules! bitfield {
    ($getter:ident, $setter:ident, $offset:expr, $width:expr, $type:ty) => {
        pub fn $getter(&self) -> $type {
            get_bitfield(&self.data, $offset, $width) as $type
        }

        pub fn $setter(&mut self, val: $type) {
            set_bitfield(&mut self.data, $offset, $width, val as u64);
        }
    };
}

#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
#[allow(non_camel_case_types)]
pub struct perf_mad {
    pub pm_key: u64,
    pub reserved: [u8; 32],
    pub data: [u8; 192],
}

#[allow(non_camel_case_types)]
impl perf_mad {
    pub fn to_bytes(&self) -> Vec<u8> {
        unsafe {
            std::slice::from_raw_parts(
                self as *const perf_mad as *const u8,
                std::mem::size_of::<perf_mad>(),
            )
            .to_vec()
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < std::mem::size_of::<perf_mad>() {
            return None;
        }
        let mut val = MaybeUninit::<perf_mad>::uninit();
        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                val.as_mut_ptr() as *mut u8,
                std::mem::size_of::<perf_mad>(),
            );
            Some(val.assume_init())
        }
    }

    pub fn pm_key(&self) -> u64 {
        u64::from_be(self.pm_key)
    }

    pub fn set_pm_key(&mut self, value: u64) {
        self.pm_key = value.to_be();
    }

    bitfield!(reserved_bits, set_reserved_bits, 0, 8, u8);
    bitfield!(port_select, set_port_select, 8, 8, u8);
    bitfield!(counter_select, set_counter_select, 16, 16, u16);
    bitfield!(counter_select2, set_counter_select2, 32, 32, u32);
    bitfield!(port_xmit_data, set_port_xmit_data, 64, 64, u64);
    bitfield!(port_rcv_data, set_port_rcv_data, 128, 64, u64);
    bitfield!(port_xmit_pkts, set_port_xmit_pkts, 192, 64, u64);
    bitfield!(port_rcv_pkts, set_port_rcv_pkts, 256, 64, u64);
    bitfield!(
        port_unicast_xmit_pkts,
        set_port_unicast_xmit_pkts,
        320,
        64,
        u64
    );
    bitfield!(
        port_unicast_rcv_pkts,
        set_port_unicast_rcv_pkts,
        384,
        64,
        u64
    );
    bitfield!(
        port_multicast_xmit_pkts,
        set_port_multicast_xmit_pkts,
        448,
        64,
        u64
    );
    bitfield!(
        port_multicast_rcv_pkts,
        set_port_multicast_rcv_pkts,
        512,
        64,
        u64
    );
    bitfield!(symbol_error_counter, set_symbol_error_counter, 576, 64, u64);
    bitfield!(
        link_error_recovery_counter,
        set_link_error_recovery_counter,
        640,
        64,
        u64
    );
    bitfield!(link_downed_counter, set_link_downed_counter, 704, 64, u64);
    bitfield!(port_rcv_errors, set_port_rcv_errors, 768, 64, u64);
    bitfield!(
        port_rcv_remote_physical_errors,
        set_port_rcv_remote_physical_errors,
        832,
        64,
        u64
    );
    bitfield!(
        port_rcv_switch_relay_errors,
        set_port_rcv_switch_relay_errors,
        896,
        64,
        u64
    );
    bitfield!(port_xmit_discards, set_port_xmit_discards, 960, 64, u64);
    bitfield!(
        port_xmit_constraint_errors,
        set_port_xmit_constraint_errors,
        1024,
        64,
        u64
    );
    bitfield!(
        port_rcv_constraint_errors,
        set_port_rcv_constraint_errors,
        1088,
        64,
        u64
    );
    bitfield!(
        local_link_integrity_errors,
        set_local_link_integrity_errors,
        1152,
        64,
        u64
    );
    bitfield!(
        excessive_buffer_overrun_errors,
        set_excessive_buffer_overrun_errors,
        1216,
        64,
        u64
    );
    bitfield!(vl15_dropped, set_vl15_dropped, 1280, 64, u64);
    bitfield!(port_xmit_wait, set_port_xmit_wait, 1344, 64, u64);
    bitfield!(qp1_dropped, set_qp1_dropped, 1408, 64, u64);
}
