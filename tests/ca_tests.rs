#[cfg(test)]
mod common;

#[cfg(test)]
mod ca_tests {
    use super::common;

    #[test]
    fn get_cas_names_success() {
        common::setup();

        if !common::can_run_ib_tests() {
            eprintln!("IB system path not found, skipping test");
            return;
        }

        match ibmad::ca::get_cas_names() {
            Ok(cas) => {
                assert!(!cas.is_empty(), "No CAs found.");
            }
            Err(e) => {
                panic!("Error finding CAs: {:?}", e);
            }
        }
    }

    #[test]
    fn get_ca_success() {
        common::setup();

        if !common::can_run_ib_tests() {
            eprintln!("IB system path not found, skipping test");
            return;
        }

        // Try to find a valid CA name first instead of hardcoding mlx5_0
        let ca_names = ibmad::ca::get_cas_names().expect("Failed to get CA names");
        if ca_names.is_empty() {
            eprintln!("No CAs found to test get_ca with");
            return;
        }
        let target_ca = &ca_names[0];

        match ibmad::ca::get_ca(target_ca) {
            Ok(ca) => {
                assert!(!ca.name.is_empty(), "CA not found.");
                log::debug!("get_ca_success - CA: {:?}", ca);
            }
            Err(e) => {
                panic!("Error finding CA {}: {:?}", target_ca, e);
            }
        }
    }

    #[test]
    fn get_cas_success() {
        common::setup();

        if !common::can_run_ib_tests() {
            eprintln!("IB system path not found, skipping test");
            return;
        }

        match ibmad::ca::get_cas() {
            Ok(cas) => {
                assert!(!cas.is_empty(), "No CAs found.");
            }
            Err(e) => {
                panic!("Error finding CAs: {:?}", e);
            }
        }
    }

    #[test]
    fn get_cas_counters_success() {
        common::setup();

        if !common::can_run_ib_tests() {
            eprintln!("IB system path not found, skipping test");
            return;
        }

        match ibmad::ca::get_cas() {
            Ok(cas) => {
                assert!(!cas.is_empty(), "No CAs found.");
                for ca in cas {
                    for port in ca.ports {
                        log::debug!("Port: {:?}", port.path);

                        match port.get_counters() {
                            Ok(ctrs) => {
                                log::debug!("Counters: {:?}", ctrs)
                            }
                            Err(e) => {
                                panic!("Error finding counters: {:?}", e);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                panic!("Error finding CAs: {:?}", e);
            }
        }
    }
}
