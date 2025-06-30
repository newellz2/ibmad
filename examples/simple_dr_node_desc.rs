
fn main() {
    let result = ibmad::ca::get_cas();
 
    // Get the first CA
    if let Ok(cas) = result {
        let ib_ca = cas.first().unwrap();

        println!("CA: {:?}", ib_ca);

        match ibmad::mad::open_port(ib_ca) {
            Ok(mut port) => {
                if let Ok(agent_id) = ibmad::mad::register_agent(&mut port, 0x81) {

                    let mut dr = ibmad::mad::dr_smp_mad {
                        m_key: 0,
                        drslid: 0xffff,
                        drdlid: 0xffff,
                        reserved: [0; 28],
                        attr_layout: [0; 64],
                        initial_path: [0; 64],
                        return_path: [0; 64],
                    };

                    // First Hop Switch
                    dr.initial_path[0] = 0;
                    dr.initial_path[1] = 1;

                    // embed DR SMP into MAD payload
                    let mut mad = ibmad::mad::ib_mad {
                        base_version: 0x1,
                        mgmt_class: ibmad::mad::IB_MGMT_CLASS_DIRECT_ROUTED_SMP.to_be(),
                        class_version: 0x1,
                        method: (0x1 as u8).to_be(),
                        status: 0,
                        hop_ptr: 0,
                        hop_cnt: 1, // Second position in initial_path
                        tid: (0x11 as u64).to_be(),
                        attr_id: (0x0010 as u16).to_be(),
                        additional_status: 0x0000,
                        attr_mod: 0x0000_0000,
                        data: [0; 232],
                    };

                    let dr_bytes: &[u8] = unsafe {
                        std::slice::from_raw_parts(
                            &dr as *const ibmad::mad::dr_smp_mad as *const u8,
                            std::mem::size_of::<ibmad::mad::dr_smp_mad>(),
                        )
                    };
                    
                    mad.data[..dr_bytes.len()].copy_from_slice(dr_bytes);

                    let mut umad = ibmad::mad::ib_user_mad {
                        agent_id,
                        status: 0,
                        timeout_ms: 50,
                        retries: 1,
                        length: 0,
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

                    let ib_mad_bytes: &[u8] = unsafe {
                        std::slice::from_raw_parts(
                            &mad as *const ibmad::mad::ib_mad as *const u8,
                            std::mem::size_of::<ibmad::mad::ib_mad>(),
                        )
                    };
                    
                    umad.data[..ib_mad_bytes.len()].copy_from_slice(ib_mad_bytes);

                    let r = ibmad::mad::send(&mut port, &umad);
                    match r {
                        Ok(s) => {
                            println!("MAD Sent, size: {}", s)
                        },
                        Err(e) => {
                            eprintln!("Failed to send MAD: {}", e)
                        },
                    }

                    let _ = ibmad::mad::recv(&mut port, &mut umad, 1000);

                    let dr: &ibmad::mad::dr_smp_mad = unsafe {
                        &*(umad.data[24..].as_ptr() as *const ibmad::mad::dr_smp_mad)
                    };

                    let node_desc_bytes = &dr.attr_layout[..64];
                    let node_desc = String::from_utf8_lossy(node_desc_bytes);

                    println!("Response Received, NodeDesc: '{}'", node_desc);

                }
            },
            Err(e) => {
                eprintln!("Failed to open port: {}", e)
            },
        }
    }


}
