use std::{cell::RefCell, collections::{HashMap, HashSet, VecDeque}, io, rc::{Rc, Weak}};

use crate::{
    enums,
    mad::{self, ib_mad_addr, ib_user_mad, node_info, IbMadPort},
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
    pub ports: Vec<Rc<RefCell<Port>>>,
}

#[derive(Debug)]
pub struct Fabric {
    pub port: IbMadPort,
    pub nodes: Vec<Rc<RefCell<Node>>>,
    pub switches: Vec<Weak<RefCell<Node>>>,
    pub hcas: Vec<Weak<RefCell<Node>>>,
    pub dr_paths: HashMap<[u8; 64], Weak<RefCell<Port>>>,
    pub timeout: u32,
    pub mad_timeouts: u64,
    pub tid: u64,
}


impl Fabric {

    fn build_umad(&self) -> mad::ib_user_mad {
        let umad = ib_user_mad{
            agent_id: 0x0,
            status: 0x0,
            timeout_ms: self.timeout,
            retries: 3,
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

    pub fn query_nodeinfo(&mut self, path: [u8; 64], hop_cnt: u8 ) -> Result<ib_user_mad, io::Error> {
        let umad =  self.build_dr_smp_umad(  path, enums::SmiAttrID::NodeInfo, 0x0, hop_cnt);
        let _s = mad::send(&mut self.port, &umad)?;

        Ok(umad)
    }

    pub fn query_portinfo(&mut self, path: [u8; 64], portnum: u8, hop_cnt: u8 ) -> Result<ib_user_mad, io::Error> {
        let umad =  self.build_dr_smp_umad(path, enums::SmiAttrID::PortInfo, portnum as u32, hop_cnt);
        let _s = mad::send(&mut self.port, &umad)?;

        Ok(umad)
    }

    pub fn recv_smp(&mut self) -> Result<ib_user_mad, io::Error> {
        let mut umad = self.build_umad();
        let _s = mad::recv(&mut self.port, &mut umad, self.timeout)?;

        Ok(umad)
    }


    pub fn discover(&mut self) -> Result<(), io::Error> {
        let mut visited: HashSet<u64> = HashSet::new();
        let mut queue: VecDeque<[u8; 64]> = VecDeque::new();
        let mut hop_cnt: u8 = 0;

        queue.push_back(START_PATH);

        while let Some(path) = queue.pop_front() {

            if path == START_PATH {
                hop_cnt = 0;
            } else {
                hop_cnt = 0;
                for b in &path[1..] {
                    if *b != 0x00 as u8 {
                        hop_cnt += 1;
                    }
                }
            }
            // Query NodeInfo for the node at the end of this path
            log::debug!("send nodeinfo: hop_cnt: {}, path:{:?}", hop_cnt, path);
            
            let _sent_umad = self.query_nodeinfo(path, hop_cnt)?;
            let r = self.recv_smp();

            if let Ok(recv_umad) = r {
                let ni = node_info::from_bytes(&recv_umad.data[64..]).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "Could not parse NodeInfo data.")
                })?;

                log::debug!("recv nodeinfo: {:?}", ni);
                let guid = ni.node_guid;

                if visited.contains(&guid) {
                    log::debug!("already visited guid: 0x{:x}", guid.to_be());
                    continue;
                }
                visited.insert(ni.node_guid);

                let node_type = match ni.node_type {
                    1 => enums::IbNodeType::CA,
                    2 => enums::IbNodeType::Switch,
                    3 => enums::IbNodeType::Router,
                    4 => enums::IbNodeType::Rnic,
                    _ => enums::IbNodeType::CA,
                };


                let node_rc = Rc::new(RefCell::new(Node {
                    lid: 0,
                    node_guid: ni.node_guid,
                    node_type: node_type.clone(),
                    description: None,
                    ports: Vec::new(),
                }));

                self.nodes.push(node_rc.clone());

                let mut nports = ni.nports;

                match node_type {
                    enums::IbNodeType::Switch => { 
                        nports -= 1;
                        self.switches.push(Rc::downgrade(&node_rc))

                    },
                    _ => self.hcas.push(Rc::downgrade(&node_rc)),
                }

                // Explore all ports on this node
                for port in 1..=nports {
                    let mut next_path = path;
                    for i in 1..=64 {
                        if next_path[i] == 0 {
                            next_path[i] = port;
                            break;
                        }
                    }

                    log::debug!("send portinfo mad to port: {}, hop_cnt: {}, path: {:?}", port, hop_cnt, path);
                    let _sent_umad = self.query_portinfo(path, port as u8, hop_cnt)?;
                    if let Ok(_recv_umad) = self.recv_smp(){
                        log::debug!("recv portinfo from path: {:?}, umad: {:?}", path, _recv_umad);
                        queue.push_back(next_path);
                    } else {
                        log::debug!("timeout reading portinfo");
                        self.mad_timeouts += 1;
                    }
                }
   
            } else {
                log::debug!("timeout reading nodeinfo from port: {:?}", r);
                self.mad_timeouts += 1;
                continue;
            }

        }

        log::debug!("Discovered {} switches", &self.switches.len());
        log::debug!("Discovered {} nodes", &self.nodes.len());
        for n in &self.nodes{
            let node_ref = n.borrow();
            log::debug!("Node 0x{:x}", node_ref.node_guid.to_be())
        }
        log::debug!("{} MAD timeouts", &self.mad_timeouts);

        Ok(())
    }
}