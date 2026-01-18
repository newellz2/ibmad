use std::{
    cell::RefCell,
    collections::HashMap,
    fs,
    io::{self, Read, Write},
    rc::{Rc, Weak},
    sync, time,
};

use crate::mad::{self, ib_mad, ib_user_mad, node_info, port_info};

const MIN_UMAD_SIZE: usize = 320;
const FIRST_HOP: [u8; 64] = [0; 64];

#[derive(Debug, Clone)]
pub struct Port {
    pub num: u8,
    pub port_info: mad::port_info,
    pub remote_port: Option<Weak<RefCell<Port>>>,
    pub parent: Weak<RefCell<Node>>,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub description: String,
    pub node_info: mad::node_info,
    pub ports: Vec<Rc<RefCell<Port>>>,
    pub lid: u16, // Cache LID for easier lookup
}

#[derive(Debug)]
pub struct Fabric {
    pub file: fs::File,
    pub nodes: Vec<Rc<RefCell<Node>>>,
    pub switches: Vec<Weak<RefCell<Node>>>,
    pub hcas: Vec<Weak<RefCell<Node>>>,
    pub dr_paths: HashMap<[u8; 64], Weak<RefCell<Port>>>,
    pub response_delay: Option<u64>,
}

pub fn connect_ports(port_a_rc: &Rc<RefCell<Port>>, port_b_rc: &Rc<RefCell<Port>>) {
    let mut port_a = port_a_rc.borrow_mut();
    let mut port_b = port_b_rc.borrow_mut();

    // Link the ports to each other
    port_a.remote_port = Some(Rc::downgrade(port_b_rc));
    port_b.remote_port = Some(Rc::downgrade(port_a_rc));

    // Set port states to ACTIVE and LINK_UP now that they are connected
    port_a.port_info.set_port_state(4); // ACTIVE
    port_a.port_info.set_port_physical_state(5); // LINK_UP
    port_a.port_info.set_link_speed_active(1); // Set active link params
    port_a.port_info.set_link_width_active(1);

    port_b.port_info.set_port_state(4); // ACTIVE
    port_b.port_info.set_port_physical_state(5); // LINK_UP
    port_b.port_info.set_link_speed_active(1); // Set active link params
    port_b.port_info.set_link_width_active(1);

    log::info!(
        "Connected port {} on node '{}' to port {} on node '{}'",
        port_a.num,
        port_a
            .parent
            .upgrade()
            .map_or("?".to_string(), |p| p.borrow().description.clone()),
        port_b.num,
        port_b
            .parent
            .upgrade()
            .map_or("?".to_string(), |p| p.borrow().description.clone())
    );
}

