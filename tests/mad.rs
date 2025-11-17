#[cfg(test)]
mod mad_tests {
    use ibmad::enums::IbPortLinkLayerState;
    use ibmad::mad::{
        self, IB_DEFAULT_QKEY, IB_MGMT_CLASS_PERFORMANCE, open_port, open_smp_port, register_agent,
    };
    use std::path::Path;

    #[test]
    fn send_nodedesc_success() {
        let _ = env_logger::try_init();
        if !Path::new("/dev/infiniband/umad0").exists() {
            eprintln!("UMAD device not found, skipping test");
            return;
        }

        match ibmad::ca::get_cas() {
            Ok(cas) => {
                assert!(cas.len() > 0, "No CAs found.");
                let hca = &cas[0];
                match open_smp_port(hca) {
                    Ok(mut port) => {
                        log::debug!("tests - send_success - Opened IB MAD Port: {:?}", port);
                        if let Ok(agent_id) = register_agent(&mut port, 0x81) {
                            log::debug!("tests - send_success - Registered agent: {}", agent_id);

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
                                    qkey: IB_DEFAULT_QKEY.to_be(),
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

                            log::debug!("tests - send_success -  Sending MAD: {:?}", umad);
                            let r = ibmad::mad::send(&mut port, &umad);
                            match r {
                                Ok(s) => {
                                    log::debug!(
                                        "tests - send_success -  Sending Successful, size: {:?}",
                                        s
                                    );
                                }
                                Err(e) => {
                                    log::debug!(
                                        "tests - send_success -  Sending MAD Failed: {:?}",
                                        e
                                    );
                                }
                            }

                            let _ = ibmad::mad::recv(&mut port, &mut umad, 1000);
                        }
                    }
                    Err(e) => {
                        assert!(false, "{}", format!("Error opening port: {:?}", e));
                    }
                }
            }
            Err(e) => {
                assert!(false, "{}", format!("Error finding CAs: {:?}", e));
            }
        }
    }

    #[test]
    fn send_node_info_success() {
        let _ = env_logger::try_init();
        if !Path::new("/dev/infiniband/umad0").exists() {
            eprintln!("UMAD device not found, skipping test");
            return;
        }

        match ibmad::ca::get_cas() {
            Ok(cas) => {
                assert!(cas.len() > 0, "No CAs found.");
                let hca = &cas[0];
                match open_smp_port(hca) {
                    Ok(mut port) => {
                        log::debug!(
                            "tests - send_nodeinfo_success - Opened IB MAD Port: {:?}",
                            port
                        );
                        if let Ok(agent_id) = register_agent(&mut port, 0x81) {
                            log::debug!(
                                "tests - send_nodeinfo_success - Registered agent: {}",
                                agent_id
                            );

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
                                attr_id: (0x0011 as u16).to_be(),
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
                                    qkey: IB_DEFAULT_QKEY.to_be(),
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

                            log::debug!("tests - send_nodeinfo_success -  Sending MAD: {:?}", umad);
                            let r = ibmad::mad::send(&mut port, &umad);
                            match r {
                                Ok(s) => {
                                    log::debug!(
                                        "tests - send_nodeinfo_success -  Sending Successful, size: {:?}",
                                        s
                                    );
                                }
                                Err(e) => {
                                    log::debug!(
                                        "tests - send_nodeinfo_success -  Sending MAD Failed: {:?}",
                                        e
                                    );
                                }
                            }

                            let _ = ibmad::mad::recv(&mut port, &mut umad, 1000);

                            let umad_bytes: &[u8] = unsafe {
                                std::slice::from_raw_parts(
                                    &umad as *const ibmad::mad::ib_user_mad as *const u8,
                                    std::mem::size_of::<ibmad::mad::ib_user_mad>(),
                                )
                            };

                            let mut ni = ibmad::mad::node_info::default();
                            let ni_ptr = &mut ni as *mut ibmad::mad::node_info as *mut u8;

                            unsafe {
                                std::ptr::copy_nonoverlapping(
                                    umad_bytes[128..=192].as_ptr(),
                                    ni_ptr,
                                    std::mem::size_of::<[u8; 64 as usize]>(),
                                );
                            };

                            log::debug!("tests - send_nodeinfo_success -  NodeInfo: {:?}", ni);
                            log::debug!(
                                "tests - send_nodeinfo_success -  NodeInfo.system_guid: {:x}",
                                ni.system_guid.to_be()
                            );
                            log::debug!(
                                "tests - send_nodeinfo_success -  NodeInfo.device_id: {}",
                                ni.device_id.to_be()
                            );
                            log::debug!(
                                "tests - send_nodeinfo_success -  NodeInfo.revision: {:x}",
                                ni.revision.to_be()
                            );
                        }
                    }
                    Err(e) => {
                        assert!(false, "{}", format!("Error opening port: {:?}", e));
                    }
                }
            }
            Err(e) => {
                assert!(false, "{}", format!("Error finding CAs: {:?}", e));
            }
        }
    }

