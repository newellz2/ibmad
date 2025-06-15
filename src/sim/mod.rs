use std::{cell::RefCell, collections::HashMap, fs, io::{self, Read}, rc::{Rc, Weak}};

use crate::mad::{self, ib_mad, ib_user_mad, node_info, port_info};

const MIN_UMAD_SIZE: usize = 320;

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
    pub dr_paths: HashMap<[u8; 64], Weak<RefCell<Port>>>
}

impl Fabric {
    pub fn new(file: fs::File) -> Self {
        Fabric { 
            file: file,
            nodes: Vec::new(),
            switches: Vec::new(),
            hcas: Vec::new(),
            dr_paths: HashMap::new(),
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

    pub fn process_one_umad(&mut self) -> Result<(), io::Error> {
        let mut buf: [u8; 320] = [0; 320];
        let r = self.file.read(&mut buf)?;
        log::trace!("process_one_umad - Read {} bytes", r);

        if r < MIN_UMAD_SIZE {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "UMAD too small"));
        }

        let umad = ib_user_mad::from_bytes(&buf).ok_or_else( || {
            io::Error::new(io::ErrorKind::InvalidData, "Unable to parse UMAD")
        })?;

        log::trace!("process_one_umad - umad: {:?}", umad);

        let mad = ib_mad::from_bytes(&umad.data).ok_or_else( || {
            io::Error::new(io::ErrorKind::InvalidData, "Unable to parse MAD address")
        })?;

        log::trace!("process_one_umad - mad: {:?}", mad);

        match mad.mgmt_class {
            0x81 =>{
                // DR MAD
                let dr_smp = mad::dr_smp_mad::from_bytes(&mad.data).ok_or_else( || {
                    io::Error::new(io::ErrorKind::InvalidData, "Unable to parse DR SMP")
                })?;

                let port_weak = self.dr_paths.get(&dr_smp.initial_path).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "Unable to find path.")

                })?;

                let port_rc = port_weak.upgrade().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::NotFound, "Unable to find node associated with path.")
                })?;


                let port_ref = port_rc.borrow();
                log::trace!(" port: {:?}", port_ref);
            }
            0x1 =>{
                // LID Routed
            }

            _ => {}
        }

        Ok(())
    }

}

impl Port {
    pub fn new_port(num: u8, lid: u16, parent: Rc<RefCell<Node>>) -> Port {

        let mut port_info = port_info { data: [0; 64] };

        port_info.set_local_portnum(num);
        port_info.set_lid(lid);

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
                node_type:  0x1,
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
                        node_type:  0x2,
                        nports: 65,
                        system_guid: guid,
                        node_guid: guid,
                        port_guid: guid,
                        partition_cap: 8,
                        device_id: 0xd2f2,
                        revision: 0x0000_00a0,
                        local_port: 0,
                        vendor_id: [0x00, 0xcf, 0x09],
                        reserved: [0; 24],
                    },
                    ports: Vec::new(),
                };
        

        switch
    }
}
