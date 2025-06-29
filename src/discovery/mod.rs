use std::{collections::{HashMap, HashSet, VecDeque}, io, sync::Weak, sync::{Arc, RwLock}, time};

use crate::{
    enums,
    mad::{self, ib_mad_addr, ib_user_mad, node_info, port_info, IbMadPort},
};

const START_PATH: [u8; 64] = [0; 64];

#[derive(Debug, Clone)]
pub struct Port {
    pub number: u8,
    pub link_state: enums::IbPortLinkLayerState,
    pub phys_state: enums::IbPortPhyState,
    pub remote_port: Option<Weak<RwLock<Port>>>,
    pub parent: Weak<RwLock<Node>>,
}

#[derive(Debug, Clone)]
pub struct Node{
    pub lid: u16,
    pub node_type: enums::IbNodeType,
    pub node_guid: u64,
    pub description: Option<String>,
    pub local_port: u8, // Port found during discovery
    pub nports: u8,
    pub ports: Vec<Arc<RwLock<Port>>>,
}

#[derive(Debug)]
pub struct Fabric {
    pub port: IbMadPort,
    pub node_map: HashMap<u64, Arc<RwLock<Node>>>,
    pub nodes: Vec<Arc<RwLock<Node>>>,
    pub switches: Vec<Weak<RwLock<Node>>>,
    pub hcas: Vec<Weak<RwLock<Node>>>,
    pub dr_paths: HashMap<[u8; 64], Weak<RwLock<Port>>>,
    pub ni_timings: Vec<time::Duration>,
    pub retries: u32,
    pub timeout: u32,
    pub mad_errors: u64,
    pub mad_timeouts: u64,
    pub mads_sent: u64,
    pub tid: u64,
}


fn lock_err<T: std::fmt::Debug>(e: T) -> io::Error {
    io::Error::new(io::ErrorKind::Other, format!("Lock poisoned: {:?}", e))
}

impl Fabric {

    fn format_path(path: &[u8; 64]) -> String {
        let mut hop_vec: Vec<String> = Vec::new();

        // The actual path starts at index 1.
        for &hop in path.iter().skip(1) {
            if hop == 0 {
                break;
            }
            hop_vec.push(hop.to_string());
        }
        if hop_vec.is_empty() {
            "Direct".to_string()
        } else {
            format!("0 -> {}", hop_vec.join(" -> "))
        }
    }
    
    fn get_hop_count(path: &[u8; 64]) -> u8 {
        path.iter().skip(1).take_while(|&&p| p != 0).count() as u8
    }

    fn build_umad(timeout: u32, retries: u32) -> mad::ib_user_mad {
        let umad = ib_user_mad{
            agent_id: 0x0,
            status: 0x0,
            timeout_ms: timeout,
            retries: retries,
            length: 0,
            addr: ib_mad_addr {
                qpn: 0,
                qkey: mad::IB_DEFAULT_QKEY.to_be(),
                lid: 0xffff,
                sl: 0,
                path_bits: 0,
                grh_present: 0,
                hop_limit: 63,
                gid_index: 0,
                traffic_class: 0,
                gid: [0; 16],
                flow_label: 0,
                pkey_index: 0,
                reserved: [0; 6],
            },
            data: [0; 256]

        };

        umad
    }

    fn build_mad(
        mgmt_class: u8,
        method: u8,
        attr_id: enums::SmiAttrID,
        attr_mod: u32,
        hop_cnt: u8,
        tid: u64,
    ) -> mad::ib_mad {
        let mad = mad::ib_mad{
            base_version: 0x1,
            mgmt_class: mgmt_class,
            method: method,
            class_version: 0x1,
            status: 0x0,
            hop_ptr: 0,
            hop_cnt: hop_cnt,
            tid: (tid as u64).to_be(),
            attr_id: (attr_id as u16).to_be(),
            additional_status: 0x0,
            attr_mod: attr_mod.to_be(),
            data: [0; 232],
        };

        return mad
    }

