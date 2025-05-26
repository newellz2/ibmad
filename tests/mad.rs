#[cfg(test)]
mod mad_tests {
    use ibmad::mad::{open_port, register_agent};


    #[test]
    fn open_port_success() {

        let _ = env_logger::try_init();

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

        match  ibmad::cas::get_cas(){
            Ok(cas) =>{
                assert!(cas.len() > 0, "No CAs found.");
                let hca = &cas[0];
                match open_port(hca) {
                    Ok(mut port) => {
                        log::debug!("register_agent_success - Opened IB MAD Port: {:?}", port);
                        register_agent(&mut port, ibmad::IB_PERFORMANCE_MGMT_CLASS);
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

        match  ibmad::cas::get_cas(){
            Ok(cas) =>{
                assert!(cas.len() > 0, "No CAs found.");
                let hca = &cas[0];
                match open_port(hca) {
                    Ok(mut port) => {
                        log::debug!("send_success - Opened IB MAD Port: {:?}", port);
                        register_agent(&mut port, 0x81);
                        ibmad::mad::send(&mut port);
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
