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
    fn register_agent_success() {

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
                        log::debug!("register_agent_success - Opened IB MAD Port: {:?}", port);
                        let r = register_agent(&mut port, ibmad::IB_PERFORMANCE_MGMT_CLASS);
                        assert!(r.is_ok(), "Failed to register agent: {:?}", r);
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
                        if let Ok(_) = register_agent(&mut port, 0x81) {
                            ibmad::mad::send(&mut port);
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
