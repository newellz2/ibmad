#[cfg(test)]
mod discovery_tests {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::os::fd::{FromRawFd, IntoRawFd};
    use std::rc::Rc;
    use std::os::unix::net::UnixStream;
    use std::sync::mpsc::channel;
    use std::{fs, sync, thread};

    use ibmad::mad::{self, IbMadPort};
    use ibmad::sim::Port;

    fn build_fabric(fabric: &mut ibmad::sim::Fabric){
        {
            // build sixteen spine switches
            let mut spines = Vec::new();
            let mut lid = 2000; // Spines start at 2000

            for spine_idx in 0..16 {
                let spine = ibmad::sim::Node::new_switch(
                    &format!("spine-{}", spine_idx),
                    0x7ffc_0000_0000_1000 + spine_idx as u64,
                );
                let spine_rc = fabric.add_switch(spine);

                {
                    let mut spine_ref = spine_rc.borrow_mut();
                    for i in 0..=65 {
                        let port = Port::new_port(i, lid, spine_rc.clone());
                        spine_ref.ports.push(Rc::new(RefCell::new(port)));
                    }
                }
                spines.push(spine_rc);
                lid += 1;
            }

            // create thirty two leaf switches each hosting thirty two HCAs
            let mut hca_count = 0;
            let mut lid = 3000; // Leaf switches start at 3000

            for leaf_idx in 0..32 {
                let leaf = ibmad::sim::Node::new_switch(
                    &format!("leaf-{}", leaf_idx),
                    0x7ffc_0000_0000_2000 + leaf_idx as u64,
                );

                let leaf_rc = fabric.add_switch(leaf);

                {
                    let mut leaf_ref = leaf_rc.borrow_mut();
                    for i in 0..=65 {
                        let port = Port::new_port(i as u8, lid, leaf_rc.clone());
                        log::debug!("Adding leaf port, logical_state: {},  physical_state: {}",
                            port.port_info.port_state(),
                            port.port_info.port_physical_state(),
                        );
                        leaf_ref.ports.push(Rc::new(RefCell::new(port)));
                    }
                }

                lid += 1;

                // connect leaf to all spines for a non blocking fabric
                // 4*16 = 64 spine ports
                for i in 0..32 {
                    for (spine_idx, spine_rc) in spines.iter().enumerate() {
                        let base = i * 1;

                        let spine_port_rc = {
                            let spine_ref = spine_rc.borrow();
                            spine_ref.ports[leaf_idx + 1 + base].clone()
                        };

                        let port_idx = 33 + spine_idx + (base / 2);
                        log::debug!("Adding leaf to spine port: base={}, port={}, leaf={}, spine={}", base, port_idx, leaf_idx, spine_idx);
                        
                        // Now we get an immutable borrow which is fine.
                        let leaf_port_rc = leaf_rc.borrow().ports[port_idx].clone();

                        ibmad::sim::connect_ports(&spine_port_rc, &leaf_port_rc);
                    }
                }

                // each leaf hosts thirty two HCAs on ports 1-32
                for h in 0..32 {
                    hca_count += 1;
                    let hca = ibmad::sim::Node::new_hca(
                        &format!("host{:04}", hca_count),
                        0x7ffc_0000_0000_3000 + hca_count as u64,
                    );
                    let hca_rc = fabric.add_hca(hca);

                    let hca_port = Rc::new(RefCell::new(ibmad::sim::Port::new_port(1, hca_count + 1, hca_rc.clone())));
                    hca_rc.borrow_mut().ports.push(hca_port.clone());

                    // connect HCA to leaf
                    let leaf_hca_port_rc = leaf_rc.borrow().ports[h + 1].clone();

                    ibmad::sim::connect_ports(&leaf_hca_port_rc, &hca_port);

                    // first HCA becomes the first hop in dr_paths
                    if hca_count == 1 {
                        fabric.dr_paths.insert([0; 64], Rc::downgrade(&hca_port));
                    }
                }
            }
        }
    }

    #[test]
    fn test_seq_discovery_sim_success() {

        let _ = env_logger::try_init();

        let (client, server) = UnixStream::pair().unwrap();

        let client_file = unsafe { fs::File::from_raw_fd(client.into_raw_fd()) };
        let server_file = unsafe { fs::File::from_raw_fd(server.into_raw_fd()) };

        let (tx, rx) = channel::<bool>();
        let barrier = sync::Arc::new(sync::Barrier::new(2));
        let barrier_clone: sync::Arc<sync::Barrier> = barrier.clone();

        thread::spawn(move || {
            let mut fabric = ibmad::sim::Fabric::new(server_file);
            fabric.response_delay = Some(600);
            build_fabric(&mut fabric);
            barrier_clone.wait();
            let _ = fabric.run(rx);
        });
        
        let port = IbMadPort{
            file: client_file,
        };

        let mut fabric = ibmad::discovery::Fabric{
            port: port,
            node_map: HashMap::new(),
            nodes: Vec::new(),
            hcas: Vec::new(),
            switches: Vec::new(),
            dr_paths: HashMap::new(),
            ni_timings: Vec::new(),
            retries: 1,
            timeout: 50,
            mad_errors: 0,
            mad_timeouts: 0,
            mads_sent: 0,
            tid: 1,
        };

        barrier.wait();

        let r = fabric.seq_discover();

        if let Err(e) = r {
            log::debug!("{:?}", e);
        }

        let _ = tx.send(true);
    }

    #[test]
    fn test_discovery_success() {

        let _ = env_logger::try_init();

        match  ibmad::ca::get_cas(){
            Ok(cas) =>{
                        assert!(cas.len() > 0, "No CAs found.");
                        let hca = &cas[0];
                        match ibmad::mad::open_port(hca) {
                            Ok(mut port) => {
                                let _ = mad::register_agent(&mut port, 0x81);
                                let mut fabric = ibmad::discovery::Fabric{
                                    port: port,
                                    node_map: HashMap::new(),
                                    nodes: Vec::new(),
                                    hcas: Vec::new(),
                                    switches: Vec::new(),
                                    dr_paths: HashMap::new(),
                                    ni_timings: Vec::new(),
                                    retries: 1,
                                    timeout: 50,
                                    mad_errors: 0,
                                    mad_timeouts: 0,
                                    mads_sent: 0,
                                    tid: 1,
                                };

                                let r = fabric.seq_discover();

                                match r {
                                    Ok(_) => {},
                                    Err(e) => { log::debug!("Error: {:?}", e)}        
                                }

                            }
                            Err(_) => {}, // Do nothing
                        }
                    }
            Err(_) => {}, // Do nothing
                    }
    }
}