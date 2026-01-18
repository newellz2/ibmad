use std::{
    collections::{HashSet, VecDeque},
    io,
    sync::{Arc, RwLock},
};

use super::lib::{lock_err, Fabric, Node, Port, START_PATH};
use crate::enums;

const NVLINK_RING_PORTS: [u8; 2] = [73, 74];

fn is_nvlink_ring_port(port_number: u8) -> bool {
    NVLINK_RING_PORTS.contains(&port_number)
}

impl Fabric {
    /// NVLink fabrics use a spine/leaf style topology where only specific ports (73/74)
    /// are used to traverse between switches. Other active ports are probed for endpoints
    /// but are not added to the main discovery stack.
    pub fn seq_discover_nvlink(&mut self) -> Result<(), io::Error> {
        let mut visited: HashSet<u64> = HashSet::new();
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

        self.first_hop_discovery_nvlink(&mut visited, &mut stack)?;

        log::debug!("Initial discovery stack size: {}", stack.len());

        while let Some((local_port_arc, path_to_local_node)) = stack.pop_front() {
            let (parent_desc, local_port_number) = {
                let local_port = local_port_arc.read().map_err(lock_err)?;
                let parent_arc = local_port
                    .parent
                    .upgrade()
                    .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Parent node not found"))?;
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

            if local_port_number == 0 {
                continue;
            }

            // Skip already-linked ports
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

            if (hop_cnt as usize) < path_to_remote_node.len() {
                path_to_remote_node[hop_cnt as usize] = local_port_number;
            } else {
                log::warn!("Path too long, cannot discover beyond port {}", local_port_number);
                continue;
            }

            let remote_node_info = match self.fetch_node_info(path_to_remote_node, hop_cnt) {
                Ok(ni) => ni,
                Err(e) if e.kind() == io::ErrorKind::TimedOut => {
                    continue;
                }
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
                        // If it's a switch, probe all non-spine ports for endpoints and add only spine ports.
                        let is_switch = {
                            let node = new_node_arc.read().map_err(lock_err)?;
                            node.node_type == enums::IbNodeType::Switch
                        };

                        if is_switch {
                            if let Err(e) = self.discover_connected_nodes_nvlink(
                                &new_node_arc,
                                path_to_remote_node,
                                hop_cnt,
                            ) {
                                log::warn!("Error discovering connected nodes: {}", e);
                            }

                            let node = new_node_arc.read().map_err(lock_err)?;
                            for port_arc in node.ports.iter() {
                                let port = port_arc.read().map_err(lock_err)?;
                                if is_nvlink_ring_port(port.number)
                                    && (port.link_state == enums::IbPortLinkLayerState::Active
                                        || port.link_state == enums::IbPortLinkLayerState::Init)
                                {
                                    stack.push_front((port_arc.clone(), path_to_remote_node));
                                }
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
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "Inconsistent fabric: remote node 0x{:X} ('{}') reported port {} which was not found",
                        remote_node_guard.node_guid.to_be(),
                        remote_node_guard.description.as_deref().unwrap_or("N/A"),
                        remote_port_number
                    ),
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

        let ts_diff = std::time::Instant::now() - start_ts;
        log::info!(
            "NVLink discovery complete. Found {} nodes ({} switches, {} HCAs) in {:.2}s. MADs Sent: {}, Timeouts: {}, Errors: {}",
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

    fn first_hop_discovery_nvlink(
        &mut self,
        visited: &mut HashSet<u64>,
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
        visited.insert(first_node.node_guid);

        for port_arc in first_node.ports.iter() {
            let port_number = port_arc.read().map_err(lock_err)?.number;

            if port_number == 0 {
                continue;
            }

            let mut path: [u8; 64] = [0; 64];
            let (path_index, neighbor_hop_cnt) = if first_node_is_switch {
                (hop_cnt as usize, hop_cnt)
            } else {
                (hop_cnt as usize + 1, hop_cnt + 1)
            };

            if path_index >= path.len() {
                continue;
            }

            path[path_index] = port_number;

            match self.discover_node(path, neighbor_hop_cnt) {
                Ok(neighbor_node_arc) => {
                    let neighbor_node = neighbor_node_arc.read().map_err(lock_err)?;
                    visited.insert(neighbor_node.node_guid);

                    if neighbor_node.node_type == enums::IbNodeType::Switch {
                        drop(neighbor_node);

                        if let Err(e) =
                            self.discover_connected_nodes_nvlink(&neighbor_node_arc, path, neighbor_hop_cnt)
                        {
                            log::warn!("Error discovering connected nodes: {}", e);
                        }

                        let neighbor_node = neighbor_node_arc.read().map_err(lock_err)?;
                        for neighbor_port_arc in neighbor_node.ports.iter() {
                            let neighbor_port = neighbor_port_arc.read().map_err(lock_err)?;
                            if is_nvlink_ring_port(neighbor_port.number)
                                && (neighbor_port.link_state == enums::IbPortLinkLayerState::Active
                                    || neighbor_port.link_state == enums::IbPortLinkLayerState::Init)
                            {
                                stack.push_front((neighbor_port_arc.clone(), path));
                            }
                        }
                    }
                }
                Err(_e) => {
                    continue;
                }
            }
        }

        Ok(())
    }

    /// NVLink helper: probe all active non-spine ports on a switch, creating links, but
    /// intentionally do not add them to the traversal stack.
    fn discover_connected_nodes_nvlink(
        &mut self,
        switch_arc: &Arc<RwLock<Node>>,
        path_to_switch: [u8; 64],
        base_hop_cnt: u8,
    ) -> Result<(), io::Error> {
        let ports_to_probe: Vec<(Arc<RwLock<Port>>, u8)> = {
            let switch = switch_arc.read().map_err(lock_err)?;
            switch
                .ports
                .iter()
                .filter_map(|port_arc| {
                    let port = port_arc.read().ok()?;
                    // Skip port 0, inactive ports, and ports 73/74 (handled by main discovery)
                    if port.number == 0
                        || is_nvlink_ring_port(port.number)
                        || port.remote_port.is_some()
                        || (port.link_state != enums::IbPortLinkLayerState::Active
                            && port.link_state != enums::IbPortLinkLayerState::Init)
                    {
                        return None;
                    }
                    Some((port_arc.clone(), port.number))
                })
                .collect()
        };

        for (local_port_arc, port_number) in ports_to_probe {
            let hop_cnt = base_hop_cnt + 1;
            let mut path_to_remote = path_to_switch;

            if (hop_cnt as usize) >= path_to_remote.len() {
                continue;
            }
            path_to_remote[hop_cnt as usize] = port_number;

            let remote_node_info = match self.fetch_node_info(path_to_remote, hop_cnt) {
                Ok(ni) => ni,
                Err(e) if e.kind() == io::ErrorKind::TimedOut => {
                    continue;
                }
                Err(_) => {
                    continue;
                }
            };

            let node_guid = remote_node_info.node_guid;

            let remote_node_arc = if let Some(found_node) = self.node_map.get(&node_guid) {
                found_node.clone()
            } else {
                match self.discover_node(path_to_remote, hop_cnt) {
                    Ok(new_node) => new_node,
                    Err(_) => continue,
                }
            };

            let remote_port_number = remote_node_info.local_port;
            let remote_node_guard = remote_node_arc.read().map_err(lock_err)?;

            if let Some(remote_port_arc) = remote_node_guard.ports.iter().find(|p| {
                p.read()
                    .map_or(false, |pg| pg.number == remote_port_number)
            }) {
                local_port_arc.write().map_err(lock_err)?.remote_port =
                    Some(Arc::downgrade(remote_port_arc));
                remote_port_arc.write().map_err(lock_err)?.remote_port =
                    Some(Arc::downgrade(&local_port_arc));
            }
        }

        Ok(())
    }
}

