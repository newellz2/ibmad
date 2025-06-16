#[cfg(test)]
mod discovery_tests {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::os::fd::{FromRawFd, IntoRawFd};
    use std::rc::Rc;
    use std::os::unix::net::UnixStream;
    use std::sync::mpsc::channel;
    use std::{fs, thread};

    use ibmad::mad::IbMadPort;
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
                    for i in 0..65 {
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
                let mut leaf_ref = leaf_rc.borrow_mut();

                for i in 0..65 {
                    let port = Port::new_port(i as u8, lid, leaf_rc.clone());
                    leaf_ref.ports.push(Rc::new(RefCell::new(port)));
                }

                lid += 1;

                // connect leaf to all spines for a non blocking fabric
                for i in 0..2 {
                    for (spine_idx, spine_rc) in spines.iter().enumerate() {
                        let base = i * 32;

                        let spine_port_rc = {
                            let spine_ref = spine_rc.borrow();

                            // Iteration 1: Port 1-32, Iterations 2: Ports 33-64
                            spine_ref.ports[leaf_idx + 1 + base].clone()
                        };

                        // Iteration 1: Port 33-48, Iterations 2: Ports 49-64
                        let leaf_port_rc = leaf_ref.ports[33 + spine_idx + (base/2)].clone();
                        spine_port_rc
                            .borrow_mut()
                            .remote_port = Some(Rc::downgrade(&leaf_port_rc));
                        leaf_port_rc
                            .borrow_mut()
                            .remote_port = Some(Rc::downgrade(&spine_port_rc));
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
                    let leaf_hca_port_rc = leaf_ref.ports[h + 1].clone();
                    leaf_hca_port_rc
                        .borrow_mut()
                        .remote_port = Some(Rc::downgrade(&hca_port));
                    hca_port
                        .borrow_mut()
                        .remote_port = Some(Rc::downgrade(&leaf_hca_port_rc));

                    // first HCA becomes the first hop in dr_paths
                    if hca_count == 1 {
                        fabric.dr_paths.insert([0; 64], Rc::downgrade(&hca_port));
                    }
                }
            }
        }
    }

    #[test]
    fn test_discovery_success() {

        let _ = env_logger::try_init();

        let (client, server) = UnixStream::pair().unwrap();

        let client_file = unsafe { fs::File::from_raw_fd(client.into_raw_fd()) };
        let server_file = unsafe { fs::File::from_raw_fd(server.into_raw_fd()) };

        let (tx, rx) = channel::<bool>();

        thread::spawn(|| {
            let mut fabric = ibmad::sim::Fabric::new(server_file);
            build_fabric(&mut fabric);
            let _ = fabric.run(rx);
        });
        
        let port = IbMadPort{
            file: client_file,
        };

        let mut fabric = ibmad::discovery::Fabric{
            port: port,
            hcas: Vec::new(),
            switches: Vec::new(),
            nodes: Vec::new(),
            dr_paths: HashMap::new(),
        };

        let _ = fabric.discover();

        let _ = tx.send(true);


    }
}