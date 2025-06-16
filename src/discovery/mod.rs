use std::{cell::RefCell, collections::{HashMap, HashSet, VecDeque}, io, rc::{Rc, Weak}};

use crate::{enums, mad::{self, ib_mad_addr, ib_user_mad, node_info, IbMadPort}};

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
    pub description: String,
    pub ports: Vec<Rc<RefCell<Port>>>,
}

#[derive(Debug)]
pub struct Fabric {
    pub port: IbMadPort,
    pub nodes: Vec<Rc<RefCell<Node>>>,
    pub switches: Vec<Weak<RefCell<Node>>>,
    pub hcas: Vec<Weak<RefCell<Node>>>,
    pub dr_paths: HashMap<[u8; 64], Weak<RefCell<Port>>>
}


impl Fabric {

    fn build_umad(&self) -> mad::ib_user_mad {
        let umad = ib_user_mad{
            agent_id: 0x0,
            status: 0x0,
            timeout_ms: 50,
            retries: 3,
            length: 256,
            addr: ib_mad_addr {
                qpn: 0,
                qkey: mad::IB_DEFAULT_QKEY,
                lid: 0xfff,
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

    fn build_mad(&self, attr_id: enums::SmiAttrID, attr_mod: u32) -> mad::ib_mad {
        let mad = mad::ib_mad{
            base_version: 0x1,
            mgmt_class: 0x81,
            method: 0x1,
            class_version: 0x1,
            status: 0x0,
            hop_ptr: 0x0,
            hop_cnt: 0,
            tid: 0x1,
            attr_id: attr_id as u16,
            additional_status: 0x0,
            attr_mod: attr_mod,
            data: [0; 232],
        };

        return mad
    }

    fn build_dr_smp(&self, path: [u8; 64]) -> mad::dr_smp_mad {
        let dr_smp = mad::dr_smp_mad{
            m_key: 0x0,
            drslid: 0xfff,
            drdlid: 0xfff,
            reserved: [0; 28],
            attr_layout: [0; 64],
            initial_path: path,
            return_path: [0; 64],
        };

        return dr_smp
    }

    fn build_dr_smp_umad(&self, path: [u8; 64], attr_id: enums::SmiAttrID, attr_mod: u32) -> ib_user_mad {
        let mut dr_smp = self.build_dr_smp(path);
        let mut mad = self.build_mad(attr_id, attr_mod);
        let mut umad = self.build_umad();

        dr_smp.initial_path = path;

        // Assemble the UMAD

        let dr_bytes = dr_smp.to_bytes();
        mad.data[..dr_bytes.len()].copy_from_slice(&dr_bytes);

        let mad_bytes = mad.to_bytes();
        umad.data[..mad_bytes.len()].copy_from_slice(&mad_bytes);

        umad
    }

    pub fn query_nodeinfo(&mut self, path: [u8; 64] ) -> Result<ib_user_mad, io::Error> {
        let umad =  self.build_dr_smp_umad(  path, enums::SmiAttrID::NodeInfo, 0x0);
        let _s = mad::send(&mut self.port, &umad)?;

        Ok(umad)
    }

    pub fn query_portinfo(&mut self, path: [u8; 64], portnum: u8 ) -> Result<ib_user_mad, io::Error> {
        let umad =  self.build_dr_smp_umad( path, enums::SmiAttrID::NodeInfo, portnum as u32);
        let _s = mad::send(&mut self.port, &umad)?;

        Ok(umad)
    }


    pub fn recv_smp(&mut self) -> Result<ib_user_mad, io::Error> {

        let mut umad = self.build_umad();

        let _s = mad::recv(&mut self.port, &mut umad)?;

        Ok(umad)
    }


    pub fn discover(&mut self) -> Result<(), io::Error> {
        let mut visited: HashSet<u64> = HashSet::new();
        let mut queue: VecDeque<Rc<RefCell<Node>>> = VecDeque::new();

        let sent_umad = self.query_nodeinfo(START_PATH)?;
        log::debug!("First-hop send NodeInfo: {:?}", sent_umad);
        let recv_umad = self.recv_smp()?;

        let ni = node_info::from_bytes(&recv_umad.data[64..]).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "Could not parse NodeInfo data.")
        })?;
        log::debug!("First-hop recv NodeInfo: {:?}", ni);

        let local_port = ni.local_port;

        let sent_umad = self.query_portinfo(START_PATH,  local_port as u8)?;
        log::debug!("First-hop send PortInfo: {:?}", sent_umad);

        let recv_umad = self.recv_smp()?;
        log::debug!("First-hop recv PortInfo: {:?}", recv_umad);

        Ok(())
    }
}