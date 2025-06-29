use std::{cell::RefCell, collections::HashMap, fs, io::{self, Read, Write}, rc::{Rc, Weak}, sync, time};

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
pub struct Node{
    pub description: String,
    pub node_info: mad::node_info,
    pub ports: Vec<Rc<RefCell<Port>>>,
}

#[derive(Debug)]
pub struct Fabric {
    pub file: fs::File,
    pub nodes: Vec<Rc<RefCell<Node>>>,
    pub switches: Vec<Weak<RefCell<Node>>>,
    pub hcas: Vec<Weak<RefCell<Node>>>,
    pub dr_paths: HashMap<[u8; 64], Weak<RefCell<Port>>>,
    pub response_delay: Option<u64>
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
        port_a.parent.upgrade().map_or("?".to_string(), |p| p.borrow().description.clone()),
        port_b.num,
        port_b.parent.upgrade().map_or("?".to_string(), |p| p.borrow().description.clone())
    );
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

    pub fn add_switch(&mut self, switch: Node)  -> Rc<RefCell<Node>> {
        let hca_switch_rc = Rc::new(RefCell::new(switch));
        self.switches.push(
            Rc::downgrade(&hca_switch_rc)
        );

        self.nodes.push(hca_switch_rc.clone());

        return hca_switch_rc.clone();
    }

    pub fn add_hca(&mut self, hca: Node) -> Rc<RefCell<Node>> {
        let hca_rc =  Rc::new(RefCell::new(hca));
        self.hcas.push(
            Rc::downgrade(&hca_rc)
        );

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

    pub fn process_one_umad(&mut self) -> Result<(), io::Error> {
        let mut buf: [u8; 320] = [0; 320];
        let r = self.file.read(&mut buf)?;
        log::trace!("Read {} bytes from UMAD file.", r);

        if r < MIN_UMAD_SIZE {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, format!("UMAD too small: expected at least {} bytes, got {}", MIN_UMAD_SIZE, r)));
        }

        let umad = ib_user_mad::from_bytes(&buf).ok_or_else( || {
            io::Error::new(io::ErrorKind::InvalidData, "Failed to parse ib_user_mad")
        })?;

        let mad = ib_mad::from_bytes(&umad.data).ok_or_else( || {
            io::Error::new(io::ErrorKind::InvalidData, "Failed to parse ib_mad")
        })?;

        // Use the transaction ID for correlated logging
        let tid = mad.tid;
        let attr_id = mad.attr_id;
        log::debug!("[tid: {}] Received MAD. Class: 0x{:02X}, AttrID: 0x{:04X}", tid, mad.mgmt_class, attr_id);


        match mad.mgmt_class {
            0x81 =>{ // SubnAdm (Directed Route)
                log::trace!("[tid: {}] Processing SubnAdm Directed Route MAD.", tid);

                let dr_smp = mad::dr_smp_mad::from_bytes(&mad.data).ok_or_else( || {
                    io::Error::new(io::ErrorKind::InvalidData, "Unable to parse DR SMP")
                })?;

                log::trace!("[tid: {}] Initial Path: {:?}", tid, dr_smp.initial_path);

                let mut current_node: Option<Rc<RefCell<Node>>> = None;
                let mut current_port: Option<Rc<RefCell<Port>>> = None;

                // --- Path Traversal ---
                for (index, portnum) in dr_smp.initial_path.iter().enumerate(){
                    if index == 0 && *portnum == 0 {
                        log::trace!("[tid: {}] Path[{}]: Port 0, initiating traversal from first hop.", tid, index);

                        let node_weak = self.dr_paths.get(&FIRST_HOP).ok_or_else(|| {
                            io::Error::new(io::ErrorKind::NotFound, format!("[tid: {}] Unable to find first hop in dr_paths.", tid))
                        })?;

                        let first_hop_port = node_weak.upgrade().ok_or_else(|| {
                            io::Error::new(io::ErrorKind::NotFound, format!("[tid: {}] First hop reference is stale.", tid))
                        })?;

                        let port_ref = first_hop_port.borrow();

                        let parent_node = port_ref.parent.upgrade().ok_or_else(|| {
                            io::Error::new(io::ErrorKind::NotFound, format!("[tid: {}] First hop port {} has no parent.", tid, port_ref.num))
                        })?;
                        
                        log::trace!("[tid: {}] Path[{}]: Starting at node '{}'", tid, index, parent_node.borrow().description);

                        current_node = Some(parent_node);
                        current_port = Some(first_hop_port.clone());
                        continue;
                    }

                    // A port number of 0 signifies the end of the path.
                    if *portnum == 0 {
                        log::trace!("[tid: {}] Path[{}]: Encountered port 0, path traversal complete.", tid, index);
                        break;
                    }
                    
                    let node_rc = current_node.clone().ok_or_else(|| {
                        io::Error::new(io::ErrorKind::NotFound, format!("[tid: {}] Path traversal failed: current_node is None at index {}.", tid, index))
                    })?;

                    let node_ref = node_rc.borrow();
                    log::trace!("[tid: {}] Path[{}]: Traversing from node '{}' via port {}.", tid, index, node_ref.description, *portnum);


                    let egress_port_rc = &node_ref.ports.iter().find( |p| p.borrow().num == *portnum).ok_or_else(||{
                        io::Error::new(io::ErrorKind::NotFound, format!("[tid: {}] Could not find egress port {} on node '{}'", tid, *portnum, node_ref.description))
                    })?;

                    let egress_port_ref = egress_port_rc.borrow();

                    let remote_port_weak = egress_port_ref.remote_port.as_ref().ok_or_else(|| {
                        io::Error::new(io::ErrorKind::NotFound, format!("[tid: {}] Port {} on node '{}' is not connected (no remote_port).", tid, *portnum, node_ref.description))
                    })?;

                    let remote_port_rc = remote_port_weak.upgrade().ok_or_else(|| {
                        io::Error::new(io::ErrorKind::NotFound, format!("[tid: {}] Remote port reference from '{}' port {} is stale.", tid, node_ref.description, *portnum))
                    })?;
                    
                    let next_node_rc = { // Scoped borrow
                        let remote_port_ref = remote_port_rc.borrow();
                        remote_port_ref.parent.upgrade().ok_or_else(|| {
                            io::Error::new( io::ErrorKind::NotFound, format!("[tid: {}] Remote port {} has no parent node.", tid, remote_port_ref.num))
                        })?
                    };
                    
                    log::trace!("[tid: {}] Path[{}]: Arrived at node '{}'", tid, index, next_node_rc.borrow().description);
                    current_node = Some(next_node_rc);
                    current_port = Some(remote_port_rc);
                }

                let attr_id = mad.attr_id;
                
                if let Some(cn) = &current_node {
                    log::debug!("[tid: {}] Path traversal finished. Final node: '{}'. Processing AttrID: 0x{:04X}", tid, cn.borrow().description, attr_id);
                } else {
                    log::debug!("[tid: {}] Path traversal finished. Final node: None. Processing AttrID: 0x{:04X}", tid, attr_id);
                }
                

                match attr_id {
                    0x1000 => { // NodeDesc
                        let node_rc = current_node.ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("[tid: {}] Target node not found for NodeDesc query", tid)))?;
                        let node_ref = node_rc.borrow();

                        log::debug!("[tid: {}] Responding with NodeDesc for '{}': '{}'", tid, node_ref.description, node_ref.description);

                        let resp_nd_str = &node_ref.description;
                        let nd_bytes = resp_nd_str.as_bytes();

                        self.send_dr_response(tid, &umad, &mad, &dr_smp, &nd_bytes)?;
                        log::trace!("[tid: {}] Wrote NodeDesc response.", tid);
                    }

                    0x1100 => { // NodeInfo
                        let node_rc = current_node.ok_or_else(|| io::Error::new( io::ErrorKind::NotFound, format!("[tid: {}] Target node not found for NodeInfo query", tid)))?;
                        let port_rc = current_port.ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("[tid: {}] Target port not found for NodeInfo query", tid)))?;
                        
                        let node_ref = node_rc.borrow();
                        let port_ref = port_rc.borrow();

                        log::debug!("[tid: {}] Responding with NodeInfo for '{}' from perspective of port {}", tid, node_ref.description, port_ref.num);

                        let mut resp_ni = node_ref.node_info.clone();
                        resp_ni.local_port = port_ref.num;

                        let ni_bytes = resp_ni.to_bytes();
 
                        self.send_dr_response(tid, &umad, &mad, &dr_smp, &ni_bytes)?;
                        log::trace!("[tid: {}] Wrote NodeInfo response.", tid);
                    }

                    0x1500 => { // PortInfo

                        let mut portnum = mad.attr_mod.to_be() as u8;

                        log::debug!(

                            "[tid: {}] Received PortInfo for port {}",
                            tid,
                            portnum,
                        );

                        if portnum == 0 {
                            portnum = 1;
                        }

                        let node_rc = current_node.ok_or_else(|| io::Error::new( io::ErrorKind::NotFound, format!("[tid: {}] Target node not found for NodeInfo query", tid)))?;                        
                        let node_ref = node_rc.borrow();

                        let target_port_rc = node_ref.ports.iter().find( |p| p.borrow().num == portnum).ok_or_else(||{
                            io::Error::new(io::ErrorKind::NotFound, format!("[tid: {}] Could not find egress port {} on node '{}'", tid, portnum, node_ref.description))
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
            0x1 =>{
                log::debug!("[tid: {}] Received LID-Routed MAD. (Currently unhandled)", tid);
            }

            _ => {
                log::warn!("[tid: {}] Received unhandled MAD management class: 0x{:02X}", tid, mad.mgmt_class);
            }
        }

        Ok(())
    }

    pub fn run(&mut self, done: sync::mpsc::Receiver<bool> ) -> Result<(), io::Error>{
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

        let port = Port{
            num: num,
            port_info: port_info,
            remote_port: None,
            parent: Rc::downgrade(
                &parent
            )

        };

        port
    }
}

impl Node {
    pub fn new_hca(description: &str, guid: u64) -> Node {
        let hca = Node{
            description: description.to_owned(),
            node_info: node_info{
                base_version: 0x1,
                class_version: 0x1,
                node_type:  0x1, // Channel Adapter
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
        };

        hca
    }

    pub fn new_switch(description: &str, guid: u64) -> Node {

        let switch =
                Node{
                    description: description.to_owned(),
                    node_info: node_info{
                        base_version: 0x1,
                        class_version: 0x1,
                        node_type:  0x2, // Switch
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
                };


        switch
    }
}