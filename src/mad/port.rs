use std::mem::MaybeUninit;


fn extract_bitfield(data: &[u8], bit_offset: usize, width: usize) -> u64 {

    assert!((1..=64).contains(&width), "width must be 1-64");

    let start_byte = bit_offset / 8;
    let bit_in_first = bit_offset % 8;
    let needed_bytes = (bit_in_first + width + 7) / 8;
    let end_byte = start_byte + needed_bytes;

    let mut tmp = 0u128;
    for &b in &data[start_byte..start_byte + needed_bytes] {
        tmp = (tmp << 8) | b as u128;
    }

    assert!(start_byte + needed_bytes <= data.len(), "buffer too short");


    let total_bits = (end_byte - start_byte) * 8;
    let shift = total_bits - (bit_offset % 8 + width);

    let leading = needed_bytes * 8 - (bit_in_first + width);
    let val = (tmp >> leading) & ((1u128 << width) - 1);

    log::trace!("extract_bitfield - offset: {}, width: {} start_byte: {}, end_byte: {}, bytes: {:?} val: {}, shift: {}",
        bit_offset,
        width,
        start_byte,
        end_byte,
        &data[start_byte..end_byte],
        val,
        shift
    );

    val as u64
}

macro_rules! bitfield {
    ($getter:ident, $offset:expr, $width:expr, $type:ty) => {
        pub fn $getter(&self) -> $type {
            extract_bitfield(&self.data, $offset, $width) as $type
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
    bitfield!(m_key, 0, 64, u64);
    bitfield!(gid_prefix, 64, 64, u64);
    bitfield!(lid, 128, 16, u16);
    bitfield!(master_sm_lid, 144, 16, u16);
    bitfield!(capability_mask, 160, 32, u32);
    bitfield!(diag_code, 192, 16, u16);
    bitfield!(m_key_lease_period, 208, 16, u16);
    bitfield!(local_portnum, 224, 8, u8);
    bitfield!(link_width_enabled, 232, 8, u8);
    bitfield!(link_width_supported, 240, 8, u8);
    bitfield!(link_width_active, 248, 8, u8);
    bitfield!(link_speed_supported, 256, 4, u8);
    bitfield!(port_state, 260, 4, u8);
    bitfield!(port_physical_state, 264, 4, u8);
    bitfield!(link_down_default_state, 268, 4, u8);
    bitfield!(m_key_protect_bits, 272, 2, u8);
    bitfield!(m_key_protect_bits_ext, 274, 3, u8);
    bitfield!(lmc, 277, 3, u8);
    bitfield!(link_speed_active, 280, 4, u8);
    bitfield!(link_speed_enabled, 284, 4, u8);
    bitfield!(neighnor_mtu, 288, 4, u8);
    bitfield!(master_sm_sl, 292, 4, u8);
    bitfield!(vl_cap, 296, 4, u8);
    bitfield!(init_type, 300, 4, u8);
    bitfield!(vl_high_limit, 304, 8, u8);
    bitfield!(vl_arbitration_high_cap, 312, 8, u8);
    bitfield!(vl_arbitration_low_cap, 320, 8, u8);
    bitfield!(init_type_reply, 328, 4, u8);
    bitfield!(mtu_cap, 332, 4, u8);
    bitfield!(vl_stall_count, 336, 3, u8);
    bitfield!(hoq_life, 339, 5, u8);
    bitfield!(operational_vls, 344, 4, u8);
    bitfield!(partition_enforcement_inbound, 348, 1, u8);
    bitfield!(partition_enforcement_outbound, 349, 1, u8);
    bitfield!(filter_raw_inbound, 350, 1, u8);
    bitfield!(filter_raw_outbound, 351, 1, u8);
    bitfield!(m_key_violations, 352, 16, u16);
    bitfield!(p_key_violations, 368, 16, u16);
    bitfield!(q_key_violations, 384, 16, u16);
    bitfield!(guid_cap, 400, 8, u8);
    bitfield!(client_register, 408, 1, u8);
    bitfield!(multicast_pkey_trap_suppression, 409, 2, u8);
    bitfield!(subnet_timeout, 411, 5, u8);
    bitfield!(partition_top, 416, 2, u8);
    bitfield!(enhanced_qos_arbiter_enabled, 417, 1, u8);
    bitfield!(resp_time_value, 419, 5, u8);
    bitfield!(local_phy_errors, 424, 4, u8);
    bitfield!(overrun_errors, 428, 4, u8);
    bitfield!(max_credit_hint, 432, 16, u8);
    bitfield!(link_round_trip_latency, 456, 24, u32);
    bitfield!(capability_mask2, 480, 16, u16);
    bitfield!(link_speed_ext_active, 496, 4, u8);
    bitfield!(link_speed_ext_supported, 500, 4, u8);
    bitfield!(link_speed_ext_enabled, 504, 5, u8);


}