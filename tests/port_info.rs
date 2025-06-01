#[cfg(test)]
mod port_info_tests {
    use std::io;
    use std::fs;
    use std::io::BufRead;

    fn get_pi_mad() -> Result<[u8; 256], io::Error> {
        let mut bytes: [u8; 256]= [0; 256];
        let f = fs::File::open("tests/data/packets/port_info.hex")?;
        let mut reader = io::BufReader::new(f);
        
        let mut line = String::new();
        let mut index: usize = 0;
        log::debug!("Looping through packet file.");

        loop {
            let s = reader.read_line(&mut line)?;
            if s != 0 {
                let parts = line.split(' ');
                for p in parts {
                    let p_trimmed = p.trim();
                    let iters = p_trimmed.len() / 2;
                    let mut pos: usize = 0;
                    for i in 0..iters {
                        let hex = &p_trimmed[pos..=pos+1];
                        let val = u8::from_str_radix(hex, 16)
                            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
                        log::trace!("Parsed value:  '{}', '{:x}' , '{}'", p_trimmed, val, hex);
                        pos += 2;
                        
                        // Set byte in array
                        bytes[index] = val;

                        index += 1;
                    }
                }
            } else {
                break;
            }
            line.clear();
            
        }

        log::trace!("get_pi_mad - Bytes:\n{}", ibmad::dump_bytes(&bytes));
        Ok(bytes)
    }

    #[test]
    fn get_port_info_lid_success() {

        let _ = env_logger::try_init();

        let bytes = get_pi_mad().unwrap();

        let dr_smp_bytes=  &bytes[24..256];

        log::trace!("get_pi_mad - bytes, length: {}, bytes: {:?}", dr_smp_bytes.len(), dr_smp_bytes);
        match ibmad::mad::dr_smp_mad::from_bytes(dr_smp_bytes) {
            Some(dr_smp) => {
                log::trace!("get_pi_mad - dr_smp: {:?}", dr_smp);
                let pi = ibmad::mad::port_info::from_bytes(&dr_smp.attr_layout).unwrap();

                log::trace!("get_pi_mad - lid: {}", pi.lid());
                assert!(pi.lid() == 27251, "lid is not 27251");

                log::trace!("get_pi_mad - m_key: {:x}", pi.m_key());

                log::trace!("get_pi_mad - gid_prefix: 0x{:x}", pi.gid_prefix());
                assert!(pi.gid_prefix() == 0xfe80_0000_0000_0000, "gid_prefix is not 27251");


                log::trace!("get_pi_mad - capability_mask: {:x}", pi.capability_mask());
                assert!(pi.capability_mask() == 0xa751_e848 as u32, "capability_mask is not 0xa751_e848.");

                log::trace!("get_pi_mad - master_sm_lid: {:?}", pi.master_sm_lid());
                assert!(pi.master_sm_lid() == 4, "SM LID is not 4");

                log::trace!("get_pi_mad - hoq_life: {:?}", pi.hoq_life());
                log::trace!("get_pi_mad - operational_vls: {:?}", pi.operational_vls());
                assert!(pi.operational_vls() == 3, "operational_vls is not 3");

                log::trace!("get_pi_mad - guid_cap: {:?}", pi.guid_cap());
                assert!(pi.guid_cap() == 8, "guid_cap is not 8");

                log::trace!("get_pi_mad - link_speed_ext_active: {:?}", pi.link_speed_ext_active());
                assert!(pi.link_speed_ext_active() == 8, "link_speed_ext_active is not 8");


            }
            None =>{

            }
        };

    }
}