pub fn build_standard_fabric(fabric: &mut Fabric) {
    // build sixteen spine switches
    let mut spines = Vec::new();
    let mut lid = 2000; // Spines start at 2000

    for spine_idx in 0..16 {
        let spine = Node::new_switch(
            &format!("spine-{}", spine_idx),
            0x7ffc_0000_0000_1000 + spine_idx as u64,
        );
        let spine_rc = fabric.add_switch(spine);

        {
            let mut spine_ref = spine_rc.borrow_mut();
            spine_ref.lid = lid;
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
        let leaf = Node::new_switch(
            &format!("leaf-{}", leaf_idx),
            0x7ffc_0000_0000_2000 + leaf_idx as u64,
        );

        let leaf_rc = fabric.add_switch(leaf);

        {
            let mut leaf_ref = leaf_rc.borrow_mut();
            leaf_ref.lid = lid;
            for i in 0..=65 {
                let port = Port::new_port(i as u8, lid, leaf_rc.clone());
                log::trace!(
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
        // Simplified connection logic from tests
        for i in 0..32 {
            for (spine_idx, spine_rc) in spines.iter().enumerate() {
                // Determine ports to connect
                // This logic mirrors the test logic but ensures safe indexing
                // Spine ports used: 1..33 (roughly)
                // Leaf ports used: 33..65 (roughly)
                
                let spine_port_idx = leaf_idx + 1 + i;
                if spine_port_idx >= 65 { continue; } // Safety check

                let spine_port_rc = {
                    let spine_ref = spine_rc.borrow();
                    if spine_port_idx < spine_ref.ports.len() {
                        spine_ref.ports[spine_port_idx].clone()
                    } else {
                        continue;
                    }
                };

                // Leaf ports for uplinks usually start after HCA ports
                // 32 HCAs on 1-32. Uplinks on 33+.
                let leaf_port_idx = 33 + spine_idx + (i / 2); 
                if leaf_port_idx >= 65 { continue; }

                // Now we get an immutable borrow which is fine.
                let leaf_port_rc = {
                    let leaf_ref = leaf_rc.borrow();
                    if leaf_port_idx < leaf_ref.ports.len() {
                        leaf_ref.ports[leaf_port_idx].clone()
                    } else {
                        continue;
                    }
                };

                connect_ports(&spine_port_rc, &leaf_port_rc);
            }
        }

        // each leaf hosts thirty two HCAs on ports 1-32
        for h in 0..32 {
            hca_count += 1;
            let hca = Node::new_hca(
                &format!("host{:04}", hca_count),
                0x7ffc_0000_0000_3000 + hca_count as u64,
            );
            let hca_rc = fabric.add_hca(hca);
            
            // Assign LID to HCA (simplification: sequential LIDs starting after switches)
            let hca_lid = 4000 + hca_count as u16;
            hca_rc.borrow_mut().lid = hca_lid;

            let hca_port = Rc::new(RefCell::new(Port::new_port(
                1,
                hca_lid,
                hca_rc.clone(),
            )));
            hca_rc.borrow_mut().ports.push(hca_port.clone());

            // connect HCA to leaf
            let leaf_hca_port_rc = leaf_rc.borrow().ports[h + 1].clone();

            connect_ports(&leaf_hca_port_rc, &hca_port);

            // first HCA becomes the first hop in dr_paths
            if hca_count == 1 {
                fabric.dr_paths.insert([0; 64], Rc::downgrade(&hca_port));
            }
        }
    }
}

impl Fabric {
    pub fn new(file: fs::File) -> Self {
        Fabric {
            file: file,
            nodes: Vec::new(),
            switches: Vec::new(),
            hcas: Vec::new(),
            dr_paths: HashMap::new(),
            response_delay: None,
        }
    }

    pub fn add_switch(&mut self, switch: Node) -> Rc<RefCell<Node>> {
        let hca_switch_rc = Rc::new(RefCell::new(switch));
        self.switches.push(Rc::downgrade(&hca_switch_rc));

        self.nodes.push(hca_switch_rc.clone());

        return hca_switch_rc.clone();
    }

    pub fn add_hca(&mut self, hca: Node) -> Rc<RefCell<Node>> {
        let hca_rc = Rc::new(RefCell::new(hca));
        self.hcas.push(Rc::downgrade(&hca_rc));

        self.nodes.push(hca_rc.clone());

        return hca_rc.clone();
    }

    fn send_dr_response(
        &mut self,
        tid: u64,
        umad: &ib_user_mad,
        mad: &ib_mad,
        dr_smp: &mad::dr_smp_mad,
        attr_data: &[u8],
    ) -> Result<(), io::Error> {
        if let Some(max_delay) = self.response_delay {
            if max_delay > 0 {
                let delay = rand::random_range(0..=max_delay);
                log::trace!("[tid: {}] Delaying response by {}ms", tid, delay);
                std::thread::sleep(time::Duration::from_micros(delay));
            }
        }

        let mut resp_umad = umad.clone();
        let mut resp_mad = *mad;
        let mut resp_dr = *dr_smp;

        resp_dr.attr_layout[..attr_data.len()].copy_from_slice(attr_data);
        let dr_bytes = resp_dr.to_bytes();
        resp_mad.data[..dr_bytes.len()].copy_from_slice(&dr_bytes);
        let mad_bytes = resp_mad.to_bytes();
        resp_umad.data[..mad_bytes.len()].copy_from_slice(&mad_bytes);

        self.file.write_all(&resp_umad.to_bytes())
    }

    fn send_perf_response(
        &mut self,
        tid: u64,
        umad: &ib_user_mad,
        mad: &ib_mad,
        perf_data: &mad::perf_mad,
    ) -> Result<(), io::Error> {
        if let Some(max_delay) = self.response_delay {
            if max_delay > 0 {
                let delay = rand::random_range(0..=max_delay);
                log::trace!("[tid: {}] Delaying response by {}ms", tid, delay);
                std::thread::sleep(time::Duration::from_micros(delay));
            }
        }

        let mut resp_umad = umad.clone();
        let mut resp_mad = *mad;
        
        // Mark as response (method | 0x80)
        resp_mad.method = mad.method | 0x80;
        resp_mad.status = 0; // Success

        let perf_bytes = perf_data.to_bytes();
        resp_mad.data[..perf_bytes.len()].copy_from_slice(&perf_bytes);
        
        let mad_bytes = resp_mad.to_bytes();
        resp_umad.data[..mad_bytes.len()].copy_from_slice(&mad_bytes);

        self.file.write_all(&resp_umad.to_bytes())
    }

    pub fn process_one_umad(&mut self) -> Result<(), io::Error> {
        let mut buf: [u8; 320] = [0; 320];
        let r = self.file.read(&mut buf)?;
        log::trace!("Read {} bytes from UMAD file.", r);

        if r < MIN_UMAD_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!(
                    "UMAD too small: expected at least {} bytes, got {}",
                    MIN_UMAD_SIZE, r
                ),
            ));
        }

        let umad = ib_user_mad::from_bytes(&buf).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "Failed to parse ib_user_mad")
        })?;

        let mad = ib_mad::from_bytes(&umad.data)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Failed to parse ib_mad"))?;

        // Use the transaction ID for correlated logging
        let tid = mad.tid;
        let attr_id = mad.attr_id;
        log::debug!(
            "[tid: {}] Received MAD. Class: 0x{:02X}, AttrID: 0x{:04X}",
            tid,
            mad.mgmt_class,
            attr_id
        );

        match mad.mgmt_class {
            0x81 => {
                // SubnAdm (Directed Route)
                log::trace!("[tid: {}] Processing SubnAdm Directed Route MAD.", tid);

                let dr_smp = mad::dr_smp_mad::from_bytes(&mad.data).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "Unable to parse DR SMP")
                })?;

                log::trace!("[tid: {}] Initial Path: {:?}", tid, dr_smp.initial_path);

                let mut current_node: Option<Rc<RefCell<Node>>> = None;
                let mut current_port: Option<Rc<RefCell<Port>>> = None;

                // Initialize current_node/port from the agent's location (FIRST_HOP)
                // This allows paths starting with a port number (non-zero) to work.
                if let Some(node_weak) = self.dr_paths.get(&FIRST_HOP) {
                    if let Some(first_hop_port) = node_weak.upgrade() {
                        let port_ref = first_hop_port.borrow();
                        if let Some(parent_node) = port_ref.parent.upgrade() {
                            current_node = Some(parent_node);
                            current_port = Some(first_hop_port.clone());
                        }
                    }
                }

                // --- Path Traversal ---
                for (index, portnum) in dr_smp.initial_path.iter().enumerate() {
                    if index == 0 && *portnum == 0 {
                        log::trace!(
                            "[tid: {}] Path[{}]: Port 0, initiating traversal from first hop.",
                            tid,
                            index
                        );

                        let node_weak = self.dr_paths.get(&FIRST_HOP).ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::NotFound,
                                format!("[tid: {}] Unable to find first hop in dr_paths.", tid),
                            )
                        })?;

                        let first_hop_port = node_weak.upgrade().ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::NotFound,
                                format!("[tid: {}] First hop reference is stale.", tid),
                            )
                        })?;

                        let port_ref = first_hop_port.borrow();

                        let parent_node = port_ref.parent.upgrade().ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::NotFound,
                                format!(
                                    "[tid: {}] First hop port {} has no parent.",
                                    tid, port_ref.num
                                ),
                            )
                        })?;

                        log::trace!(
                            "[tid: {}] Path[{}]: Starting at node '{}'",
                            tid,
                            index,
                            parent_node.borrow().description
                        );

                        current_node = Some(parent_node);
                        current_port = Some(first_hop_port.clone());
                        continue;
                    }

                    // A port number of 0 signifies the end of the path.
                    if *portnum == 0 {
                        log::trace!(
                            "[tid: {}] Path[{}]: Encountered port 0, path traversal complete.",
                            tid,
                            index
                        );
                        break;
                    }

                    let node_rc = current_node.clone().ok_or_else(|| {
                        io::Error::new(io::ErrorKind::NotFound, format!("[tid: {}] Path traversal failed: current_node is None at index {}.", tid, index))
                    })?;

                    let node_ref = node_rc.borrow();
                    log::trace!(
                        "[tid: {}] Path[{}]: Traversing from node '{}' via port {}.",
                        tid,
                        index,
                        node_ref.description,
                        *portnum
                    );

                    let egress_port_rc = &node_ref
                        .ports
                        .iter()
                        .find(|p| p.borrow().num == *portnum)
                        .ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::NotFound,
                                format!(
                                    "[tid: {}] Could not find egress port {} on node '{}'",
                                    tid, *portnum, node_ref.description
                                ),
                            )
                        })?;

                    let egress_port_ref = egress_port_rc.borrow();

                    let remote_port_weak = egress_port_ref.remote_port.as_ref().ok_or_else(|| {
                        io::Error::new(io::ErrorKind::NotFound, format!("[tid: {}] Port {} on node '{}' is not connected (no remote_port).", tid, *portnum, node_ref.description))
                    })?;

                    let remote_port_rc = remote_port_weak.upgrade().ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::NotFound,
                            format!(
                                "[tid: {}] Remote port reference from '{}' port {} is stale.",
                                tid, node_ref.description, *portnum
                            ),
                        )
                    })?;

                    let next_node_rc = {
                        // Scoped borrow
                        let remote_port_ref = remote_port_rc.borrow();
                        remote_port_ref.parent.upgrade().ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::NotFound,
                                format!(
                                    "[tid: {}] Remote port {} has no parent node.",
                                    tid, remote_port_ref.num
                                ),
                            )
                        })?
                    };

                    log::trace!(
                        "[tid: {}] Path[{}]: Arrived at node '{}'",
                        tid,
                        index,
                        next_node_rc.borrow().description
                    );
                    current_node = Some(next_node_rc);
                    current_port = Some(remote_port_rc);
                }

                let attr_id = mad.attr_id;

                if let Some(cn) = &current_node {
                    log::debug!(
                        "[tid: {}] Path traversal finished. Final node: '{}'. Processing AttrID: 0x{:04X}",
                        tid,
                        cn.borrow().description,
                        attr_id
                    );
                } else {
                    log::debug!(
                        "[tid: {}] Path traversal finished. Final node: None. Processing AttrID: 0x{:04X}",
                        tid,
                        attr_id
                    );
                }

                match attr_id {
                    0x1000 => {
                        // NodeDesc
                        let node_rc = current_node.ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::NotFound,
                                format!("[tid: {}] Target node not found for NodeDesc query", tid),
                            )
                        })?;
                        let node_ref = node_rc.borrow();

                        log::debug!(
                            "[tid: {}] Responding with NodeDesc for '{}': '{}'",
                            tid,
                            node_ref.description,
                            node_ref.description
                        );

                        let resp_nd_str = &node_ref.description;
                        let nd_bytes = resp_nd_str.as_bytes();

                        self.send_dr_response(tid, &umad, &mad, &dr_smp, &nd_bytes)?;
                        log::trace!("[tid: {}] Wrote NodeDesc response.", tid);
                    }

                    0x1100 => {
                        // NodeInfo
                        let node_rc = current_node.ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::NotFound,
                                format!("[tid: {}] Target node not found for NodeInfo query", tid),
                            )
                        })?;
                        let port_rc = current_port.ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::NotFound,
                                format!("[tid: {}] Target port not found for NodeInfo query", tid),
                            )
                        })?;

                        let node_ref = node_rc.borrow();
                        let port_ref = port_rc.borrow();

                        log::debug!(
                            "[tid: {}] Responding with NodeInfo for '{}' from perspective of port {}",
                            tid,
                            node_ref.description,
                            port_ref.num
                        );

                        let mut resp_ni = node_ref.node_info.clone();
                        resp_ni.local_port = port_ref.num;

                        let ni_bytes = resp_ni.to_bytes();

                        self.send_dr_response(tid, &umad, &mad, &dr_smp, &ni_bytes)?;
                        log::trace!("[tid: {}] Wrote NodeInfo response.", tid);
                    }

                    0x1500 => {
                        // PortInfo

                        let mut portnum = mad.attr_mod.to_be() as u8;

                        log::debug!("[tid: {}] Received PortInfo for port {}", tid, portnum,);

                        if portnum == 0 {
                            portnum = 1;
                        }

                        let node_rc = current_node.ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::NotFound,
                                format!("[tid: {}] Target node not found for NodeInfo query", tid),
                            )
                        })?;
                        let node_ref = node_rc.borrow();

                        let target_port_rc = node_ref
                            .ports
                            .iter()
                            .find(|p| p.borrow().num == portnum)
                            .ok_or_else(|| {
                                io::Error::new(
                                    io::ErrorKind::NotFound,
                                    format!(
                                        "[tid: {}] Could not find egress port {} on node '{}'",
                                        tid, portnum, node_ref.description
                                    ),
                                )
                            })?;

                        let target_port_ref = target_port_rc.borrow();

                        log::debug!(
                            "[tid: {}] Responding with PortInfo for port {} on node '{}' (LID: {}) logical_state: {}, phy_state: {}",
                            tid,
                            portnum,
                            node_ref.description,
                            target_port_ref.port_info.lid(),
                            target_port_ref.port_info.port_state(),
                            target_port_ref.port_info.port_physical_state(),
                        );

                        let resp_pi = target_port_ref.port_info;
                        let pi_bytes = resp_pi.to_bytes();

                        self.send_dr_response(tid, &umad, &mad, &dr_smp, &pi_bytes)?;
                        log::trace!("[tid: {}] Wrote PortInfo response.", tid);
                    }
                    _ => {
                        log::warn!("[tid: {}] Unhandled SubnAdm AttrID: 0x{:04X}", tid, attr_id);
                    }
                }
            }
            0x4 => {
                // Performance Management
                log::trace!("[tid: {}] Processing Performance Management MAD.", tid);
                let perf_req = mad::perf_mad::from_bytes(&mad.data).ok_or_else(|| {
                     io::Error::new(io::ErrorKind::InvalidData, "Unable to parse Perf MAD")
                })?;

                let dest_lid = u16::from_be(umad.addr.lid);
                let port_select = perf_req.port_select();

                // Find node by LID
                // Since we don't implement full forwarding, we cheat and search all nodes for the LID.
                let mut target_node: Option<Rc<RefCell<Node>>> = None;
                for node_rc in &self.nodes {
                    if node_rc.borrow().lid == dest_lid {
                         target_node = Some(node_rc.clone());
                         break;
                    }
                    // Check ports too if needed, but simplified model assumes node LID match or port LID match
                    // Port 0 usually has the Node LID.
                }

                if let Some(node) = target_node {
                    match attr_id {
                        0x001D => {
                            // PortCountersExtended
                            log::debug!("[tid: {}] Received PortCountersExtended for Node '{}' Port {}", tid, node.borrow().description, port_select);
                            
                            // Check if port exists
                            let node_ref = node.borrow();
                            let port_exists = node_ref.ports.iter().any(|p| p.borrow().num == port_select);
                            
                            if !port_exists {
                                log::warn!("[tid: {}] Port {} not found on node '{}'", tid, port_select, node_ref.description);
                                // Should return error MAD...
                            } else {
                                // Construct dummy response
                                let mut resp = perf_req; // Copy request to preserve other fields
                                
                                // Set some dummy counters based on port number to verify unique data
                                resp.set_port_xmit_data((1000 * port_select as u64) + 1);
                                resp.set_port_rcv_data((2000 * port_select as u64) + 2);
                                resp.set_port_xmit_pkts((10 * port_select as u64) + 3);
                                resp.set_port_rcv_pkts((20 * port_select as u64) + 4);
                                
                                self.send_perf_response(tid, &umad, &mad, &resp)?;
                            }
                        }
                        _ => {
                             log::warn!("[tid: {}] Unhandled Perf AttrID: 0x{:04X}", tid, attr_id);
                        }
                    }
                } else {
                    log::warn!("[tid: {}] Target LID {} not found in fabric.", tid, dest_lid);
                }

            }
            0x1 => {
                log::debug!(
                    "[tid: {}] Received LID-Routed MAD. (Currently unhandled)",
                    tid
                );
            }

            _ => {
                log::warn!(
                    "[tid: {}] Received unhandled MAD management class: 0x{:02X}",
                    tid,
                    mad.mgmt_class
                );
            }
        }

        Ok(())
    }

    pub fn run(&mut self, done: sync::mpsc::Receiver<bool>) -> Result<(), io::Error> {
        log::info!("Starting UMAD processing loop...");
        loop {
            // Non-blocking check for the done signal
            match done.try_recv() {
                Ok(true) => {
                    log::info!("Stop signal received. Shutting down UMAD processing loop.");
                    break;
                }
                Ok(false) => { /* Continue */ }
                Err(sync::mpsc::TryRecvError::Empty) => { /* No signal, continue */ }
                Err(sync::mpsc::TryRecvError::Disconnected) => {
                    log::warn!("MPSC channel disconnected. Shutting down.");
                    break;
                }
            }

            if let Err(e) = self.process_one_umad() {
                if e.kind() != io::ErrorKind::UnexpectedEof {
                    log::error!("Error processing UMAD packet: {}. Kind: {:?}", e, e.kind());
                }
            }
        }
        log::info!("UMAD processing loop has finished.");
        Ok(())
    }
}

