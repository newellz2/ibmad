use std::{
    collections::HashMap,
    io,
    sync::Weak,
    sync::{Arc, RwLock},
    time,
};

use crate::{
    enums,
    mad::{self, IbMadPort, ib_mad_addr, ib_user_mad, node_info, port_info},
};

pub(crate) const START_PATH: [u8; 64] = [0; 64];

#[derive(Debug, Clone)]
pub struct Port {
    pub number: u8,
    pub link_state: enums::IbPortLinkLayerState,
    pub phys_state: enums::IbPortPhyState,
    pub lid: u16,
    /// Raw `LinkWidthActive` from PortInfo (IBTA bitmask: `0x01`=1x,
    /// `0x02`=4x, `0x04`=8x, `0x08`=12x, `0x10`=2x, `0x20`=16x). `0` for
    /// placeholder ports whose PortInfo query timed out.
    pub link_width_active: u8,
    pub remote_port: Option<Weak<RwLock<Port>>>,
    pub parent: Weak<RwLock<Node>>,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub lid: u16,
    pub dr_path: [u8; 64],
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
    /// UMAD agent id to use for DR SMP requests (returned by `mad::register_agent`).
    /// For sim/tests that don't use a real UMAD device, this can be 0.
    pub agent_id: u32,
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
    /// Maximum number of hops from the local CA to traverse, `None` = no
    /// limit (sweep the whole reachable fabric). Hop 1 is the switch (or
    /// node) directly wired to the local port the sweep started from; hop 2
    /// is whatever is attached to hop 1's other ports, and so on. A node at
    /// exactly `max_hops` is still fully discovered (so its own ports are
    /// still emitted as poll targets), but the traversal does not continue
    /// past it to discover what's beyond. Set via [`Fabric::set_max_hops`].
    pub max_hops: Option<u8>,
}

pub(crate) fn lock_err<T: std::fmt::Debug>(e: T) -> io::Error {
    io::Error::new(io::ErrorKind::Other, format!("Lock poisoned: {:?}", e))
}

impl Fabric {
    /// Build an empty fabric ready for a discovery sweep.
    ///
    /// `agent_id` is the UMAD agent returned by `mad::register_agent` for the DR
    /// SMP class; pass 0 for sim/tests that have no real UMAD device.
    pub fn new(port: IbMadPort, agent_id: u32, timeout: u32, retries: u32) -> Self {
        Fabric {
            port,
            agent_id,
            node_map: HashMap::new(),
            nodes: Vec::new(),
            switches: Vec::new(),
            hcas: Vec::new(),
            dr_paths: HashMap::new(),
            ni_timings: Vec::new(),
            retries,
            timeout,
            mad_errors: 0,
            mad_timeouts: 0,
            mads_sent: 0,
            tid: 1,
            max_hops: None,
        }
    }

    /// Limit the discovery sweep to `max_hops` hops from the local CA (see
    /// [`Fabric::max_hops`] for what "hop" means here). `None` (the default
    /// from [`Fabric::new`]) sweeps the whole reachable fabric.
    pub fn set_max_hops(&mut self, max_hops: Option<u8>) -> &mut Self {
        self.max_hops = max_hops;
        self
    }

