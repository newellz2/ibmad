#[cfg(test)]
mod sim_tests {
    use std::cell::RefCell;
    use std::io::{Read, Write};
    use std::os::fd::{FromRawFd, IntoRawFd};
    use std::rc::Rc;
    use std::os::unix::net::UnixStream;
    use std::fs;

    use ibmad::sim::Port;

    fn sample_umad_attr(attr_id: u16, path: [u8; 64]) -> ibmad::mad::ib_user_mad {
        use ibmad::mad::{dr_smp_mad, ib_mad};

        // build DR SMP MAD content
        let dr = dr_smp_mad {
            m_key: 0,
            drslid: 0xffff,
            drdlid: 0xffff,
            reserved: [0; 28],
            attr_layout: [0; 64],
            initial_path: path,
            return_path: [0; 64],
        };

        // embed DR SMP into MAD payload
        let mut mad = ib_mad {
            base_version: 0x1,
            mgmt_class: ibmad::mad::IB_MGMT_CLASS_DIRECT_ROUTED_SMP.to_be(),
            class_version: 0x1,
            method: 0x1,
            status: 0,
            hop_ptr: 0,
            hop_cnt: 0,
            tid: 0x1337 as u64,
            attr_id: attr_id as u16,
            additional_status: 0,
            attr_mod: 0,
            data: [0; 232],
        };

        let dr_bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(
                &dr as *const dr_smp_mad as *const u8,
                std::mem::size_of::<dr_smp_mad>(),
            )
        };
        mad.data[..dr_bytes.len()].copy_from_slice(dr_bytes);

        let mut umad = ibmad::mad::ib_user_mad {
            agent_id: 0,
            status: 0,
            timeout_ms: 50,
            retries: 1,
            length: std::mem::size_of::<ib_mad>() as u32,
            addr: ibmad::mad::ib_mad_addr {
                qpn: 0,
                qkey: ibmad::mad::IB_DEFAULT_QKEY.to_be(),
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

        let mad_bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(
                &mad as *const ib_mad as *const u8,
                std::mem::size_of::<ib_mad>(),
            )
        };
        umad.data[..mad_bytes.len()].copy_from_slice(mad_bytes);

        umad
    }

    fn sample_umad(attr_id: u16, path: [u8; 64]) -> ibmad::mad::ib_user_mad {
        sample_umad_attr(attr_id, path)
    }

    #[test]
    fn create_new_fabric_success() {
        let (client, server) = UnixStream::pair().unwrap();

        let _client_file = unsafe { fs::File::from_raw_fd(client.into_raw_fd()) };
        let server_file = unsafe { fs::File::from_raw_fd(server.into_raw_fd()) };

        let mut _fabric = ibmad::sim::Fabric::new(server_file);
    }

    #[test]
    fn test_dr_mad_success() {

        let _ = env_logger::try_init();

        let (client, server) = UnixStream::pair().unwrap();

        let mut client_file = unsafe { fs::File::from_raw_fd(client.into_raw_fd()) };
        let server_file = unsafe { fs::File::from_raw_fd(server.into_raw_fd()) };
        let mut fabric = ibmad::sim::Fabric::new(server_file);

        {
            // build sixteen spine switches for a non blocking fabric
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
                let mut leaf_ref = leaf_rc.borrow_mut();

                for i in 0..=65 {
                    let port = Port::new_port(i as u8, lid, leaf_rc.clone());
                    leaf_ref.ports.push(Rc::new(RefCell::new(port)));
                }

                lid += 1;

                // connect leaf to all spines for a non blocking fabric
                for (spine_idx, spine_rc) in spines.iter().enumerate() {
                    let spine_port_rc = {
                        let spine_ref = spine_rc.borrow();
                        spine_ref.ports[leaf_idx + 1].clone()
                    };
                    let leaf_port_rc = leaf_ref.ports[33 + spine_idx].clone();
                    spine_port_rc
                        .borrow_mut()
                        .remote_port = Some(Rc::downgrade(&leaf_port_rc));
                    leaf_port_rc
                        .borrow_mut()
                        .remote_port = Some(Rc::downgrade(&spine_port_rc));
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

        let mut path: [u8; 64] = [0; 64];

        path[0] = 0;
        path[1] = 1;
        path[2] = 34;
        path[3] = 29;

        // NodeInfo
        let umad = sample_umad(0x0011, path);

        let _r = client_file.write(&umad.to_bytes());

        let _r = fabric.process_one_umad();

        let mut buf: [u8; 320] = [0; 320];
        let _r = client_file.read(&mut buf);

        let pi_mad = ibmad::mad::port_info::from_bytes(
            &buf[128..] // 64 + 24 + 40 = 128
        );

        log::debug!("{:?}", pi_mad);

    }
    
}