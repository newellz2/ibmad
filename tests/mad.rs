#[cfg(test)]
mod mad_tests {
    use std::path::Path;
    use ibmad::mad::{open_port, register_agent};


    #[test]
    fn open_port_success() {

        let _ = env_logger::try_init();

        if !Path::new(ibmad::SYS_INFINIBAND).exists() {
            eprintln!("IB system path not found, skipping test");
            return;
        }

        match  ibmad::cas::get_cas(){
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
    fn send_success() {

        let _ = env_logger::try_init();
        if !Path::new("/dev/infiniband/umad0").exists() {
            eprintln!("UMAD device not found, skipping test");
            return;
        }

        match  ibmad::cas::get_cas(){
            Ok(cas) =>{
                assert!(cas.len() > 0, "No CAs found.");
                let hca = &cas[0];
                match open_port(hca) {
                    Ok(mut port) => {
                        log::debug!("send_success - Opened IB MAD Port: {:?}", port);
                        if let Ok(agent_id) = register_agent(&mut port, 0x81) {
                            let umad = ibmad::mad::ib_user_mad {
                                agent_id,
                                status: 0,
                                timeout_ms: 0,
                                retries: 0,
                                length: 0,
                                addr: ibmad::mad::ib_mad_addr {
                                    qpn: 0,
                                    qkey: 0,
                                    lid: 0,
                                    sl: 0,
                                    path_bits: 0,
                                    grh_present: 0,
                                    gid_index: 0,
                                    hop_limit: 0,
                                    traffic_class: 0,
                                    gid: [0; 16],
                                    flow_label: 0,
                                    pkey_index: 0,
                                    reserved: [0; 6],
                                },
                                data: [0; 256],
                            };

                            let _ = ibmad::mad::send(&mut port, &umad);
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
