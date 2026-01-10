#[cfg(test)]
mod discovery_tests {
    use std::cell::RefCell;
    use std::collections::{HashMap, HashSet};
    use std::os::fd::{FromRawFd, IntoRawFd};
    use std::os::unix::net::UnixStream;
    use std::rc::Rc;
    use std::sync::mpsc::channel;
    use std::{fs, sync, thread};

    use ibmad::enums::IbPortLinkLayerState;
    use ibmad::mad::{self, IB_MGMT_CLASS_PERFORMANCE, IbMadPort, open_port, open_smp_port};
    use ibmad::sim::Port;

    fn build_fabric(fabric: &mut ibmad::sim::Fabric) {
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
                        log::debug!(
                            "Adding leaf port, logical_state: {},  physical_state: {}",
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
                        log::debug!(
                            "Adding leaf to spine port: base={}, port={}, leaf={}, spine={}",
                            base,
                            port_idx,
                            leaf_idx,
                            spine_idx
                        );

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

                    let hca_port = Rc::new(RefCell::new(ibmad::sim::Port::new_port(
                        1,
                        hca_count + 1,
                        hca_rc.clone(),
                    )));
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

        let port = IbMadPort { file: client_file };

        let mut fabric = ibmad::discovery::Fabric {
            port: port,
            agent_id: 0,
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
    fn test_switch_enumeration() {
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

        let port = IbMadPort { file: client_file };

        let mut fabric = ibmad::discovery::Fabric {
            port,
            agent_id: 0,
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

        fabric.seq_discover().expect("Discovery should succeed");

        let total_switch_entries = fabric.switches.len();
        assert_eq!(
            total_switch_entries, 48,
            "Expected 48 switch entries (16 spines + 32 leaves)"
        );

        let mut seen_guids = HashSet::new();
        let mut spine_count = 0;
        let mut leaf_count = 0;

        for switch_weak in &fabric.switches {
            let switch_arc = switch_weak
                .upgrade()
                .expect("Switch reference should still be valid");
            let (guid, description) = {
                let switch_guard = switch_arc
                    .read()
                    .expect("Switch RwLock should not be poisoned");
                (
                    switch_guard.node_guid,
                    switch_guard
                        .description
                        .clone()
                        .unwrap_or_else(|| String::from("<unknown>")),
                )
            };

            if !seen_guids.insert(guid) {
                panic!(
                    "Duplicate switch GUID encountered: 0x{:X} ({})",
                    guid.to_be(),
                    description
                );
            }

            if description.starts_with("spine-") {
                spine_count += 1;
            } else if description.starts_with("leaf-") {
                leaf_count += 1;
            } else {
                panic!("Unexpected switch description: {}", description);
            }
        }

        assert_eq!(spine_count, 16, "Expected 16 spine switches");
        assert_eq!(leaf_count, 32, "Expected 32 leaf switches");
        assert_eq!(
            seen_guids.len(),
            total_switch_entries,
            "Switch GUIDs should be unique"
        );

        let _ = tx.send(true);
    }

    #[test]
    fn test_nodes_enumeration_real_hca() {
        let _ = env_logger::try_init();

        let ca = match ibmad::ca::get_ca("mlx5_0") {
            Ok(ca) => ca,
            Err(e) => {
                log::warn!("Skipping real HCA switch enumeration test: {:?}", e);
                return;
            }
        };

        let mut port = match open_smp_port(&ca) {
            Ok(port) => port,
            Err(e) => {
                log::warn!(
                    "Skipping real HCA switch enumeration test, open_port failed: {:?}",
                    e
                );
                return;
            }
        };

        if let Err(e) = mad::register_agent(&mut port, 0x81) {
            log::warn!(
                "Skipping real HCA switch enumeration test, register_agent failed: {:?}",
                e
            );
            return;
        }

        let mut fabric = ibmad::discovery::Fabric {
            port,
            agent_id: 0,
            node_map: HashMap::new(),
            nodes: Vec::new(),
            hcas: Vec::new(),
            switches: Vec::new(),
            dr_paths: HashMap::new(),
            ni_timings: Vec::new(),
            retries: 2,
            timeout: 100,
            mad_errors: 0,
            mad_timeouts: 0,
            mads_sent: 0,
            tid: 1,
        };

        match fabric.seq_discover() {
            Ok(_) => {}
            Err(e) => {
                log::warn!(
                    "Skipping real HCA switch enumeration test, discovery failed: {:?}",
                    e
                );
            }
        }

        assert!(
            !fabric.nodes.is_empty(),
            "Expected at least one node to be discovered on mlx5_0"
        );

        for node_arc in &fabric.nodes {
            let node_guard = match node_arc.read() {
                Ok(guard) => guard,
                Err(_) => continue,
            };
            if let Some(description) = &node_guard.description {
                log::info!("Node: {:?}", description);
            }
        }
    }

    #[test]
    fn test_discovery_perf_counters_real_hca() {
        let _ = env_logger::try_init();
        if !std::path::Path::new("/dev/infiniband/umad0").exists() {
            eprintln!("UMAD device not found, skipping test");
            return;
        }

        let ca = match ibmad::ca::get_ca("mlx5_0") {
            Ok(ca) => ca,
            Err(e) => {
                log::warn!("Skipping perf counter discovery test: {:?}", e);
                return;
            }
        };

        let mut smp_port = match open_smp_port(&ca) {
            Ok(port) => port,
            Err(e) => {
                log::warn!(
                    "Skipping perf counter discovery test, open_smp_port failed: {:?}",
                    e
                );
                return;
            }
        };

        if let Err(e) = mad::register_agent(&mut smp_port, 0x81) {
            log::warn!(
                "Skipping perf counter discovery test, SMP agent registration failed: {:?}",
                e
            );
            return;
        }

        let mut fabric = ibmad::discovery::Fabric {
            port: smp_port,
            agent_id: 0,
            node_map: HashMap::new(),
            nodes: Vec::new(),
            hcas: Vec::new(),
            switches: Vec::new(),
            dr_paths: HashMap::new(),
            ni_timings: Vec::new(),
            retries: 2,
            timeout: 100,
            mad_errors: 0,
            mad_timeouts: 0,
            mads_sent: 0,
            tid: 1,
        };

        if let Err(e) = fabric.seq_discover() {
            log::warn!("Discovery encountered an error: {:?}", e);
        }

        let mut perf_port = match open_port(&ca) {
            Ok(port) => port,
            Err(e) => {
                log::warn!(
                    "Skipping perf counter discovery test, open_port failed: {:?}",
                    e
                );
                return;
            }
        };

        let perf_agent = match mad::register_agent(&mut perf_port, IB_MGMT_CLASS_PERFORMANCE) {
            Ok(id) => id,
            Err(e) => {
                log::warn!(
                    "Skipping perf counter discovery test, perf agent registration failed: {:?}",
                    e
                );
                return;
            }
        };

        let mut targets: Vec<(u16, u8, String)> = Vec::new();

        for node_arc in &fabric.nodes {
            let node_guard = match node_arc.read() {
                Ok(guard) => guard,
                Err(_) => continue,
            };
            let node_desc = node_guard
                .description
                .clone()
                .unwrap_or_else(|| String::from("N/A"));
            let node_lid = node_guard.lid;

            for port_arc in &node_guard.ports {
                let port_guard = match port_arc.read() {
                    Ok(guard) => guard,
                    Err(_) => continue,
                };

                if port_guard.link_state != IbPortLinkLayerState::Active {
                    continue;
                }

                let lid = node_lid;

                if lid == 0 {
                    continue;
                }

                targets.push((lid, port_guard.number, node_desc.clone()));
            }
        }

        if targets.is_empty() {
            log::warn!("Discovered no active ports with LIDs; skipping perf queries");
            return;
        }

        for (lid, port_number, description) in targets {
            let perf_resp: Option<mad::perf_mad> = match mad::query_port_counters_extended(
                &mut perf_port,
                perf_agent,
                1000,
                1,
                lid,
                port_number,
            ) {
                Ok(resp) => Some(resp),
                Err(e) => {
                    assert!(
                        false,
                        "Failed to query PortCountersExtended for {} (LID {} Port {}): {:?}",
                        description, lid, port_number, e
                    );
                    None
                }
            };

            if let Some(perf_resp) = perf_resp {
                log::info!(
                    "Discovered perf counters for {} (LID {} Port {}): xmit_data={}, rcv_data={}, xmit_pkts={}, rcv_pkts={}, unicast_xmit={}, unicast_rcv={}, multicast_xmit={}, multicast_rcv={}",
                    description,
                    lid,
                    port_number,
                    perf_resp.port_xmit_data(),
                    perf_resp.port_rcv_data(),
                    perf_resp.port_xmit_pkts(),
                    perf_resp.port_rcv_pkts(),
                    perf_resp.port_unicast_xmit_pkts(),
                    perf_resp.port_unicast_rcv_pkts(),
                    perf_resp.port_multicast_xmit_pkts(),
                    perf_resp.port_multicast_rcv_pkts()
                );
            }
        }
    }

    #[test]
    fn test_hca_discovery_success() {
        let _ = env_logger::try_init();

        match ibmad::ca::get_ca("mlx5_0") {
            Ok(ca) => {
                let hca = &ca;
                match open_smp_port(hca) {
                    Ok(mut port) => {
                        let _ = mad::register_agent(&mut port, 0x81);
                        let agent_id = mad::register_agent(&mut port, 0x81).unwrap_or(0);
                        let mut fabric = ibmad::discovery::Fabric {
                            port: port,
                            agent_id,
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
                            Ok(_) => {
                                // Avoid dumping the entire port graph (very large); print a concise per-node summary.
                                for node_arc in &fabric.nodes {
                                    if let Ok(node) = node_arc.read() {
                                        log::debug!(
                                            "Node: desc='{}' guid=0x{:X} type={:?} lid={} local_port={} nports={} ports_vec_len={}",
                                            node.description.as_deref().unwrap_or("N/A"),
                                            node.node_guid.to_be(),
                                            node.node_type,
                                            node.lid,
                                            node.local_port,
                                            node.nports,
                                            node.ports.len()
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                log::debug!("Error: {:?}", e)
                            }
                        }
                    }
                    Err(_) => {} // Do nothing
                }
            }
            Err(_) => {} // Do nothing
        }
    }

    #[test]
    fn test_nvlink_discovery_success() {
        let _ = env_logger::try_init();

        match ibmad::ca::get_ca("sx_ib_0") {
            Ok(ca) => {
                let hca = &ca;
                match open_smp_port(hca) {
                    Ok(mut port) => {
                        let agent_id = mad::register_agent(&mut port, 0x81).unwrap_or(0);
                        let mut fabric = ibmad::discovery::Fabric {
                            port: port,
                            agent_id,
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

                        let r = fabric.seq_discover_nvlink();

                        match r {
                            Ok(_) => {
                                // Avoid dumping the entire port graph (very large); print a concise per-node summary.
                                for node_arc in &fabric.nodes {
                                    if let Ok(node) = node_arc.read() {
                                        log::debug!(
                                            "Node: desc='{}' guid=0x{:X} type={:?} lid={} local_port={} nports={} ports_vec_len={}",
                                            node.description.as_deref().unwrap_or("N/A"),
                                            node.node_guid.to_be(),
                                            node.node_type,
                                            node.lid,
                                            node.local_port,
                                            node.nports,
                                            node.ports.len()
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                log::debug!("Error: {:?}", e)
                            }
                        }
                    }
                    Err(_) => {} // Do nothing
                }
            }
            Err(_) => {} // Do nothing
        }
    }

    fn build_switch_root_fabric(fabric: &mut ibmad::sim::Fabric) {
        // Create 2 switches connected to each other
        let switch1 = ibmad::sim::Node::new_switch("switch-1", 0x1001);
        let switch1_rc = fabric.add_switch(switch1);

        let switch2 = ibmad::sim::Node::new_switch("switch-2", 0x1002);
        let switch2_rc = fabric.add_switch(switch2);

        // Add ports to switch1
        {
            let mut s1 = switch1_rc.borrow_mut();
            for i in 0..=5 {
                let port = Port::new_port(i, 100, switch1_rc.clone());
                s1.ports.push(Rc::new(RefCell::new(port)));
            }
        }

        // Add ports to switch2
        {
            let mut s2 = switch2_rc.borrow_mut();
            for i in 0..=5 {
                let port = Port::new_port(i, 200, switch2_rc.clone());
                s2.ports.push(Rc::new(RefCell::new(port)));
            }
        }

        // Connect switch1 port 1 to switch2 port 1
        let s1_p1 = switch1_rc.borrow().ports[1].clone(); // Port 0 is at index 0
        let s2_p1 = switch2_rc.borrow().ports[1].clone();
        ibmad::sim::connect_ports(&s1_p1, &s2_p1);

        // Set switch1 port 0 as the entry point (simulating running on switch1)
        let s1_p0 = switch1_rc.borrow().ports[0].clone();
        // Insert as FIRST_HOP ([0; 64])
        fabric.dr_paths.insert([0; 64], Rc::downgrade(&s1_p0));
    }

    #[test]
    fn test_switch_root_discovery() {
        let _ = env_logger::try_init();

        let (client, server) = UnixStream::pair().unwrap();
        let client_file = unsafe { fs::File::from_raw_fd(client.into_raw_fd()) };
        let server_file = unsafe { fs::File::from_raw_fd(server.into_raw_fd()) };

        let (tx, rx) = channel::<bool>();
        let barrier = sync::Arc::new(sync::Barrier::new(2));
        let barrier_clone = barrier.clone();

        thread::spawn(move || {
            let mut fabric = ibmad::sim::Fabric::new(server_file);
            build_switch_root_fabric(&mut fabric);
            barrier_clone.wait();
            let _ = fabric.run(rx);
        });

        let port = IbMadPort { file: client_file };

        let mut fabric = ibmad::discovery::Fabric {
            port,
            agent_id: 0,
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

        // Run discovery
        fabric.seq_discover().expect("Discovery should not fail hard");

        let _ = tx.send(true);

        // Check results
        // Should find switch-1 and switch-2
        assert_eq!(fabric.nodes.len(), 2, "Should discover 2 nodes");

        let s1 = fabric.nodes.iter().find(|n| n.read().unwrap().node_guid == 0x1001);
        assert!(s1.is_some(), "Should find switch-1");

        let s2 = fabric.nodes.iter().find(|n| n.read().unwrap().node_guid == 0x1002);
        assert!(s2.is_some(), "Should find switch-2");
    }
}