impl Port {
    pub fn new_port(num: u8, lid: u16, parent: Rc<RefCell<Node>>) -> Port {
        let mut port_info = port_info { data: [0; 64] };

        port_info.set_local_portnum(num);
        port_info.set_lid(lid);

        port_info.set_port_state(1); // Down 
        port_info.set_port_physical_state(2); // Polling

        port_info.set_link_speed_supported(1);
        port_info.set_link_speed_enabled(1);
        port_info.set_link_speed_active(1);
        port_info.set_link_width_supported(1);
        port_info.set_link_width_enabled(1);
        port_info.set_link_width_active(1);

        let port = Port {
            num: num,
            port_info: port_info,
            remote_port: None,
            parent: Rc::downgrade(&parent),
        };

        port
    }
}

impl Node {
    pub fn new_hca(description: &str, guid: u64) -> Node {
        let hca = Node {
            description: description.to_owned(),
            node_info: node_info {
                base_version: 0x1,
                class_version: 0x1,
                node_type: 0x1, // Channel Adapter
                nports: 1,
                system_guid: guid,
                node_guid: guid,
                port_guid: guid,
                partition_cap: 128,
                device_id: 128,
                revision: 0,
                local_port: 1,
                vendor_id: [0x00, 0x02, 0xc9],
                reserved: [0; 24],
            },
            ports: Vec::new(),
            lid: 0, // Will be set by port later ideally, but simpler here
        };

        hca
    }

    pub fn new_switch(description: &str, guid: u64) -> Node {
        let switch = Node {
            description: description.to_owned(),
            node_info: node_info {
                base_version: 0x1,
                class_version: 0x1,
                node_type: 0x2, // Switch
                nports: 65,
                system_guid: guid,
                node_guid: guid,
                port_guid: guid,
                partition_cap: 8,
                device_id: 0xd2f2,
                revision: 0x0000_00a0,
                local_port: 0, // Port 0 is the management port
                vendor_id: [0x00, 0xcf, 0x09],
                reserved: [0; 24],
            },
            ports: Vec::new(),
            lid: 0,
        };

        switch
    }
}
