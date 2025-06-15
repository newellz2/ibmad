#[cfg(test)]
mod sim_tests {
    use std::cell::RefCell;
    use std::io::{Read, Write};
    use std::os::fd::{FromRawFd, IntoRawFd};
    use std::rc::Rc;
    use std::os::unix::net::UnixStream;
    use std::fs;

    use ibmad::sim::Port;

    fn sample_umad_attr(attr_id: u16) -> ibmad::mad::ib_user_mad {
        use ibmad::mad::{dr_smp_mad, ib_mad};

        // build DR SMP MAD content
        let mut dr = dr_smp_mad {
            m_key: 0,
            drslid: 0xffff,
            drdlid: 0xffff,
            reserved: [0; 28],
            attr_layout: [0; 64],
            initial_path: [0; 64],
            return_path: [0; 64],
        };
        dr.initial_path[0] = 0;
        dr.initial_path[1] = 1;

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

    fn sample_umad(attr_id: u16) -> ibmad::mad::ib_user_mad {
        sample_umad_attr(attr_id)
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
            let hca = ibmad::sim::Node::new_hca("host mlx5_0", 0x7ffc_0000_0000_0001);
            let hca_rc = fabric.add_hca(hca);

            let hca_port = Rc::new(
                RefCell::new(
                    ibmad::sim::Port::new_port(1, 1, hca_rc.clone())
                )
            );
                        
            fabric.dr_paths.insert(
                [0; 64], 
                Rc::downgrade(&hca_port)
            );

            let mut hca_ref = hca_rc.borrow_mut();
            hca_ref.ports.push(hca_port.clone());

            let switch = ibmad::sim::Node::new_switch("switch-0001", 0x7ffc_0000_0000_0001);

            let switch_rc = fabric.add_switch(switch);

            let mut switch_ref = switch_rc.borrow_mut();

            for i in 0..=65 {
                let port = Port::new_port(i, 100, switch_rc.clone());
                switch_ref.ports.push(
                    Rc::new(
                        RefCell::new(
                            port
                        )
                    )
                );
            }
            let sw_port_rc = &switch_ref.ports[0];

            let mut sw_port_ref = sw_port_rc.borrow_mut();

            sw_port_ref.remote_port = Some(
                Rc::downgrade(&hca_port)
            );

            let mut hca_port_ref = hca_port.borrow_mut();

            hca_port_ref.remote_port = Some(Rc::downgrade(&sw_port_rc));
        }

        let umad = sample_umad(0x0015);

        let r = client_file.write(&umad.to_bytes());

        let r = fabric.process_one_umad();

        let mut buf: [u8; 320] = [0; 320];
        let r = client_file.read(&mut buf);


    }
    
}