    fn build_dr_smp(path: [u8; 64]) -> mad::dr_smp_mad {
        let dr_smp = mad::dr_smp_mad{
            m_key: 0x0,
            drslid: 0xffff,
            drdlid: 0xffff,
            reserved: [0; 28],
            attr_layout: [0; 64],
            initial_path: path,
            return_path: [0; 64],
        };

        return dr_smp
    }

    fn build_dr_smp_umad( 
        path: [u8; 64], 
        attr_id: enums::SmiAttrID, 
        attr_mod: u32,
        hop_cnt: u8,
        tid : u64,
        timeout: u32,
        retries: u32,
     ) -> ib_user_mad {
        
        let mut dr_smp = Fabric::build_dr_smp(path);
        let mut mad = Fabric::build_mad(
            enums::MadClasses::DirecteRoute as u8,
            enums::Methods::Get as u8,
            attr_id, 
            attr_mod, 
            hop_cnt,
            tid,
        );
        let mut umad = Fabric::build_umad(
            timeout,
            retries,
        );

        dr_smp.initial_path = path;

        // Assemble the UMAD
        let dr_bytes = dr_smp.to_bytes();
        mad.data[..dr_bytes.len()].copy_from_slice(&dr_bytes);

        let mad_bytes = mad.to_bytes();
        umad.data[..mad_bytes.len()].copy_from_slice(&mad_bytes);

        umad
    }

