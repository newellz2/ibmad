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

    log::trace!(
        "extract_bitfield - offset: {}, width: {} start_byte: {}, end_byte: {}, bytes: {:?} val: {:x}, shift: {}",
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
        val & ((1 << width) - 1) // Mask out anything more than the width
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

    let mask = ((1u128 << width) - 1) << shift;
    let cur_val_mask = cur_val & !mask;
    let mut new_val = cur_val_mask | ((val as u128) << shift);

    log::trace!(
        "set_bitfield - offset: {}, width: {} start_byte: {}, end_byte: {}, cur_val: {} new_val: {}, shift: {:}, mask: 0x{:x}",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_get_bitfield_aligned() {
        let mut data = [0u8; 8];
        
        // Test byte aligned write/read
        set_bitfield(&mut data, 0, 8, 0xAB);
        assert_eq!(data[0], 0xAB);
        assert_eq!(get_bitfield(&data, 0, 8), 0xAB);

        // Test u16 aligned write/read
        set_bitfield(&mut data, 8, 16, 0x1234);
        assert_eq!(data[1], 0x12);
        assert_eq!(data[2], 0x34);
        assert_eq!(get_bitfield(&data, 8, 16), 0x1234);
    }

    #[test]
    fn test_set_get_bitfield_unaligned() {
        let mut data = [0u8; 8];

        // Write 4 bits at offset 4 (lower nibble of byte 0)
        set_bitfield(&mut data, 4, 4, 0xF);
        assert_eq!(data[0], 0x0F);
        assert_eq!(get_bitfield(&data, 4, 4), 0xF);

        // Write crossing byte boundary: offset 12 (lower nibble of byte 1), width 8
        // Should occupy data[1][0..4] and data[2][4..8] ?
        // Offset 12 is in byte 1 (12/8 = 1, rem 4).
        // Width 8. Ends at 12+8=20.
        // Byte 1: bits 4-7. Byte 2: bits 0-3.
        set_bitfield(&mut data, 12, 8, 0xCC);
        assert_eq!(get_bitfield(&data, 12, 8), 0xCC);
    }

    #[test]
    fn test_set_bitfield_masking() {
        let mut data = [0u8; 8];
        // Value 0xFF exceeds 4 bits width, should be masked to 0xF
        set_bitfield(&mut data, 0, 4, 0xFF);
        assert_eq!(get_bitfield(&data, 0, 4), 0xF);
    }
    
    #[test]
    fn test_preserves_surrounding_bits() {
        let mut data = [0xFFu8; 4];
        
        // Set middle bits to 0
        set_bitfield(&mut data, 4, 4, 0x0);
        // data[0] should be 1111 0000 -> 0xF0
        assert_eq!(data[0], 0xF0);
        
        // Check other bytes untouched
        assert_eq!(data[1], 0xFF);
    }
}
