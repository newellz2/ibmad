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
#[derive(Clone, Copy, Debug)]
#[allow(non_camel_case_types)]
pub struct port_info {
    pub data: [u8; 64],
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

    // Bit Fields
    bitfield!(m_key,                           set_mkey,                              0, 64, u64);
    bitfield!(gid_prefix,                      set_gid_prefix,                       64, 64, u64);
    bitfield!(lid,                             set_lid,                             128, 16, u16);
    bitfield!(master_sm_lid,                   set_master_sm_lid,                   144, 16, u16);
    bitfield!(capability_mask,                 set_capability_mask,                 160, 32, u32);
    bitfield!(diag_code,                       set_diag_code,                       192, 16, u16);
    bitfield!(m_key_lease_period,              set_m_key_lease_period,              208, 16, u16);
    bitfield!(local_portnum,                   set_local_portnum,                   224,  8, u8 );
    bitfield!(link_width_enabled,              set_link_width_enabled,              232,  8, u8 );
    bitfield!(link_width_supported,            set_link_width_supported,            240,  8, u8 );
    bitfield!(link_width_active,               set_link_width_active,               248,  8, u8 );
    bitfield!(link_speed_supported,            set_link_speed_supported,            256,  4, u8 );
    bitfield!(port_state,                      set_port_state,                      260,  4, u8 );
    bitfield!(port_physical_state,             set_port_physical_state,             264,  4, u8 );
    bitfield!(link_down_default_state,         set_link_down_default_state,         268,  4, u8 );
    bitfield!(m_key_protect_bits,              set_m_key_protect_bits,              272,  2, u8 );
    bitfield!(m_key_protect_bits_ext,          set_m_key_protect_bits_ext,          274,  3, u8 );
    bitfield!(lmc,                             set_lmc,                             277,  3, u8 );
    bitfield!(link_speed_active,               set_link_speed_active,               280,  4, u8 );
    bitfield!(link_speed_enabled,              set_link_speed_enabled,              284,  4, u8 );
    bitfield!(neighbor_mtu,                    set_neighbor_mtu,                    288,  4, u8 );
    bitfield!(master_sm_sl,                    set_master_sm_sl,                    292,  4, u8 );
    bitfield!(vl_cap,                          set_vl_cap,                          296,  4, u8 );
    bitfield!(init_type,                       set_init_type,                       300,  4, u8 );
    bitfield!(vl_high_limit,                   set_vl_high_limit,                   304,  8, u8 );
    bitfield!(vl_arbitration_high_cap,         set_vl_arbitration_high_cap,         312,  8, u8 );
    bitfield!(vl_arbitration_low_cap,          set_vl_arbitration_low_cap,          320,  8, u8 );
    bitfield!(init_type_reply,                 set_init_type_reply,                 328,  4, u8 );
    bitfield!(mtu_cap,                         set_mtu_cap,                         332,  4, u8 );
    bitfield!(vl_stall_count,                  set_vl_stall_count,                  336,  3, u8 );
    bitfield!(hoq_life,                        set_hoq_life,                        339,  5, u8 );
    bitfield!(operational_vls,                 set_operational_vls,                 344,  4, u8 );
    bitfield!(partition_enforcement_inbound,   set_partition_enforcement_inbound,   348,  1, u8 );
    bitfield!(partition_enforcement_outbound,  set_partition_enforcement_outbound,  349,  1, u8 );
    bitfield!(filter_raw_inbound,              set_filter_raw_inbound,              350,  1, u8 );
    bitfield!(filter_raw_outbound,             set_filter_raw_outbound,             351,  1, u8 );
    bitfield!(m_key_violations,                set_m_key_violations,                352, 16, u16);
    bitfield!(p_key_violations,                set_p_key_violations,                368, 16, u16);
    bitfield!(q_key_violations,                set_q_key_violations,                384, 16, u16);
    bitfield!(guid_cap,                        set_guid_cap,                        400,  8, u8 );
    bitfield!(client_register,                 set_client_register,                 408,  1, u8 );
    bitfield!(multicast_pkey_trap_suppression, set_multicast_pkey_trap_suppression, 409,  2, u8 );
    bitfield!(subnet_timeout,                  set_subnet_timeout,                  411,  5, u8 );
    bitfield!(partition_top,                   set_partition_top,                   416,  2, u8 );
    bitfield!(enhanced_qos_arbiter_enabled,    set_enhanced_qos_arbiter_enabled,    417,  1, u8 );
    bitfield!(resp_time_value,                 set_resp_time_value,                 419,  5, u8 );
    bitfield!(local_phy_errors,                set_local_phy_errors,                424,  4, u8 );
    bitfield!(overrun_errors,                  set_overrun_errors,                  428,  4, u8 );
    bitfield!(max_credit_hint,                 set_max_credit_hint,                 432, 16, u16);
    bitfield!(link_round_trip_latency,         set_link_round_trip_latency,         456, 24, u32);
    bitfield!(capability_mask2,                set_capability_mask2,                480, 16, u16);
    bitfield!(link_speed_ext_active,           set_link_speed_ext_active,           496,  4, u8 );
    bitfield!(link_speed_ext_supported,        set_link_speed_ext_supported,        500,  4, u8 );
    bitfield!(link_speed_ext_enabled,          set_link_speed_ext_enabled,          504,  5, u8 );
}