    #[test]
    fn get_port_counters_extended_success() {
        let _ = env_logger::try_init();
        if !Path::new("/dev/infiniband/umad0").exists() {
            eprintln!("UMAD device not found, skipping test");
            return;
        }

        let ca = match ibmad::ca::get_ca("mlx5_0") {
            Ok(ca) => ca,
            Err(e) => {
                eprintln!("Failed to enumerate CAs: {:?}", e);
                return;
            }
        };

        let hca = &ca;
        let port_info = match hca
            .ports
            .iter()
            .find(|p| p.lid != 0 && p.state == IbPortLinkLayerState::Active)
        {
            Some(port) => port,
            None => {
                eprintln!("No active HCA ports with LID found, skipping test");
                return;
            }
        };

        let mut port = match open_port(hca) {
            Ok(port) => port,
            Err(e) => {
                assert!(false, "Error opening port: {:?}", e);
                return;
            }
        };

        log::debug!(
            "tests - get_port_counters_extended_success - Opened IB MAD Port: {:?}",
            port
        );

        let agent_id = match register_agent(&mut port, IB_MGMT_CLASS_PERFORMANCE) {
            Ok(id) => id,
            Err(e) => {
                assert!(false, "Failed to register performance agent: {:?}", e);
                return;
            }
        };

        let perf_response = match mad::query_port_counters_extended(
            &mut port,
            agent_id,
            1000,
            1,
            port_info.lid as u16,
            port_info.number as u8,
        ) {
            Ok(resp) => resp,
            Err(e) => {
                assert!(false, "Failed to query PortCountersExtended: {:?}", e);
                return;
            }
        };

        assert_eq!(
            perf_response.port_select(),
            port_info.number as u8,
            "PortSelect in response did not match requested port"
        );

        log::debug!("PortCountersExtended:");
        log::debug!("  PortSelect: {}", perf_response.port_select());
        log::debug!("  CounterSelect: 0x{:04x}", perf_response.counter_select());
        log::debug!("  PortXmitData: {}", perf_response.port_xmit_data());
        log::debug!("  PortRcvData: {}", perf_response.port_rcv_data());
        log::debug!("  PortXmitPkts: {}", perf_response.port_xmit_pkts());
        log::debug!("  PortRcvPkts: {}", perf_response.port_rcv_pkts());
        log::debug!(
            "  PortUnicastXmitPkts: {}",
            perf_response.port_unicast_xmit_pkts()
        );
        log::debug!(
            "  PortUnicastRcvPkts: {}",
            perf_response.port_unicast_rcv_pkts()
        );
        log::debug!(
            "  PortMulticastXmitPkts: {}",
            perf_response.port_multicast_xmit_pkts()
        );
        log::debug!(
            "  PortMulticastRcvPkts: {}",
            perf_response.port_multicast_rcv_pkts()
        );
        log::debug!(
            "  CounterSelect2: 0x{:08x}",
            perf_response.counter_select2()
        );
        log::debug!(
            "  SymbolErrorCounter: {}",
            perf_response.symbol_error_counter()
        );
        log::debug!(
            "  LinkErrorRecoveryCounter: {}",
            perf_response.link_error_recovery_counter()
        );
        log::debug!(
            "  LinkDownedCounter: {}",
            perf_response.link_downed_counter()
        );
        log::debug!("  PortRcvErrors: {}", perf_response.port_rcv_errors());
        log::debug!(
            "  PortRcvRemotePhysicalErrors: {}",
            perf_response.port_rcv_remote_physical_errors()
        );
        log::debug!(
            "  PortRcvSwitchRelayErrors: {}",
            perf_response.port_rcv_switch_relay_errors()
        );
        log::debug!("  PortXmitDiscards: {}", perf_response.port_xmit_discards());
        log::debug!(
            "  PortXmitConstraintErrors: {}",
            perf_response.port_xmit_constraint_errors()
        );
        log::debug!(
            "  PortRcvConstraintErrors: {}",
            perf_response.port_rcv_constraint_errors()
        );
        log::debug!(
            "  LocalLinkIntegrityErrors: {}",
            perf_response.local_link_integrity_errors()
        );
        log::debug!(
            "  ExcessiveBufferOverrunErrors: {}",
            perf_response.excessive_buffer_overrun_errors()
        );
        log::debug!("  VL15Dropped: {}", perf_response.vl15_dropped());
        log::debug!("  PortXmitWait: {}", perf_response.port_xmit_wait());
        log::debug!("  QP1Dropped: {}", perf_response.qp1_dropped());
    }
}