    fn send_and_match_with_retries(
        &mut self,
        umad_to_send: ib_user_mad,
    ) -> Result<ib_user_mad, io::Error> {
        let retries = self.retries;
        let mut current_timeout = self.timeout;
        let backoff_factor = 2;
        let expected_tid = umad_to_send.get_tid()?;

        for attempt in 0..=retries {
            log::trace!(
                "-> Sending MAD with TID 0x{:X} (Attempt {}/{})",
                expected_tid,
                attempt + 1,
                retries + 1
            );
            if let Err(e) = mad::send(&mut self.port, &umad_to_send) {
                log::debug!(
                    "Fatal error sending MAD with TID 0x{:X}: {:?}",
                    expected_tid,
                    e
                );
                self.mad_errors += 1;
                return Err(io::Error::new(e.kind(), format!("Fatal send error: {}", e)));
            }

            if attempt == 0 {
                self.mads_sent += 1;
            }

            let deadline = time::Instant::now() + time::Duration::from_millis(current_timeout as u64);

            loop {
                let now = time::Instant::now();
                if now >= deadline {
                    log::debug!(
                        "Attempt {} timed out waiting for TID 0x{:X}",
                        attempt + 1,
                        expected_tid
                    );
                    self.mad_timeouts += 1;
                    break;
                }
                let remaining_time = (deadline - now).as_millis() as u32;
                let mut recv_umad = Fabric::build_umad(
                    self.timeout,
                    self.retries,
                );

                match mad::recv(&mut self.port, &mut recv_umad, remaining_time) {
                    Ok(_) => {
                        if umad_to_send.is_tid_equal(&recv_umad) {
                            log::trace!("<- Matched response for TID 0x{:X}", expected_tid);
                            return Ok(recv_umad);
                        } else {
                            log::trace!(
                                "Discarding mismatched TID. Expected 0x{:X}, got 0x{:X}",
                                expected_tid,
                                recv_umad.get_tid().unwrap_or(0)
                            );
                            continue;
                        }
                    }
                    Err(e) if e.kind() == io::ErrorKind::TimedOut => {
                        break;
                    }
                    Err(e) => {
                        log::warn!(
                            "Receive error on attempt {} for TID 0x{:X}: {:?}",
                            attempt + 1,
                            expected_tid,
                            e,
                        );
                        self.mad_errors += 1;
                        continue;
                    }
                }
            }
            current_timeout *= backoff_factor;
        }

        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            format!("All {} retries failed for TID {}", retries, expected_tid)
        ))
    }

    pub fn recv_smp(&mut self) -> Result<ib_user_mad, io::Error> {
        let mut umad = Fabric::build_umad(
            self.timeout,
            self.retries,
        );
        let _s = mad::recv(&mut self.port, &mut umad, self.timeout)?;

        Ok(umad)
    }

    pub fn discover_node(&mut self, path: [u8; 64], hop_cnt: u8) -> Result<Arc<RwLock<Node>>, io::Error> {
        let node_info = self.fetch_node_info(path, hop_cnt)?;

        let node_type = enums::IbNodeType::try_from(node_info.node_type)
            .map_err(|_e| io::Error::new(
                io::ErrorKind::InvalidData, 
                format!("invalid node_type: {}", node_info.node_type))
            )?;

        let mut node = Node {
            node_guid: node_info.node_guid,
            node_type,
            local_port: node_info.local_port,
            nports: node_info.nports,
            description: None,
            lid: 0,
            ports: Vec::with_capacity(node_info.nports as usize),
        };

        let node_desc = self.fetch_node_desc(path, hop_cnt)?;
        node.description = Some(node_desc);

        log::debug!(
            "Discovered Node: '{}' (GUID: 0x{:X}, Type: {:?}, Ports: {})",
            node.description.as_deref().unwrap_or("N/A"),
            node.node_guid.to_be(),
            node.node_type,
            node.nports
        );

        let nports = node.nports;
        let node_rc = Arc::new(RwLock::new(node));

        self.populate_node_ports(&node_rc, nports, path, hop_cnt)?;

        self.nodes.push(node_rc.clone());
        self.node_map.insert(node_info.node_guid, node_rc.clone());

        Ok(node_rc)
    }

    fn fetch_node_info(&mut self, path: [u8; 64], hop_cnt: u8) -> Result<node_info, io::Error> {
        let start_ts = time::Instant::now();
        log::debug!("Fetching NodeInfo for path: [{}]", Fabric::format_path(&path));

        let umad_to_send = Fabric::build_dr_smp_umad(
            path,
            enums::SmiAttrID::NodeInfo, 
            0x0, 
            hop_cnt, 
            self.tid,
            self.timeout,
            self.retries,
        ); 

        let recv_ni_umad = self.send_and_match_with_retries(umad_to_send)?;

        self.ni_timings.push(time::Instant::now() - start_ts);

        let ni = node_info::from_bytes(&recv_ni_umad.data[64..]).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "Could not parse NodeInfo data.")
        })?;
        
        log::trace!("<- Received NodeInfo: {:?}", ni);
        Ok(ni)
    }

    fn fetch_node_desc(&mut self, path: [u8; 64], hop_cnt: u8) -> Result<String, io::Error> {
        log::debug!("Fetching NodeDesc for path: [{}]", Fabric::format_path(&path));
        
        let umad_to_send = Fabric::build_dr_smp_umad(
            path,
            enums::SmiAttrID::NodeDesc, 
            0x0, 
            hop_cnt, 
            self.tid,
            self.timeout,
            self.retries,
        );

        let recv_nd_umad = self.send_and_match_with_retries(umad_to_send)?;

        let dr: &mad::dr_smp_mad = unsafe {
            &*(recv_nd_umad.data[mad::node::NODE_DESC_OFFSET..].as_ptr() as *const mad::dr_smp_mad)
        };

        let node_desc_bytes = &dr.attr_layout[..mad::node::NODE_DESC_LENGTH];
        let mut node_desc = String::from_utf8_lossy(node_desc_bytes).trim_end_matches('\0').to_string();

        if let Some(null_idx) = node_desc.find('\0') {
            node_desc.truncate(null_idx);
        }

        log::trace!("<- Received NodeDesc: '{}'", node_desc);
        Ok(node_desc)
    }

    fn fetch_port_info(&mut self, path: [u8; 64], port_num: u8, hop_cnt: u8) -> Result<Port, io::Error> {
        log::debug!(
            "Fetching PortInfo for port {} on path: [{}]",
            port_num,
            Fabric::format_path(&path)
        );

        let umad_to_send = Fabric::build_dr_smp_umad(
            path,
            enums::SmiAttrID::PortInfo, 
            port_num as u32, 
            hop_cnt, 
            self.tid,
            self.timeout,
            self.retries,
        );

        let recv_pi_umad = self.send_and_match_with_retries(umad_to_send)?;

        let pi = port_info::from_bytes(&recv_pi_umad.data[64..]).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "could not parse portinfo data.")
        })?;

        log::trace!("<- Received PortInfo for port {}: {:?} {} {}", port_num, pi, pi.port_state(), pi.port_physical_state());

        let link_state = enums::IbPortLinkLayerState::try_from(pi.port_state())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("invalid port_state: {:?}", e)))?;
        let phy_state = enums::IbPortPhyState::try_from(pi.port_physical_state())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("invalid port_physical_state: {:?}", e)))?;

        Ok(Port {
            number: port_num,
            link_state,
            phys_state: phy_state,
            parent: Weak::new(),
            remote_port: None,
        })
    }

    fn populate_node_ports(&mut self, 
        node_arc: &Arc<RwLock<Node>>, 
        num_ports: u8, 
        path: [u8; 64], 
        hop_cnt: u8
    ) -> Result<(), io::Error> {

        for p in 1..=num_ports {

            match self.fetch_port_info(path, p, hop_cnt) {
                Ok(port) => {
                    let mut node_ref = node_arc.write().map_err(|e| 
                        io::Error::new(io::ErrorKind::Deadlock, 
                format!("Could not access node rwlock: {:?}", e))
                    )?;

                    let port_arc = Arc::new(RwLock::new(port));

                    let mut port_ref = port_arc.write().map_err(|e| 
                        io::Error::new(io::ErrorKind::Deadlock, 
                format!("Could not access port rwlock: {:?}", e))
                    )?;

                    log::trace!(
                        "Successfully added Port {} ({:?}/{:?}) to node '{}'",
                        p,
                        port_ref.link_state,
                        port_ref.phys_state,
                        node_ref.description.as_deref().unwrap_or("N/A")
                    );
                    port_ref.parent = Arc::downgrade(&node_arc);
                    node_ref.ports.push(port_arc.clone());
                }
                Err(e) if e.kind() == io::ErrorKind::TimedOut => {
                    log::debug!("Timeout getting PortInfo for port {} on path [{}]", p, Fabric::format_path(&path));
                    self.mad_timeouts += 1;
                }
                Err(e) => {
                    log::error!("Error getting PortInfo for port {} on path [{}]: {}", p, Fabric::format_path(&path), e);
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    pub fn first_hop_discovery(&mut self) -> Result<(), io::Error> {

        let mut nodes: Vec<Arc<RwLock<Node>>> = Vec::new();
        let mut hop_cnt: u8 = 0;


        let first_node_arc = self.discover_node(START_PATH, hop_cnt)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, 
                format!("Could not discover first-hop node: {:?}", e)))?;

        nodes.push(first_node_arc.clone());

        hop_cnt += 1;

        let first_node = first_node_arc.read()
        .map_err(|e| 
            io::Error::new(io::ErrorKind::Deadlock, 
            format!("Could not access first-hop node: {:?}", e))
        )?;

        for port_arc in first_node.ports.iter() {
            let mut path: [u8; 64] = [0; 64];

            path[1] = port_arc.read()
            .map_err(|e| 
                io::Error::new(io::ErrorKind::Deadlock, 
            format!("Could not access port: {:?}", e))
            )?.number;
            
            log::debug!("Probing from local HCA Port {}", path[1]);
            match self.discover_node(path, hop_cnt) {
                Ok(node) => {
                    nodes.push(node);
                },
                Err(e) => {
                    log::error!("Could not discover neighbor on local HCA Port {}: {}", path[1], e);
                    continue;
                },
            }
        }

        log::debug!(" Discovered first-hop nodes: {:?}", nodes);

        Ok(())
    }

    pub fn seq_discover(&mut self) -> Result<(), io::Error> {
        let mut visited: HashSet<u64> = HashSet::new();
        let mut stack: VecDeque<(Arc<RwLock<Port>>, [u8; 64])> = VecDeque::new();
        let mut hop_cnt: u8 = 0;

        self.node_map.clear();
        self.nodes.clear();
        self.switches.clear();
        self.hcas.clear();
        self.dr_paths.clear();
        self.mad_errors = 0;
        self.mad_timeouts = 0;
        self.mads_sent = 0;

        let start_ts = time::Instant::now();

        let first_node_arc = self.discover_node(START_PATH, hop_cnt)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("Could not discover first-hop node: {:?}", e)))?;

        hop_cnt += 1;
        
        // Initial discovery from the first node's ports
        {
            let first_node = first_node_arc.read().map_err(lock_err)?;
            visited.insert(first_node.node_guid);

            for port_arc in first_node.ports.iter() {
                let mut path: [u8; 64] = [0; 64];
                path[1] = port_arc.read().map_err(lock_err)?.number;

                log::debug!("Probing from local HCA Port {}", path[1]);
                match self.discover_node(path, hop_cnt) {
                    Ok(neighbor_node_arc) => {
                        let neighbor_node = neighbor_node_arc.read().map_err(lock_err)?;
                        visited.insert(neighbor_node.node_guid);
                        
                        for neighbor_port_arc in neighbor_node.ports.iter() {
                            let neighbor_port = neighbor_port_arc.read().map_err(lock_err)?;
                            if neighbor_port.link_state == enums::IbPortLinkLayerState::Active {
                                log::debug!(
                                    "Adding switch port {}, state: {:?}, desc: ('{}') to discovery stack. Path: [{}]",
                                    neighbor_port.number,
                                    neighbor_port.link_state,
                                    neighbor_node.description.as_deref().unwrap_or("N/A"),
                                    Fabric::format_path(&path)
                                );
                                stack.push_front((neighbor_port_arc.clone(), path));
                            }
                        }
                    },
                    Err(e) => {
                        log::warn!("Could not discover neighbor on local HCA Port {}: {}", path[1], e);
                        continue;
                    },
                }
            }
        }

        log::debug!("Initial discovery stack size: {}", stack.len());

        while let Some((local_port_arc, path_to_local_node)) = stack.pop_front() {
            let (parent_desc, local_port_number) = {
                let local_port = local_port_arc.read().map_err(lock_err)?;
                let parent_arc = local_port.parent.upgrade().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Parent node not found"))?;
                let parent_node = parent_arc.read().map_err(lock_err)?;
                (parent_node.description.clone(), local_port.number)
            };

            log::trace!(
                "Popped Port {} on node '{}' (path: [{}]) from stack. (Stack size: {})",
                local_port_number,
                parent_desc.as_deref().unwrap_or("N/A"),
                Fabric::format_path(&path_to_local_node),
                stack.len()
            );

            // Check if remote port is already linked
            if local_port_arc.read().map_err(lock_err)?.remote_port.is_some() {
                log::trace!("Port {} already has a remote link, skipping.", local_port_number);
                continue;
            }

            let hop_cnt = Fabric::get_hop_count(&path_to_local_node) + 1;
            let mut path_to_remote_node = path_to_local_node;

            if (hop_cnt as usize) < path_to_remote_node.len() {
                path_to_remote_node[hop_cnt as usize] = local_port_number;
            } else {
                log::warn!("Path too long, cannot discover beyond port {}", local_port_number);
                continue;
            }

            let remote_node_info = match self.fetch_node_info(path_to_remote_node, hop_cnt) {
                Ok(ni) => ni,
                Err(e) if e.kind() == io::ErrorKind::TimedOut => {
                    let parent_arc = local_port_arc.read().map_err(lock_err)?.parent.upgrade().unwrap();
                    let parent_node = parent_arc.read().map_err(lock_err)?;
                    log::debug!(
                        "Port {} on node '{}' (0x{:X}) appears unconnected (timeout on path [{}]).",
                        local_port_number,
                        parent_node.description.as_deref().unwrap_or("N/A"),
                        parent_node.node_guid.to_be(),
                        Fabric::format_path(&path_to_remote_node),
                    );
                    continue;
                }
                Err(e) => return Err(e),
            };

            let node_guid = remote_node_info.node_guid;

            let remote_node_arc = if let Some(found_node) = self.node_map.get(&node_guid) {
                log::trace!("Remote node 0x{:X} already discovered.", remote_node_info.node_guid.to_be());
                found_node.clone()
            } else {
                log::debug!("Found new node with GUID: 0x{:X}", remote_node_info.node_guid.to_be());
                match self.discover_node(path_to_remote_node, hop_cnt) {
                    Ok(new_node_arc) => {
                        {
                        let node = new_node_arc.read().map_err(lock_err)?;
                        if node.node_type == enums::IbNodeType::Switch {
                            for port_arc in node.ports.iter().rev() {
                                let port = port_arc.read().map_err(lock_err)?;
                                if port.link_state == enums::IbPortLinkLayerState::Active {
                                    log::debug!(
                                        "Adding switch port {}, state: {:?}, desc: ('{}') to discovery stack. Path: [{}]",
                                        port.number,
                                        port.link_state,
                                        node.description.as_deref().unwrap_or("N/A"),
                                        Fabric::format_path(&path_to_remote_node)
                                    );
                                    stack.push_front((port_arc.clone(), path_to_remote_node));
                                }
                            }
                        }
                        }
                        new_node_arc
                    },
                    Err(e) => {
                        log::warn!("Failed to discover new remote node at path [{}]: {}",
                            Fabric::format_path(&path_to_remote_node), e
                        );
                        continue;
                    },
                }
            };

            let remote_port_number = remote_node_info.local_port;
            let remote_node_guard = remote_node_arc.read().map_err(lock_err)?;
            
            if let Some(remote_port_arc) = remote_node_guard.ports.iter().find(|p| p.read().map_or(false, |p_guard| p_guard.number == remote_port_number)) {
                let parent_arc = local_port_arc.read().map_err(lock_err)?.parent.upgrade().unwrap();
                let parent_guard = parent_arc.read().map_err(lock_err)?;
                
                log::trace!(
                    "Linking '{}' Port {} <--> '{}' Port {}",
                    parent_guard.description.as_deref().unwrap_or("N/A"),
                    local_port_number,
                    remote_node_guard.description.as_deref().unwrap_or("N/A"),
                    remote_port_number
                );
                
                // Perform the link
                local_port_arc.write().map_err(lock_err)?.remote_port = Some(Arc::downgrade(remote_port_arc));
                remote_port_arc.write().map_err(lock_err)?.remote_port = Some(Arc::downgrade(&local_port_arc));

            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "Inconsistent fabric: remote node 0x{:X} ('{}') reported port {} which was not found",
                        remote_node_guard.node_guid.to_be(),
                        remote_node_guard.description.as_deref().unwrap_or("N/A"),
                        remote_port_number
                    )
                ));
            }
        }

        // Final categorization
        for node_arc in &self.nodes {
            let node_type = &node_arc.read().map_err(lock_err)?.node_type;
            match node_type {
                enums::IbNodeType::Switch => self.switches.push(Arc::downgrade(node_arc)),
                _ => self.hcas.push(Arc::downgrade(node_arc)),
            }
        }

        let ts_diff = time::Instant::now() - start_ts;
        log::info!("Discovery complete. Found {} nodes ({} switches, {} HCAs).",
            self.nodes.len(), self.switches.len(), self.hcas.len());

        log::info!("MADs Sent: {}, Timeouts: {}, Errors: {}",
            self.mads_sent, self.mad_timeouts, self.mad_errors);
        
        let zero_duration = time::Duration::new(0, 0);
        if !self.ni_timings.is_empty() {
            let ni_time_min = self.ni_timings.iter().min().unwrap_or(&zero_duration);
            let ni_time_max = self.ni_timings.iter().max().unwrap_or(&zero_duration);
            let ni_time_sum: u128 = self.ni_timings.iter().map(|d| d.as_micros()).sum();
            let ni_avg = ni_time_sum / self.ni_timings.len() as u128;
            log::info!(
                "Discovery Duration: {:.2}s, NI RTT Avg: {}us, Max: {}us, Min: {}us",
                ts_diff.as_secs_f64(),
                ni_avg,
                ni_time_max.as_micros(),
                ni_time_min.as_micros()
            );
        } else {
            log::info!("Discovery Duration: {:.2}s. No NodeInfo timings were recorded.", ts_diff.as_secs_f64());
        }
        Ok(())
    }
}