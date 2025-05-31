#[cfg(test)]
mod mad_tests {
    use std::path::Path;
    use ibmad::mad::{open_port, register_agent, IB_DEFAULT_QKEY};


    #[test]
    fn open_port_success() {

        let _ = env_logger::try_init();

        if !Path::new(ibmad::ca::SYS_INFINIBAND).exists() {
            eprintln!("IB system path not found, skipping test");
            return;
        }

        match  ibmad::ca::get_cas(){
            Ok(cas) =>{
                assert!(cas.len() > 0, "No CAs found.");
                let hca = &cas[0];
                match open_port(hca) {
                    Ok(port) => {
                        log::debug!("open_port_success - IB MAD Port: {:?}", port);
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

    #[test]
    fn send_nodedesc_success() {

        let _ = env_logger::try_init();
        if !Path::new("/dev/infiniband/umad0").exists() {
            eprintln!("UMAD device not found, skipping test");
            return;
        }

        match  ibmad::ca::get_cas(){
            Ok(cas) =>{
                assert!(cas.len() > 0, "No CAs found.");
                let hca = &cas[0];
                match open_port(hca) {
                    Ok(mut port) => {
                        log::debug!("tests - send_success - Opened IB MAD Port: {:?}", port);
                        if let Ok(agent_id) = register_agent(&mut port, 0x81) {

                        log::debug!("tests - send_success - Registered agent: {}", agent_id);

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
                                attr_id: (0x0010 as u16).to_be(),
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
                                    qkey: IB_DEFAULT_QKEY.to_be(),
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

                            log::debug!("tests - send_success -  Sending MAD: {:?}", umad);
                            let r = ibmad::mad::send(&mut port, &umad);
                            match r {
                                Ok(s) => {
                                    log::debug!("tests - send_success -  Sending Successful, size: {:?}", s);
                                },
                                Err(e) => {
                                    log::debug!("tests - send_success -  Sending MAD Failed: {:?}", e);
                                },
                            }

                            let _ = ibmad::mad::recv(&mut port, &mut umad);

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

    #[test]
    fn send_nodeinfo_success() {

        let _ = env_logger::try_init();
        if !Path::new("/dev/infiniband/umad0").exists() {
            eprintln!("UMAD device not found, skipping test");
            return;
        }

        match  ibmad::ca::get_cas(){
            Ok(cas) =>{
                assert!(cas.len() > 0, "No CAs found.");
                let hca = &cas[0];
                match open_port(hca) {
                    Ok(mut port) => {
                        log::debug!("tests - send_nodeinfo_success - Opened IB MAD Port: {:?}", port);
                        if let Ok(agent_id) = register_agent(&mut port, 0x81) {

                        log::debug!("tests - send_nodeinfo_success - Registered agent: {}", agent_id);

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
                                attr_id: (0x0011 as u16).to_be(),
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
                                    qkey: IB_DEFAULT_QKEY.to_be(),
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

                            log::debug!("tests - send_nodeinfo_success -  Sending MAD: {:?}", umad);
                            let r = ibmad::mad::send(&mut port, &umad);
                            match r {
                                Ok(s) => {
                                    log::debug!("tests - send_nodeinfo_success -  Sending Successful, size: {:?}", s);
                                },
                                Err(e) => {
                                    log::debug!("tests - send_nodeinfo_success -  Sending MAD Failed: {:?}", e);
                                },
                            }

                            let _ = ibmad::mad::recv(&mut port, &mut umad);

                            let umad_bytes: &[u8] = unsafe {
                                std::slice::from_raw_parts(
                                    &umad as *const ibmad::mad::ib_user_mad as *const u8,
                                    std::mem::size_of::<ibmad::mad::ib_user_mad>(),
                                )
                            };

                            let mut ni = ibmad::mad::node_info::default();
                            let ni_ptr = &mut ni as *mut ibmad::mad::node_info as *mut u8;

                            unsafe {
                                std::ptr::copy_nonoverlapping(umad_bytes[128..=192].as_ptr(), 
                                    ni_ptr,
                                    std::mem::size_of::<[u8; 64 as usize] >());
                            };

                            log::debug!("tests - send_nodeinfo_success -  NodeInfo: {:?}", ni);
                            log::debug!("tests - send_nodeinfo_success -  NodeInfo.system_guid: {:x}", ni.system_guid.to_be());
                            log::debug!("tests - send_nodeinfo_success -  NodeInfo.device_id: {}", ni.device_id.to_be());
                            log::debug!("tests - send_nodeinfo_success -  NodeInfo.revision: {:x}", ni.revision.to_be());


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