    pub(crate) fn format_path(path: &[u8; 64]) -> String {
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

    pub(crate) fn get_hop_count(path: &[u8; 64]) -> u8 {
        path.iter().skip(1).take_while(|&&p| p != 0).count() as u8
    }

    pub(crate) fn build_umad(agent_id: u32, timeout: u32, retries: u32) -> mad::ib_user_mad {
        let umad = ib_user_mad {
            agent_id,
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
            data: [0; 256],
        };

        umad
    }

    pub(crate) fn build_mad(
        mgmt_class: u8,
        method: u8,
        attr_id: enums::SmiAttrID,
        attr_mod: u32,
        hop_cnt: u8,
        tid: u64,
    ) -> mad::ib_mad {
        let tid = tid & 0x0000_0000_ffff_ffff;
        let mad = mad::ib_mad {
            base_version: 0x1,
            mgmt_class: mgmt_class,
            method: method,
            class_version: 0x1,
            status: 0x0,
            hop_ptr: 0,
            hop_cnt: hop_cnt,
            tid: tid.to_be(),
            attr_id: (attr_id as u16).to_be(),
            additional_status: 0x0,
            attr_mod: attr_mod.to_be(),
            data: [0; 232],
        };

        return mad;
    }

    pub(crate) fn build_dr_smp(path: [u8; 64]) -> mad::dr_smp_mad {
        let dr_smp = mad::dr_smp_mad {
            m_key: 0x0,
            drslid: 0xffff,
            drdlid: 0xffff,
            reserved: [0; 28],
            attr_layout: [0; 64],
            initial_path: path,
            return_path: [0; 64],
        };

        return dr_smp;
    }

    pub(crate) fn build_dr_smp_umad(
        path: [u8; 64],
        attr_id: enums::SmiAttrID,
        attr_mod: u32,
        hop_cnt: u8,
        tid: u64,
        agent_id: u32,
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
        let mut umad = Fabric::build_umad(agent_id, timeout, retries);

        dr_smp.initial_path = path;

        // Assemble the UMAD
        let dr_bytes = dr_smp.to_bytes();
        mad.data[..dr_bytes.len()].copy_from_slice(&dr_bytes);

        let mad_bytes = mad.to_bytes();
        umad.data[..mad_bytes.len()].copy_from_slice(&mad_bytes);

        umad
    }

    pub(crate) fn next_tid(&mut self) -> u64 {
        let mut current = self.tid & 0x0000_0000_ffff_ffff;
        if current == 0 {
            current = 1;
        }
        let mut next = current.wrapping_add(1) & 0x0000_0000_ffff_ffff;
        if next == 0 {
            next = 1;
        }
        self.tid = next;
        current
    }

    /// Send a DR SMP and wait for the response with the same TID, retrying
    /// with exponential backoff.
    ///
    /// This loop is the *only* retry layer: each attempt is handed to the
    /// kernel with `retries = 0` and `timeout_ms` = this attempt's timeout.
    /// Previously the kernel also retried each attempt, which multiplied the
    /// cost of every dead port and left stray send-timeout notifications in
    /// the queue. Those notifications are short packets carrying the TID of
    /// the attempt that timed out; one for our TID ends the attempt early,
    /// and ones for other TIDs are discarded instead of aborting our wait.
    pub(crate) fn send_and_match_with_retries(
        &mut self,
        umad_to_send: ib_user_mad,
    ) -> Result<ib_user_mad, io::Error> {
        /// Grace period past the kernel's timeout for its notice to arrive.
        const KERNEL_NOTIFY_SLACK_MS: u32 = 50;

        let retries = self.retries;
        let mut current_timeout = self.timeout.max(1);
        let backoff_factor = 2;
        let expected_tid = umad_to_send.get_tid()?;
        let mut umad_to_send = umad_to_send;
        umad_to_send.retries = 0;

        for attempt in 0..=retries {
            umad_to_send.timeout_ms = current_timeout;
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

            let deadline = time::Instant::now()
                + time::Duration::from_millis(
                    current_timeout.saturating_add(KERNEL_NOTIFY_SLACK_MS) as u64,
                );

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
                let remaining_time = ((deadline - now).as_millis() as u32).max(1);
                let mut recv_umad = Fabric::build_umad(self.agent_id, self.timeout, self.retries);

                match mad::recv_packet(&mut self.port, &mut recv_umad, remaining_time) {
                    Ok(_) => {
                        if !umad_to_send.is_tid_equal(&recv_umad) {
                            log::trace!(
                                "Discarding mismatched TID. Expected 0x{:X}, got 0x{:X}",
                                expected_tid,
                                recv_umad.get_tid().unwrap_or(0)
                            );
                            continue;
                        }
                        let kernel_status = recv_umad.status; // copy out of packed struct
                        if kernel_status != 0 {
                            // Kernel send-timeout notice for this attempt.
                            log::debug!(
                                "Attempt {} for TID 0x{:X} timed out (kernel status {})",
                                attempt + 1,
                                expected_tid,
                                kernel_status
                            );
                            self.mad_timeouts += 1;
                            break;
                        }
                        log::trace!("<- Matched response for TID 0x{:X}", expected_tid);
                        return Ok(recv_umad);
                    }
                    Err(e) if e.kind() == io::ErrorKind::TimedOut => {
                        self.mad_timeouts += 1;
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
            format!("All {} retries failed for TID {}", retries, expected_tid),
        ))
    }

    pub fn recv_smp(&mut self) -> Result<ib_user_mad, io::Error> {
        let mut umad = Fabric::build_umad(self.agent_id, self.timeout, self.retries);
        let _s = mad::recv(&mut self.port, &mut umad, self.timeout)?;

        Ok(umad)
    }

    /// Reject SMP responses whose MAD status reports an error. Bit 15 of a DR
    /// SMP's status is the direction (D) bit, set on every response, so only
    /// the low 15 bits indicate a failure. Without this check an error
    /// response's (mostly zero) attribute data gets parsed as if it were real.
    pub(crate) fn check_smp_status(umad: &ib_user_mad, what: &str) -> Result<(), io::Error> {
        let status = u16::from_be_bytes([umad.data[4], umad.data[5]]) & 0x7FFF;
        if status != 0 {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("{} returned MAD status 0x{:04X}", what, status),
            ));
        }
        Ok(())
    }

