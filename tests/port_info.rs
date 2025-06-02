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
                    for _i in 0..iters {
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

        log::trace!("get_port_info_lid_success - bytes, length: {}, bytes: {:?}", dr_smp_bytes.len(), dr_smp_bytes);
        match ibmad::mad::dr_smp_mad::from_bytes(dr_smp_bytes) {
            Some(dr_smp) => {
                log::trace!("get_port_info_lid_success - dr_smp: {:?}", dr_smp);
                let pi = ibmad::mad::port_info::from_bytes(&dr_smp.attr_layout).unwrap();

                log::trace!("get_port_info_lid_success - lid: {}", pi.lid());
                assert!(pi.lid() == 27251, "lid is not 27251");

            }
            None =>{

            }
        };
    }

    #[test]
    fn set_port_info_lid_success() {

        let _ = env_logger::try_init();

        let bytes = get_pi_mad().unwrap();

        let dr_smp_bytes=  &bytes[24..256];

        log::trace!("set_port_info_lid_success - MAD bytes, length: {}, bytes: {:?}", dr_smp_bytes.len(), dr_smp_bytes);
        match ibmad::mad::dr_smp_mad::from_bytes(dr_smp_bytes) {
            Some(dr_smp) => {
                log::trace!("set_port_info_lid_success - DR SMP: {:?}", dr_smp);
                let mut pi = ibmad::mad::port_info::from_bytes(&dr_smp.attr_layout).unwrap();

                log::trace!("set_port_info_lid_success - lid: {}", pi.lid());
                pi.set_lid(27);
                log::trace!("set_port_info_lid_success - new lid: {}", pi.lid());

                log::trace!("set_port_info_lid_success - mkey: {}", pi.m_key());
                pi.set_mkey(0x1);
                log::trace!("set_port_info_lid_success - new mkey: {}", pi.m_key());

            }
            None =>{

            }
        };
    }
}