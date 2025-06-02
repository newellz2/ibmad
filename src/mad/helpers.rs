pub fn get_bitfield(data: &[u8], bit_offset: usize, width: usize) -> u64 {

    assert!((1..=64).contains(&width), "width must be 1-64");

    let start_byte = bit_offset / 8;
    let pos_in_first_byte = bit_offset % 8;
    let needed_bytes = (pos_in_first_byte + width + 7) / 8;
    let end_byte = start_byte + needed_bytes;

    let mut tmp = 0u128;
    for &b in &data[start_byte..start_byte + needed_bytes] {
        tmp = (tmp << 8) | b as u128;
    }

    assert!(start_byte + needed_bytes <= data.len(), "buffer too short");

    let total_bits = (end_byte - start_byte) * 8;
    let shift = total_bits - (bit_offset % 8 + width);

    let leading = needed_bytes * 8 - (pos_in_first_byte + width);
    let val = (tmp >> leading) & ((1u128 << width) - 1);

    log::trace!("extract_bitfield - offset: {}, width: {} start_byte: {}, end_byte: {}, bytes: {:?} val: {:x}, shift: {}",
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


pub fn set_bitfield(data: &mut [u8], bit_offset: usize, width: usize, val: u64) {

    assert!((1..=64).contains(&width), "width must be 1-64");
    let val = if width < 64 {
        val & ((1 << width) - 1)  // Mask out anything more than the width
    } else {
        val
    };

    let start_byte = bit_offset / 8;
    let pos_in_first_byte = bit_offset % 8;
    let needed_bytes = (pos_in_first_byte + width + 7) / 8;
    let end_byte = start_byte + needed_bytes;

    let mut cur_val = 0u128;
    for &b in &data[start_byte..start_byte + needed_bytes] {
        cur_val = (cur_val << 8) | b as u128;
    }

    assert!(start_byte + needed_bytes <= data.len(), "buffer too short");

    let total_bits = (end_byte - start_byte) * 8;
    let shift = total_bits - (bit_offset % 8 + width);

    let mask =  ((1u128 << width) - 1) << shift;
    let cur_val_mask = cur_val & !mask;
    let mut new_val = cur_val_mask | ((val as u128) << shift);

    log::trace!("set_bitfield - offset: {}, width: {} start_byte: {}, end_byte: {}, cur_val: {} new_val: {}, shift: {:}, mask: 0x{:x}",
        bit_offset,
        width,
        start_byte,
        end_byte,
        cur_val,
        new_val,
        shift,
        mask
    );

    for i in (start_byte..end_byte).rev() {
        data[i] = (new_val & 0xff) as u8;
        new_val >>= 8;
    }

}