    pub fn discover_node(
        &mut self,
        path: [u8; 64],
        hop_cnt: u8,
    ) -> Result<Arc<RwLock<Node>>, io::Error> {
        let node_info = self.fetch_node_info(path, hop_cnt)?;
        self.discover_node_from_info(path, hop_cnt, node_info)
    }

    /// Like [`Fabric::discover_node`], but reuses a NodeInfo the caller already
    /// fetched for `path` instead of querying it again.
    pub(crate) fn discover_node_from_info(
        &mut self,
        path: [u8; 64],
        hop_cnt: u8,
        node_info: node_info,
    ) -> Result<Arc<RwLock<Node>>, io::Error> {
        let node_guid = node_info.node_guid;
        if let Some(existing) = self.node_map.get(&node_guid) {
            log::trace!(
                "Node 0x{:X} already discovered, reusing existing entry.",
                node_info.node_guid.to_be()
            );
            return Ok(existing.clone());
        }

        let node_type = enums::IbNodeType::try_from(node_info.node_type).map_err(|_e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid node_type: {}", node_info.node_type),
            )
        })?;

        let mut node = Node {
            node_guid: node_info.node_guid,
            dr_path: path,
            node_type,
            local_port: node_info.local_port,
            nports: node_info.nports,
            description: None,
            lid: 0,
            ports: Vec::with_capacity(node_info.nports as usize),
        };

        // A missing description shouldn't cost us the node (and everything
        // behind it); NodeInfo already proved it's there.
        node.description = match self.fetch_node_desc(path, hop_cnt) {
            Ok(desc) => Some(desc),
            Err(e) => {
                log::warn!(
                    "Failed to fetch NodeDesc for 0x{:X} at path [{}]: {}",
                    node_info.node_guid.to_be(),
                    Fabric::format_path(&path),
                    e
                );
                None
            }
        };

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

    pub(crate) fn fetch_node_info(
        &mut self,
        path: [u8; 64],
        hop_cnt: u8,
    ) -> Result<node_info, io::Error> {
        let start_ts = time::Instant::now();
        log::debug!(
            "Fetching NodeInfo for path: [{}]",
            Fabric::format_path(&path)
        );

        let tid = self.next_tid();
        let umad_to_send = Fabric::build_dr_smp_umad(
            path,
            enums::SmiAttrID::NodeInfo,
            0x0,
            hop_cnt,
            tid,
            self.agent_id,
            self.timeout,
            self.retries,
        );

        let recv_ni_umad = self.send_and_match_with_retries(umad_to_send)?;
        Fabric::check_smp_status(&recv_ni_umad, "NodeInfo")?;

        self.ni_timings.push(time::Instant::now() - start_ts);

        let ni = node_info::from_bytes(&recv_ni_umad.data[64..]).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "Could not parse NodeInfo data.")
        })?;

        log::trace!("<- Received NodeInfo: {:?}", ni);
        Ok(ni)
    }

    pub(crate) fn fetch_node_desc(
        &mut self,
        path: [u8; 64],
        hop_cnt: u8,
    ) -> Result<String, io::Error> {
        log::debug!(
            "Fetching NodeDesc for path: [{}]",
            Fabric::format_path(&path)
        );

        let tid = self.next_tid();
        let umad_to_send = Fabric::build_dr_smp_umad(
            path,
            enums::SmiAttrID::NodeDesc,
            0x0,
            hop_cnt,
            tid,
            self.agent_id,
            self.timeout,
            self.retries,
        );

        let recv_nd_umad = self.send_and_match_with_retries(umad_to_send)?;
        Fabric::check_smp_status(&recv_nd_umad, "NodeDesc")?;

        let dr: &mad::dr_smp_mad = unsafe {
            &*(recv_nd_umad.data[mad::node::NODE_DESC_OFFSET..].as_ptr() as *const mad::dr_smp_mad)
        };

        let node_desc_bytes = &dr.attr_layout[..mad::node::NODE_DESC_LENGTH];
        let mut node_desc = String::from_utf8_lossy(node_desc_bytes)
            .trim_end_matches('\0')
            .to_string();

        if let Some(null_idx) = node_desc.find('\0') {
            node_desc.truncate(null_idx);
        }

        log::trace!("<- Received NodeDesc: '{}'", node_desc);
        Ok(node_desc)
    }

    pub(crate) fn fetch_port_info(
        &mut self,
        path: [u8; 64],
        port_num: u8,
        hop_cnt: u8,
    ) -> Result<Port, io::Error> {
        log::debug!(
            "Fetching PortInfo for port {} on path: [{}]",
            port_num,
            Fabric::format_path(&path)
        );

        let tid = self.next_tid();
        let umad_to_send = Fabric::build_dr_smp_umad(
            path,
            enums::SmiAttrID::PortInfo,
            port_num as u32,
            hop_cnt,
            tid,
            self.agent_id,
            self.timeout,
            self.retries,
        );

        let recv_pi_umad = self.send_and_match_with_retries(umad_to_send)?;
        Fabric::check_smp_status(&recv_pi_umad, "PortInfo")?;

        let pi = port_info::from_bytes(&recv_pi_umad.data[64..]).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "could not parse portinfo data.")
        })?;

        log::trace!(
            "<- Received PortInfo for port {}: {:?} {} {}",
            port_num,
            pi,
            pi.port_state(),
            pi.port_physical_state()
        );

        let link_state = enums::IbPortLinkLayerState::try_from(pi.port_state()).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid port_state: {:?}", e),
            )
        })?;
        // An unrecognised physical state (reserved/vendor value) is no reason
        // to throw away an otherwise valid port; the link state is what
        // traversal and polling depend on.
        let raw_phy = pi.port_physical_state();
        let phy_state = enums::IbPortPhyState::try_from(raw_phy).unwrap_or_else(|_| {
            log::debug!(
                "Port {} on path [{}] reports unknown physical state {}",
                port_num,
                Fabric::format_path(&path),
                raw_phy
            );
            enums::IbPortPhyState::Unknown
        });
        let lid = pi.lid();

        Ok(Port {
            number: port_num,
            link_state,
            phys_state: phy_state,
            lid,
            link_width_active: pi.link_width_active(),
            parent: Weak::new(),
            remote_port: None,
        })
    }

    pub(crate) fn attach_port_to_node(
        node_arc: &Arc<RwLock<Node>>,
        mut port: Port,
        port_number: u8,
        is_placeholder: bool,
    ) -> Result<(), io::Error> {
        let link_state = port.link_state.clone();
        let phys_state = port.phys_state.clone();
        let port_lid = port.lid;

        let node_desc = {
            let guard = node_arc.read().map_err(|e| {
                io::Error::new(
                    io::ErrorKind::Deadlock,
                    format!("Could not access node rwlock: {:?}", e),
                )
            })?;

            // Propagate switch base LID (smalid) to external ports, mirroring
            // ibnetdisc's recv_port_info: `port->base_lid = node->smalid`
            if matches!(guard.node_type, enums::IbNodeType::Switch)
                && port_number != 0
                && guard.lid != 0
            {
                port.lid = guard.lid;
            }

            guard.description.clone()
        };

        let port_arc = Arc::new(RwLock::new(port));

        {
            let mut port_ref = port_arc.write().map_err(|e| {
                io::Error::new(
                    io::ErrorKind::Deadlock,
                    format!("Could not access port rwlock: {:?}", e),
                )
            })?;
            port_ref.parent = Arc::downgrade(node_arc);
        }

        {
            let mut node_ref = node_arc.write().map_err(|e| {
                io::Error::new(
                    io::ErrorKind::Deadlock,
                    format!("Could not access node rwlock: {:?}", e),
                )
            })?;
            if node_ref.lid == 0
                && port_lid != 0
                && (port_number == node_ref.local_port || port_number == 0)
            {
                node_ref.lid = port_lid;
            }
            node_ref.ports.push(port_arc.clone());
        }

        if is_placeholder {
            log::trace!(
                "Inserted placeholder for Port {} ({:?}/{:?}) on node '{}'",
                port_number,
                link_state,
                phys_state,
                node_desc.as_deref().unwrap_or("N/A")
            );
        } else {
            log::trace!(
                "Successfully added Port {} ({:?}/{:?}) to node '{}'",
                port_number,
                link_state,
                phys_state,
                node_desc.as_deref().unwrap_or("N/A")
            );
        }

        Ok(())
    }

    pub(crate) fn populate_node_ports(
        &mut self,
        node_arc: &Arc<RwLock<Node>>,
        num_ports: u8,
        path: [u8; 64],
        hop_cnt: u8,
    ) -> Result<(), io::Error> {
        let is_switch = {
            let node = node_arc.read().map_err(|e| {
                io::Error::new(
                    io::ErrorKind::Deadlock,
                    format!("Could not access node rwlock: {:?}", e),
                )
            })?;
            matches!(node.node_type, enums::IbNodeType::Switch)
        };

        // For switches, query port 0 first to establish the base LID (smalid)
        // before processing external ports. This mirrors ibnetdisc's
        // recv_port0_info callback which processes port 0 then triggers
        // queries for ports 1..N, ensuring smalid is available for
        // propagation to external ports via attach_port_to_node.
        if is_switch {
            match self.fetch_port_info(path, 0, hop_cnt) {
                Ok(port) => {
                    Fabric::attach_port_to_node(node_arc, port, 0, false)?;
                }
                Err(e) if e.kind() == io::ErrorKind::TimedOut => {
                    log::warn!(
                        "Timeout getting PortInfo for switch port 0 on path [{}]; switch LID will be unavailable",
                        Fabric::format_path(&path),
                    );
                }
                Err(e) => {
                    log::warn!(
                        "Error getting PortInfo for switch port 0 on path [{}]: {}",
                        Fabric::format_path(&path),
                        e
                    );
                }
            }
        }

        for p in 1..=num_ports {
            log::trace!(
                "Fetching PortInfo for port {} on path [{}], hop_cnt: {}",
                p,
                Fabric::format_path(&path),
                hop_cnt
            );

            // Any failure on one port becomes a Down placeholder rather than
            // aborting the node: aborting used to drop the whole switch and,
            // with it, everything discovery would have reached through it.
            let (port, is_placeholder) = match self.fetch_port_info(path, p, hop_cnt) {
                Ok(port) => (port, false),
                Err(e) => {
                    if e.kind() == io::ErrorKind::TimedOut {
                        log::debug!(
                            "Timeout getting PortInfo for port {} on path [{}], inserting placeholder",
                            p,
                            Fabric::format_path(&path)
                        );
                    } else {
                        log::warn!(
                            "Error getting PortInfo for port {} on path [{}]: {}. Inserting placeholder",
                            p,
                            Fabric::format_path(&path),
                            e
                        );
                        self.mad_errors += 1;
                    }
                    (
                        Port {
                            number: p,
                            link_state: enums::IbPortLinkLayerState::Down,
                            phys_state: enums::IbPortPhyState::Disabled,
                            lid: 0,
                            link_width_active: 0,
                            parent: Weak::new(),
                            remote_port: None,
                        },
                        true,
                    )
                }
            };

            Fabric::attach_port_to_node(node_arc, port, p, is_placeholder)?;
        }

        Ok(())
    }

    // NOTE: Topology-specific discovery traversal lives in `ibmad::discovery::ib` (default IB)
    // and `ibmad::discovery::nvlink` (NVLink fabrics).
}
