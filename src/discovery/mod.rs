use std::{cell::RefCell, collections::{HashMap, HashSet, VecDeque}, io, rc::{Rc, Weak}, time};

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
    pub remote_port: Option<Weak<RefCell<Port>>>,
    pub parent: Weak<RefCell<Node>>,
}

#[derive(Debug, Clone)]
pub struct Node{
    pub lid: u16,
    pub node_type: enums::IbNodeType,
    pub node_guid: u64,
    pub description: Option<String>,
    pub local_port: u8, // Port found during discovery
    pub nports: u8,
    pub ports: Vec<Rc<RefCell<Port>>>,
}

#[derive(Debug)]
pub struct Fabric {
    pub port: IbMadPort,
    pub node_map: HashMap<u64, Rc<RefCell<Node>>>,
    pub nodes: Vec<Rc<RefCell<Node>>>,
    pub switches: Vec<Weak<RefCell<Node>>>,
    pub hcas: Vec<Weak<RefCell<Node>>>,
    pub dr_paths: HashMap<[u8; 64], Weak<RefCell<Port>>>,
    pub ni_timings: Vec<time::Duration>,
    pub retries: u32,
    pub timeout: u32,
    pub mad_errors: u64,
    pub mad_timeouts: u64,
    pub mads_sent: u64,
    pub tid: u64,
}


impl Fabric {

    fn get_hop_count(&self, path: &[u8; 64]) -> u8 {
        path.iter().skip(1).take_while(|&&p| p != 0).count() as u8
    }

