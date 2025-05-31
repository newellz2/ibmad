
mod agent_tests {
    use  std::path;
    
    #[test]
    fn register_agent_success() {

        let _ = env_logger::try_init();

        if !path::Path::new("/dev/infiniband/umad0").exists() {
            eprintln!("UMAD device not found, skipping test");
            return;
        }

        match ibmad::ca::get_cas(){
            Ok(cas) =>{
                assert!(cas.len() > 0, "No CAs found.");
                let hca = &cas[0];
                match ibmad::mad::open_port(hca) {
                    Ok(mut port) => {
                        log::debug!("register_agent_success - Opened IB MAD Port: {:?}", port);
                        let r = ibmad::mad::register_agent(&mut port, ibmad::mad::IB_MGMT_CLASS_PERFORMANCE);
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
    fn register_agent_invalid_fd() {
        use std::fs::File;

        // open /dev/null which does not support our ioctl
        let file = File::open("/dev/null").expect("/dev/null should exist");
        let mut port = ibmad::mad::IbMadPort { file };

        let res = ibmad::mad::register_agent(&mut port, ibmad::mad::IB_MGMT_CLASS_PERFORMANCE);
        assert!(res.is_err(), "expected error registering agent on invalid fd");
    }
}