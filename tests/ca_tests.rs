#[cfg(test)]
mod ca_tests {
    use std::path::Path;

    #[test]
    fn get_cas_names_success() {

        let _ = env_logger::try_init();

        if !Path::new(ibmad::ca::SYS_INFINIBAND).exists() {
            eprintln!("IB system path not found, skipping test");
            return;
        }

        match  ibmad::ca::get_cas_names(){
            Ok(cas) =>{
                assert!( cas.len() > 0, "No CAs found.");
                for _ca in cas.iter(){
                    // 
                }
            }
            Err(e) => {
                assert!(false, "{}", format!("Error finding CAs: {:?}", e));
            }
        }
    }

    #[test]
    fn get_ca_success() {
        
        let _ = env_logger::try_init();

        if !Path::new(ibmad::ca::SYS_INFINIBAND).exists() {
            eprintln!("IB system path not found, skipping test");
            return;
        }

        match  ibmad::ca::get_ca("mlx5_0"){
            Ok(ca) =>{
                assert!(!ca.name.is_empty(), "CA not found.");
                log::debug!("get_ca_success - CA: {:?}", ca);

            }
            Err(e) => {
                assert!(false, "{}", format!("Error finding CA: {:?}", e));
            }
        }
    }

    #[test]
    fn get_cas_success() {
        
        let _ = env_logger::try_init();

        if !Path::new(ibmad::ca::SYS_INFINIBAND).exists() {
            eprintln!("IB system path not found, skipping test");
            return;
        }

        match  ibmad::ca::get_cas(){
            Ok(cas) =>{
                assert!(cas.len() > 0, "No CAs found.");
            }
            Err(e) => {
                assert!(false, "{}", format!("Error finding CAs: {:?}", e));
            }
        }
    }

    #[test]
    fn get_cas_counters_success() {
        
        let _ = env_logger::try_init();

        if !Path::new(ibmad::ca::SYS_INFINIBAND).exists() {
            eprintln!("IB system path not found, skipping test");
            return;
        }

        match  ibmad::ca::get_cas(){
            Ok(cas) =>{
                assert!(cas.len() > 0, "No CAs found.");
                for ca in cas {
                    for port in ca.ports{
                        log::debug!("Port: {:?}", port.path);

                        match port.get_counters() {
                            Ok(ctrs) => {
                                log::debug!("Counters: {:?}", ctrs)
                            }
                            Err(e) => {
                                assert!(false, "{}", format!("Error finding counters: {:?}", e));
                            }
                        }
                    }
                }
            }
            Err(e) => {
                assert!(false, "{}", format!("Error finding CAs: {:?}", e));
            }
        }
    }

}