    fn build_umad(&self) -> mad::ib_user_mad {
        let umad = ib_user_mad{
            agent_id: 0x0,
            status: 0x0,
            timeout_ms: self.timeout,
            retries: self.retries,
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

    fn build_mad(&mut self, 
        attr_id: enums::SmiAttrID,
        attr_mod: u32,
        hop_cnt: u8,
    ) -> mad::ib_mad {
        let mad = mad::ib_mad{
            base_version: 0x1,
            mgmt_class: 0x81,
            method: 0x1,
            class_version: 0x1,
            status: 0x0,
            hop_ptr: 0,
            hop_cnt: hop_cnt,
            tid: (self.tid as u64).to_be(),
            attr_id: (attr_id as u16).to_be(),
            additional_status: 0x0,
            attr_mod: attr_mod.to_be(),
            data: [0; 232],
        };

        self.tid += 1;

        return mad
    }

    fn build_dr_smp(&self, path: [u8; 64]) -> mad::dr_smp_mad {
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

    fn build_dr_smp_umad(&mut self, 
        path: [u8; 64], 
        attr_id: enums::SmiAttrID, 
        attr_mod: u32,
        hop_cnt: u8,
     ) -> ib_user_mad {
        
        let mut dr_smp = self.build_dr_smp(path);
        let mut mad = self.build_mad(attr_id, attr_mod, hop_cnt);
        let mut umad = self.build_umad();

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
        umad_to_send: ib_user_mad, // We take ownership to allow resending
    ) -> Result<ib_user_mad, io::Error> {
        let retries = self.retries; // Or make this configurable on the Fabric struct
        let mut current_timeout = self.timeout; // Initial timeout in ms
        let backoff_factor = 2; // e.g., 100ms -> 200ms -> 400ms

        for attempt in 0..=retries {
            // Send MAD
            if let Err(e) = mad::send(&mut self.port, &umad_to_send) {
                log::debug!(
                    "Attempt {} encounted an error while sending MAD: {:?}",
                    attempt + 1,
                    e
                );
                self.mad_errors += 1;
                return Err(io::Error::new(e.kind(), format!("Fatal send error: {}", e)));
            }

            if attempt == 0 { // Only count the first send
                self.mads_sent += 1;
            }

            // --- Wait for a matching response ---
            let expected_tid = umad_to_send.get_tid()?;
            let deadline = time::Instant::now() + time::Duration::from_millis(current_timeout as u64);

            loop {
                let now = time::Instant::now();
                if now >= deadline {
                    // This attempt timed out. Break the inner loop to trigger a retry.
                    log::debug!(
                        "Attempt {} timed out waiting for TID 0x{:x}",
                        attempt + 1,
                        expected_tid
                    );
                    self.mad_timeouts += 1;
                    break; // Exit the loop to go to the next retry attempt
                }
                let remaining_time = (deadline - now).as_millis() as u32;
                let mut recv_umad = self.build_umad();

                match mad::recv(&mut self.port, &mut recv_umad, remaining_time) {
                    Ok(_) => {

                        if umad_to_send.is_tid_equal(&recv_umad) {
                            return Ok(recv_umad);
                        } else {
                            log::trace!("Discarding mismatched TID, expected 0x{:x}, got {:?}", expected_tid, recv_umad.get_tid());
                            continue;
                        }

                    }
                    Err(e) if e.kind() == io::ErrorKind::TimedOut => {
                        log::debug!(
                            "Inner recv for attempt {} timed out waiting for TID 0x{:x}",
                            attempt + 1,
                            expected_tid
                        );
                        self.mad_timeouts += 1;
                        break;
                    }
                    Err(e) => {
                        log::debug!(
                            "Inner recv for attempt {} encountered an error waiting for TID 0x{:x}, error: {:?}",
                            attempt + 1,
                            expected_tid,
                            e,
                        );
                        self.mad_errors += 1;
                        break;
                    }
                }
            }

            current_timeout *= backoff_factor;
        }

        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            format!("All {} retries failed for TID {}", retries, umad_to_send.get_tid()?)
        ))
    }

    pub fn recv_smp(&mut self) -> Result<ib_user_mad, io::Error> {
        let mut umad = self.build_umad();
        let _s = mad::recv(&mut self.port, &mut umad, self.timeout)?;

        Ok(umad)
    }

    pub fn discover_node(&mut self, path: [u8; 64], hop_cnt: u8) -> Result<Rc<RefCell<Node>>, io::Error> {
        // 1. Fetch node_info, create a Node
        let node_info = self.fetch_node_info(path, hop_cnt)?;

        let node_type = enums::IbNodeType::try_from(node_info.node_type)
            .map_err(|_e| io::Error::new(
                io::ErrorKind::InvalidData, 
                format!("invalid node_type: {}", node_info.node_type))
            )?;

        let mut node = Node {
            node_guid: node_info.node_guid,
            node_type: node_type,
            local_port: node_info.local_port,
            nports: node_info.nports,
            description: None,
            lid: 0,
            ports: Vec::with_capacity(node_info.nports as usize),
        };

        // 2. Fetch the NodeDescription and update the Node object.
        let nodedesc = self.fetch_node_desc(path, hop_cnt)?;

        node.description = Some(nodedesc);
        let nports = node.nports;

        // 3. Wrap the node in Rc<RefCell> for shared ownership.
        let node_rc: Rc<RefCell<Node>> = Rc::new(RefCell::new(node));

        // 4. Discover and populate all ports for the node.
        self.populate_node_ports(&node_rc, nports, path, hop_cnt)?;

        // 5. Add the fully populated node to our list.
        log::debug!("adding node: {:?}", node_rc);
        self.nodes.push(node_rc.clone());
        self.node_map.insert(node_info.node_guid, node_rc.clone()); // <-- ADD THIS LINE

        Ok(node_rc)
    }

    fn fetch_node_info(&mut self, path: [u8; 64], hop_cnt: u8) -> Result<node_info, io::Error> {
        let start_ts = time::Instant::now();

        let umad_to_send = self.build_dr_smp_umad(path, enums::SmiAttrID::NodeInfo, 0x0, hop_cnt);
        let recv_ni_umad = self.send_and_match_with_retries(umad_to_send)?;

        let end_ts = time::Instant::now();

        let ts_diff = end_ts - start_ts;

        self.ni_timings.push(ts_diff);

        let ni = node_info::from_bytes(&recv_ni_umad.data[64..]).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "Could not parse NodeInfo data.")
        })?;

        Ok(ni)
    }

    fn fetch_node_desc(&mut self, path: [u8; 64], hop_cnt: u8) -> Result<String, io::Error> {
        let umad_to_send = self.build_dr_smp_umad(path, enums::SmiAttrID::NodeDesc, 0x0, hop_cnt);
        let recv_nd_umad = self.send_and_match_with_retries(umad_to_send)?;

        let dr: &mad::dr_smp_mad = unsafe {
            &*(recv_nd_umad.data[mad::node::NODE_DESC_OFFSET..].as_ptr() as *const mad::dr_smp_mad)
        };

        let node_desc_bytes = &dr.attr_layout[..mad::node::NODE_DESC_LENGTH];
        let mut node_desc = String::from_utf8_lossy(node_desc_bytes).trim_end_matches('\0').to_string();

        let r = node_desc.find('\0');
        if let Some(null_idx) = r {
            node_desc = node_desc[0..null_idx].to_string();
        }

        Ok(node_desc)
    }

    fn fetch_port_info(&mut self, path: [u8; 64], port_num: u8, hop_cnt: u8) -> Result<Port, io::Error> {
        let umad_to_send = self.build_dr_smp_umad(path, enums::SmiAttrID::PortInfo, port_num as u32, hop_cnt);
        let recv_pi_umad = self.send_and_match_with_retries(umad_to_send)?;

        log::debug!("recv portinfo from path: {:?}, umad: {:?}", path, recv_pi_umad);

        let pi = port_info::from_bytes(&recv_pi_umad.data[64..]).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "could not parse portinfo data.")
        })?;

        let link_state = enums::IbPortLinkLayerState::try_from(pi.port_state())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("invalid port_state: {:?}", e)))?;

        log::debug!("Port physical state: {}", pi.port_physical_state());
        let phy_state = enums::IbPortPhyState::try_from(pi.port_physical_state())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("invalid port_physical_state: {:?}", e)))?;

        Ok(Port {
            number: port_num,
            link_state,
            phys_state: phy_state,
            parent: std::rc::Weak::new(), // Parent is set by the caller.
            remote_port: None,
        })
    }

    fn populate_node_ports(&mut self, node_rc: &Rc<RefCell<Node>>, num_ports: u8, path: [u8; 64], hop_cnt: u8) -> Result<(), io::Error> {
        for p in 1..=num_ports {
            log::debug!("send portinfo mad to port: {}", p);
            match self.fetch_port_info(path, p, hop_cnt) {
                Ok(port) => {
                    let mut node_ref = node_rc.borrow_mut();
                    let port_rc = Rc::new(RefCell::new(port));
                    port_rc.borrow_mut().parent = Rc::downgrade(node_rc);
                    node_ref.ports.push(port_rc);
                }
                Err(e) if e.kind() == io::ErrorKind::TimedOut => {
                    log::debug!("timeout reading portinfo for port {}", p);
                    self.mad_timeouts += 1;
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    pub fn discover(&mut self) -> Result<(), io::Error> {
        let mut visited: HashSet<u64> = HashSet::new();
        let mut stack: VecDeque<(Rc<RefCell<Port>>, [u8; 64])> = VecDeque::new();
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

        // first-hop, the hca performing the discovery
        let first_node = self.discover_node(START_PATH, hop_cnt)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("could not discover first-hop node: {:?}", e)))?;

        // next hop(s) - should only be one port
        hop_cnt +=1;
        let node_ref = first_node.borrow();

        visited.insert(node_ref.node_guid);

        for port_rc in node_ref.ports.iter(){
            let mut path: [u8; 64] = [0; 64];
            let port_ref = port_rc.borrow();
            path[1] = port_ref.number;
            match self.discover_node(path, hop_cnt) {
                Ok(node) => {
                    log::debug!("found node connected to node: {:?}", node);

                    let node_ref = node.borrow();

                    visited.insert(node_ref.node_guid);

                    for nh_p in node_ref.ports.iter(){
                        let port_ref = nh_p.borrow();
                        if port_ref.link_state == enums::IbPortLinkLayerState::Active || 
                            port_ref.link_state == enums::IbPortLinkLayerState::Init {
                            stack.push_front(
                                (nh_p.clone(), path)
                            );
                        }
                    }
                },
                Err(e) => {
                    log::debug!("Error discovering node on path: {:?}, error: {:?}", path, e);
                    continue;
                },
            }
        }

        log::debug!("path stack length: {}", stack.len());

        while let Some((local_port_rc, path_to_local_node)) = stack.pop_front() {

            let mut local_port_ref = local_port_rc.borrow_mut();

            // Check if this port has already been connected
            if local_port_ref.remote_port.is_some() {
                continue;
            }

            // Build the DR path
            let hop_cnt = self.get_hop_count(&path_to_local_node) + 1;

            let mut path_to_remote_node = path_to_local_node;

            if (hop_cnt as usize) < path_to_remote_node.len() {
                path_to_remote_node[hop_cnt as usize] = local_port_ref.number;
            } else {
                log::debug!("Path too long, cannot discover beyond port {}", local_port_ref.number);
                continue;
            }

            let remote_hop_cnt = hop_cnt;

            let remote_node_info = match self.fetch_node_info(path_to_remote_node, remote_hop_cnt) {
                Ok(ni) => {
                    ni
                },
                Err(e) if e.kind() == io::ErrorKind::TimedOut => {
                    if let Some(parent_rc) = local_port_ref.parent.upgrade() {
                        let parent_ref = parent_rc.borrow();
                        log::debug!(
                            "Port {} on node 0x{:x}, {:?} is not connected (timeout).",
                            local_port_ref.number,
                            parent_ref.node_guid.to_be(),
                            parent_ref.description,
                        );
                    } else {
                        log::debug!("Port {} has no parent, cannot log full info.", local_port_rc.borrow().number);
                    }
                    continue;
                }
                Err(e) => return Err(e),
            };

            // Find or discover the full remote node.
            let node_guid = remote_node_info.node_guid;
            let remote_node_port = remote_node_info.local_port;

            let remote_node_rc = if let Some(found_node) = self.node_map.get(&node_guid) {
                // Node previously found
                found_node.clone()
            } else {
                // New node, discover it
                log::debug!("Found new node with GUID: 0x{:x}", remote_node_info.node_guid.to_be());
                log::debug!("New node, path: {:?}, remote_hop_cnt: {}", path_to_remote_node, remote_hop_cnt);

                visited.insert(remote_node_info.node_guid);
                
                match self.discover_node(path_to_remote_node, remote_hop_cnt){
                    Ok(node_rc) => {
                        {
                            let node_ref = node_rc.borrow();

                            // Add the node to the stack if it is a switch
                            // HCA's have 1-4 ports
                            for port_rc in node_ref.ports.iter().rev() {

                                if node_ref.node_type == enums::IbNodeType::Switch &&
                                    (
                                    port_rc.borrow().link_state == enums::IbPortLinkLayerState::Active || 
                                    port_rc.borrow().link_state == enums::IbPortLinkLayerState::Init
                                    ) 
                                {
                                    let entry = (port_rc.clone(), path_to_remote_node);
                                    log::debug!("Adding entry to stack: {:?}", entry);
                                    stack.push_front(entry);
                                }
                            }
                        }
                        node_rc
                    },
                    Err(e) => {
                        log::debug!("Error discovering remote node on path: {:?}, error: {:?}", path_to_remote_node, e);
                        continue
                    },
                }

            };

            // Assign remote_port to local_port.remote_port
            let remote_node_ref = remote_node_rc.borrow();

            // Look for the local_port returned in the node_info
            if let Some(remote_port_rc) = remote_node_ref.ports.iter().find(
                |p| p.borrow().number == remote_node_port
            ) {

                local_port_ref.remote_port = Some(
                    Rc::downgrade(remote_port_rc)
                );

                let mut remote_port_ref = remote_port_rc.borrow_mut();
                remote_port_ref.remote_port = Some(
                    Rc::downgrade(&local_port_rc)
                );

            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "Inconsistent fabric: remote node 0x{:x} reported port {} which was not found", 
                    remote_node_ref.node_guid, 
                    remote_node_port
                )
                ));
            }


        }

        // Add nodes to the collections
        for node_rc in &self.nodes {
            match node_rc.borrow().node_type {
                enums::IbNodeType::Switch => self.switches.push(Rc::downgrade(node_rc)),
                _ => self.hcas.push(Rc::downgrade(node_rc)),
            }
        }


        let end_ts = time::Instant::now();
        let ts_diff = end_ts - start_ts;

        log::info!("Discovery complete. Found {} nodes ({} switches, {} HCAs).", self.nodes.len(), self.switches.len(), self.hcas.len());
        log::info!("MADs Sent: {}, Timeouts: {}, Errors: {}", self.mads_sent, self.mad_timeouts, self.mad_errors);
        let zero_duration = time::Duration::new(0, 0);
        if !self.ni_timings.is_empty() { // <-- ADD THIS GUARD
            let ni_time_min = self.ni_timings.iter().min().unwrap_or(&zero_duration);
            let ni_time_max = self.ni_timings.iter().max().unwrap_or(&zero_duration);
            let ni_time_sum: u128 = self.ni_timings.iter().map(|d| d.as_micros()).sum();
            let ni_avg = ni_time_sum / self.ni_timings.len() as u128;
            log::info!("Discovery Duration: {}s, NI Avg: {}us, Max: {}us, Min: {}us",
                ts_diff.as_secs_f64(),
                ni_avg,
                ni_time_max.as_micros(),
                ni_time_min.as_micros()
            );
        } else {
            log::info!("Discovery Duration: {}s. No NodeInfo timings were recorded.", ts_diff.as_secs_f64());
        }
        Ok(())
    }
}