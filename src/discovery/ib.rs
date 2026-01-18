use std::{
    collections::{VecDeque},
    io,
    sync::{
        Arc, RwLock,
    },
};

use super::lib::{lock_err, Fabric, Port, START_PATH};
use crate::{enums};


impl Fabric {
    /// Default InfiniBand DR SMP discovery.
    ///
    /// Unlike NVLink discovery, this traversal uses all ports on a switch.
    /// Any active/init port on a discovered switch is eligible to be traversed.
    pub fn seq_discover(&mut self) -> Result<(), io::Error> {
        let mut stack: VecDeque<(Arc<RwLock<Port>>, [u8; 64])> = VecDeque::new();

        self.node_map.clear();
        self.nodes.clear();
        self.switches.clear();
        self.hcas.clear();
        self.dr_paths.clear();
        self.ni_timings.clear();
        self.mad_errors = 0;
        self.mad_timeouts = 0;
        self.mads_sent = 0;

        let start_ts = std::time::Instant::now();

        self.first_hop_discovery_ib(&mut stack)?;

        while let Some((local_port_arc, path_to_local_node)) = stack.pop_front() {
            let local_port_number = local_port_arc.read().map_err(lock_err)?.number;
            if local_port_number == 0 {
                continue;
            }

            if local_port_arc
                .read()
                .map_err(lock_err)?
                .remote_port
                .is_some()
            {
                continue;
            }

            let hop_cnt = Fabric::get_hop_count(&path_to_local_node) + 1;
            let mut path_to_remote_node = path_to_local_node;
            if (hop_cnt as usize) >= path_to_remote_node.len() {
                continue;
            }
            path_to_remote_node[hop_cnt as usize] = local_port_number;

            let remote_node_info = match self.fetch_node_info(path_to_remote_node, hop_cnt) {
                Ok(ni) => ni,
                Err(e) if e.kind() == io::ErrorKind::TimedOut => continue,
                Err(e) => {
                    log::warn!(
                        "Failed to fetch node info at path [{}]: {}",
                        Fabric::format_path(&path_to_remote_node),
                        e
                    );
                    continue;
                }
            };

            let node_guid = remote_node_info.node_guid;

            let remote_node_arc = if let Some(found_node) = self.node_map.get(&node_guid) {
                found_node.clone()
            } else {
                match self.discover_node(path_to_remote_node, hop_cnt) {
                    Ok(new_node_arc) => {
                        // If it's a switch, add all eligible ports to the traversal stack.
                        let is_switch = {
                            let node = new_node_arc.read().map_err(lock_err)?;
                            node.node_type == enums::IbNodeType::Switch
                        };
                        if is_switch {
                            let node = new_node_arc.read().map_err(lock_err)?;
                            for port_arc in node.ports.iter() {
                                let port = port_arc.read().map_err(lock_err)?;
                                if port.number == 0 {
                                    continue;
                                }
                                if port.link_state != enums::IbPortLinkLayerState::Active
                                    && port.link_state != enums::IbPortLinkLayerState::Init
                                {
                                    continue;
                                }
                                stack.push_back((port_arc.clone(), path_to_remote_node));
                            }
                        }
                        new_node_arc
                    }
                    Err(e) => {
                        log::warn!(
                            "Failed to discover new remote node at path [{}]: {}",
                            Fabric::format_path(&path_to_remote_node),
                            e
                        );
                        continue;
                    }
                }
            };

            // Link ports
            let remote_port_number = remote_node_info.local_port;
            let remote_node_guard = remote_node_arc.read().map_err(lock_err)?;
            if let Some(remote_port_arc) = remote_node_guard.ports.iter().find(|p| {
                p.read()
                    .map_or(false, |p_guard| p_guard.number == remote_port_number)
            }) {
                local_port_arc.write().map_err(lock_err)?.remote_port =
                    Some(Arc::downgrade(remote_port_arc));
                remote_port_arc.write().map_err(lock_err)?.remote_port =
                    Some(Arc::downgrade(&local_port_arc));
            } else {
                log::warn!(
                    "Inconsistent fabric: remote node 0x{:X} ('{}') reported port {} which was not found",
                    remote_node_guard.node_guid.to_be(),
                    remote_node_guard.description.as_deref().unwrap_or("N/A"),
                    remote_port_number
                );
                continue;
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

        let ts_diff = std::time::Instant::now() - start_ts;
        log::info!(
            "IB discovery complete. Found {} nodes ({} switches, {} HCAs) in {:.2}s. MADs Sent: {}, Timeouts: {}, Errors: {}",
            self.nodes.len(),
            self.switches.len(),
            self.hcas.len(),
            ts_diff.as_secs_f64(),
            self.mads_sent,
            self.mad_timeouts,
            self.mad_errors
        );

        Ok(())
    }

    fn first_hop_discovery_ib(
        &mut self,
        stack: &mut VecDeque<(Arc<RwLock<Port>>, [u8; 64])>,
    ) -> Result<(), io::Error> {
        let hop_cnt: u8 = 0;

        let first_node_arc = self.discover_node(START_PATH, hop_cnt).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Could not discover first-hop node: {:?}", e),
            )
        })?;

        let first_node = first_node_arc.read().map_err(lock_err)?;
        let first_node_is_switch = first_node.node_type == enums::IbNodeType::Switch;

        for port_arc in first_node.ports.iter() {
            let port = port_arc.read().map_err(lock_err)?;
            if port.number == 0 {
                continue;
            }
            if port.link_state != enums::IbPortLinkLayerState::Active
                && port.link_state != enums::IbPortLinkLayerState::Init
            {
                continue;
            }

            let mut path: [u8; 64] = [0; 64];
            let (path_index, neighbor_hop_cnt) = if first_node_is_switch {
                (hop_cnt as usize, hop_cnt + 1)
            } else {
                (hop_cnt as usize + 1, hop_cnt + 1)
            };

            if path_index >= path.len() {
                continue;
            }
            path[path_index] = port.number;

            let neighbor_node_arc = match self.discover_node(path, neighbor_hop_cnt) {
                Ok(n) => n,
                Err(_) => continue,
            };

            let neighbor_node = neighbor_node_arc.read().map_err(lock_err)?;
            if neighbor_node.node_type != enums::IbNodeType::Switch {
                continue;
            }

            // Seed traversal with all eligible ports on the first-hop switch.
            for neighbor_port_arc in neighbor_node.ports.iter() {
                let neighbor_port = neighbor_port_arc.read().map_err(lock_err)?;
                if neighbor_port.number == 0 {
                    continue;
                }
                if neighbor_port.link_state != enums::IbPortLinkLayerState::Active
                    && neighbor_port.link_state != enums::IbPortLinkLayerState::Init
                {
                    continue;
                }
                stack.push_back((neighbor_port_arc.clone(), path));
            }
        }

        Ok(())
    }
}
