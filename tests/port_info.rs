#[cfg(test)]
mod port_info_tests {
    use std::io;
    use std::fs;
    use std::io::BufRead;

    use ibmad::mad::port_info;

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

#[test]
    fn send_dr_port_info_success() {

        let _ = env_logger::try_init();
        if !std::path::Path::new("/dev/infiniband/umad0").exists() {
            eprintln!("UMAD device not found, skipping test");
            return;
        }

        match  ibmad::ca::get_cas(){
            Ok(cas) =>{
                assert!(cas.len() > 0, "No CAs found.");
                let hca = &cas[0];
                match ibmad::mad::open_port(hca) {
                    Ok(mut port) => {
                        log::debug!("tests - send_dr_port_info_success - Opened IB MAD Port: {:?}", port);
                        if let Ok(agent_id) = ibmad::mad::register_agent(&mut port, 0x81) {

                        log::debug!("tests - send_dr_port_info_success - Registered agent: {}", agent_id);

                            let mut dr = ibmad::mad::dr_smp_mad {
                                m_key: 0,
                                drslid: 0xffff,
                                drdlid: 0xffff,
                                reserved: [0; 28],
                                attr_layout: [0; 64],
                                initial_path: [0; 64],
                                return_path: [0; 64],
                            };

                            // First Hop Switch
                            dr.initial_path[0] = 0;
                            dr.initial_path[1] = 1;

                            // embed DR SMP into MAD payload
                            let mut mad = ibmad::mad::ib_mad {
                                base_version: 0x1,
                                mgmt_class: ibmad::mad::IB_MGMT_CLASS_DIRECT_ROUTED_SMP.to_be(),
                                class_version: 0x1,
                                method: (0x1 as u8).to_be(),
                                status: 0,
                                hop_ptr: 0,
                                hop_cnt: 1, // Second position in initial_path
                                tid: (0x11 as u64).to_be(),
                                attr_id: (0x0015 as u16).to_be(),
                                additional_status: 0x0000,
                                attr_mod: 0x0000_0000,
                                data: [0; 232],
                            };

                            let dr_bytes: &[u8] = unsafe {
                                std::slice::from_raw_parts(
                                    &dr as *const ibmad::mad::dr_smp_mad as *const u8,
                                    std::mem::size_of::<ibmad::mad::dr_smp_mad>(),
                                )
                            };
                            
                            mad.data[..dr_bytes.len()].copy_from_slice(dr_bytes);

                            let mut umad = ibmad::mad::ib_user_mad {
                                agent_id,
                                status: 0,
                                timeout_ms: 50,
                                retries: 1,
                                length: 0,
                                addr: ibmad::mad::ib_mad_addr {
                                    qpn: 0,
                                    qkey: ibmad::mad::IB_DEFAULT_QKEY.to_be(),
                                    lid: 0xffff,
                                    sl: 0,
                                    path_bits: 0,
                                    grh_present: 0,
                                    gid_index: 0,
                                    hop_limit: 64,
                                    traffic_class: 0,
                                    gid: [0; 16],
                                    flow_label: 0,
                                    pkey_index: 0,
                                    reserved: [0; 6],
                                },
                                data: [0; 256],
                            };

                            let ib_mad_bytes: &[u8] = unsafe {
                                std::slice::from_raw_parts(
                                    &mad as *const ibmad::mad::ib_mad as *const u8,
                                    std::mem::size_of::<ibmad::mad::ib_mad>(),
                                )
                            };
                            
                            umad.data[..ib_mad_bytes.len()].copy_from_slice(ib_mad_bytes);

                            log::debug!("tests - send_dr_port_info_success -  Sending MAD: {:?}", umad);
                            let r = ibmad::mad::send(&mut port, &umad);
                            match r {
                                Ok(s) => {
                                    log::debug!("tests - send_dr_port_info_success -  Sending Successful, size: {:?}", s);
                                },
                                Err(e) => {
                                    log::debug!("tests - send_dr_port_info_success -  Sending MAD Failed: {:?}", e);
                                },
                            }

                            let _ = ibmad::mad::recv(&mut port, &mut umad, 1000);

                            let pi_mad = port_info::from_bytes(&umad.data[64..]).unwrap();

                            log::debug!("tests - send_dr_port_info_success -  Sending MAD Failed: {:?}", pi_mad);
                            log::debug!("tests - send_dr_port_info_success -  pi_mad.lid: {}", pi_mad.lid());
                            log::debug!("tests - send_dr_port_info_success -  pi_mad.master_sm_lid: {}", pi_mad.master_sm_lid());


                        }
                    },
                    Err(e) => {
                        assert!(false, "{}", format!("Error opening port: {:?}", e));
                    },
                }
            }
            Err(e) => {
                assert!(false, "{}", format!("Error finding CAs: {:?}", e));
            }
        }
    }

}