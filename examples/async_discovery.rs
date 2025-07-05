use hashbrown::HashMap;
use std::{
    collections::HashSet,
    fs::File,
    io::{self, Read, Write},
    sync::{
        Arc, RwLock, Weak,
        atomic::{AtomicU32, AtomicU64},
    },
    time::{Duration, Instant},
};

use tokio::io::{Interest, unix::AsyncFd};

#[derive(Debug, Copy, Clone)]
pub struct TimedSmp {
    guid: Option<u64>,
    smp: ibmad::mad::ib_user_mad,
    timestamp: Instant,
}

#[derive(Debug)]
pub enum AsyncMessage {
    Exit(),
    Send(TimedSmp),
    Recv(TimedSmp),
    PollRecv(),
    Error(TimedSmp),
    Timeout(),
}

pub struct AsyncMadHandler {
    mad_port: ibmad::mad::IbMadPort,
    agent_id: u32,
    timeout: u32,
    tx_chan: tokio::sync::mpsc::Sender<AsyncMessage>,
    send_rx_chan: Option<tokio::sync::mpsc::Receiver<AsyncMessage>>,
    recv_rx_chan: Option<tokio::sync::mpsc::Receiver<AsyncMessage>>,
}

impl AsyncMadHandler {
    async fn handle_recv(
        async_fd: &AsyncFd<File>,
        timeout: u32,
    ) -> Result<ibmad::mad::ib_user_mad, io::Error> {
        let timeout_dur = Duration::from_millis(timeout as u64);
        let mut guard =
            match tokio::time::timeout(timeout_dur, async_fd.ready(Interest::READABLE)).await {
                Ok(Ok(guard)) => guard,
                Ok(Err(e)) => return Err(e),
                Err(_) => {
                    log::warn!("[handle_recv] Read operation timed out after {:?}", timeout_dur);
                    return Err(io::Error::new(io::ErrorKind::TimedOut, "Read timed out"));
                }
            };

        let mut buf: [u8; 320] = [0; 320];
        match guard.try_io(|inner| inner.get_ref().read_exact(&mut buf)) {
            Ok(Ok(_)) => {
                guard.clear_ready();
                log::trace!("[handle_recv] Successfully read from UMAD port.");
            }
            Ok(Err(e)) => {
                guard.clear_ready();
                log::error!("[handle_recv] Error with read: {:?}", e);
            }
            Err(e) => {
                guard.clear_ready();
                log::error!("[handle_recv] Error with read guard: {:?}", e);

            }
        }

        ibmad::mad::ib_user_mad::from_bytes(&buf)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Could not parse UMAD"))
    }

    async fn handle_send(async_fd: &AsyncFd<File>, t_smp: TimedSmp) -> Result<(), io::Error> {
        let mut guard = async_fd.ready(Interest::WRITABLE).await?;
        let umad_bytes = t_smp.smp.to_bytes();

        match guard.try_io(|inner| inner.get_ref().write_all(&umad_bytes)) {
            Ok(Ok(_)) => {
                guard.clear_ready();
                log::trace!("[handle_send] Successfully wrote from UMAD port.");
                Ok(())
            }
            Ok(Err(e)) => {
                guard.clear_ready();
                log::error!("[handle_send] Error with write: {:?}", e);
                Err(e)
            }
            Err(e) => {
                guard.clear_ready();
                log::error!("[handle_send]  Error with write guard: {:?}", e);
                Err(
                    io::Error::new(io::ErrorKind::Other, "[handle_send] I/O operation would block")
                )
            }
        }
    }

    fn spawn_recv_task(
        &self,
        file: std::fs::File,
        mut rx_chan: tokio::sync::mpsc::Receiver<AsyncMessage>,
        tx_chan: tokio::sync::mpsc::Sender<AsyncMessage>,
    ) -> Result<(), io::Error> {
        let async_r_file: AsyncFd<File> = AsyncFd::new(file)?;
        let timeout = *&self.timeout;

        let _read_handle = tokio::spawn(async move {
            let async_r_file = async_r_file;

            log::info!("[Reader] Task started. Waiting for packets to send.");

            while let Some(msg) = rx_chan.recv().await {
                match msg {
                    AsyncMessage::Exit() => {
                        log::info!("[Reader] Sending exit message.");
                        break;
                    }
                    AsyncMessage::PollRecv() => {
                        log::trace!("[Reader] Received PollRecv message");
                        let r = AsyncMadHandler::handle_recv(&async_r_file, timeout).await;
                        match r {
                            Ok(umad) => {
                                if log::log_enabled!(log::Level::Debug) {
                                    let tid = umad.get_tid().unwrap() & 0x0000_0000_ffff_ffff;
                                    log::trace!("Received UMAD TID: {}", tid);
                                }
                                let _ = tx_chan
                                    .send(AsyncMessage::Recv(TimedSmp {
                                        guid: None,
                                        smp: umad,
                                        timestamp: Instant::now(),
                                    }))
                                    .await;
                            }
                            Err(e) if e.kind() == io::ErrorKind::TimedOut => {
                                log::info!("Recv Timeout: {:?}", e);
                                let _ = tx_chan.send(AsyncMessage::Timeout()).await;
                            }
                            Err(e) => {
                                log::error!("Recv error: {:?}", e);
                            }
                        }
                    }
                    _ => {
                        log::error!("[recv] Unknown message type");
                    }
                }
            }

            log::info!("[Reader] Exiting");
        });

        Ok(())
    }

    fn spawn_send_task(
        &self,
        file: std::fs::File,
        mut rx_chan: tokio::sync::mpsc::Receiver<AsyncMessage>,
        tx_chan: tokio::sync::mpsc::Sender<AsyncMessage>,
    ) -> Result<(), io::Error> {
        let async_wr_file = AsyncFd::new(file)?;

        let _writer_handle = tokio::spawn(async move {
            log::info!("[send] Task started. Waiting for packets to send.");

            while let Some(msg) = rx_chan.recv().await {
                match msg {
                    AsyncMessage::Exit() => {
                        log::info!("[send] Sending exit message.");
                        break;
                    }
                    AsyncMessage::Send(t_smp) => {
                        log::trace!("[send] Received send message: {:?}", t_smp);
                        let _ = AsyncMadHandler::handle_send(&async_wr_file, t_smp).await;
                        let _ = tx_chan.send(AsyncMessage::PollRecv()).await;
                    }
                    _ => {
                        log::info!("[send] Unknown message type.");
                    }
                }
            }

            log::info!("[send] Exiting");
        });

        Ok(())
    }

    pub fn new(hca: &str) -> Result<Self, io::Error> {
        let ca = ibmad::ca::get_ca(hca)?;
        let mut mad_port = ibmad::mad::open_port(&ca)?;
        let agent_id = ibmad::mad::register_agent(&mut mad_port, 0x81)?;
        let (tx_chan, rx_chan) = tokio::sync::mpsc::channel::<AsyncMessage>(128);

        Ok(Self {
            mad_port,
            agent_id,
            timeout: 1000,
            tx_chan,
            send_rx_chan: Some(rx_chan),
            recv_rx_chan: None,
        })
    }

    pub fn start(&mut self) -> Result<(), io::Error> {

        let read_file = self.mad_port.file.try_clone()?;
        let write_file = self.mad_port.file.try_clone()?;

        let rx = self.send_rx_chan.take().unwrap();

        let (tx_recv_chan, rx_recv_chan) = tokio::sync::mpsc::channel::<AsyncMessage>(1);
        let (tx_res_chan, rx_res_chan) = tokio::sync::mpsc::channel::<AsyncMessage>(256);

        self.recv_rx_chan = Some(rx_res_chan);

        let _ = &self.spawn_send_task(write_file, rx, tx_recv_chan);
        let _ = &self.spawn_recv_task(read_file, rx_recv_chan, tx_res_chan);

        Ok(())
    }
}

// Topology Types
pub type NodeRef = Arc<RwLock<Node>>;
pub type PortRef = Arc<RwLock<Port>>;
pub type WNodeRef = Weak<RwLock<Node>>;
pub type WPortRef = Weak<RwLock<Port>>;
pub type PathKey = String;

#[derive(Debug, Default)]
pub struct Node {
    pub guid: u64,
    pub desc: Option<String>,
    pub info: Option<ibmad::mad::node_info>,
    pub ports: HashMap<u8, PortRef>,
}

impl Node {
    fn new(guid: u64) -> Self {
        Self {
            guid,
            desc: None,
            info: None,
            ports: HashMap::new(),
        }
    }
}

#[derive(Debug)]
pub struct Port {
    pub port_num: u8,
    pub info: Option<ibmad::mad::port_info>,
    pub parent: WNodeRef,
    pub remote: Option<WPortRef>,
}

impl Port {
    fn new(port_num: u8, parent: &NodeRef) -> Self {
        Self {
            port_num,
            info: None,
            parent: Arc::downgrade(parent),
            remote: None,
        }
    }
}

#[derive(Default)]
pub struct Fabric {
    // Guid, Node
    nodes: dashmap::DashMap<u64, NodeRef>,
}

impl Fabric {
    pub fn get_node(&self, guid: u64) -> Option<NodeRef> {
        let node = match self.nodes.get(&guid) {
            Some(n) => Some(n.clone()),
            None => None,
        };
        node
    }
    pub fn get_or_create_node(&self, guid: u64) -> NodeRef {
        self.nodes
            .entry(guid)
            .or_insert_with(|| {
                log::debug!("[fabric] creating node 0x{:x}", guid);
                Arc::new(RwLock::new(Node::new(guid)))
            })
            .clone()
    }

    pub fn get_or_create_port(&self, guid: u64, port_num: u8) -> PortRef {
        let node = self.get_or_create_node(guid);
        {
            let mut n = node.write().unwrap();
            n.ports
                .entry(port_num)
                .or_insert_with(|| {
                    log::debug!("[fabric] creating port {}, parent: {:x}", port_num, guid);
                    Arc::new(RwLock::new(Port::new(port_num, &node)))
                })
                .clone()
        }
    }

    pub fn link_ports(&self, a: &PortRef, b: &PortRef) {
        a.write().unwrap().remote = Some(Arc::downgrade(b));
        b.write().unwrap().remote = Some(Arc::downgrade(a));
    }
}

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
        "0".to_string()
    } else {
        format!("0 -> {}", hop_vec.join(" -> "))
    }
}

fn build_umad(agent_id: u32, timeout: u32, retries: u32) -> ibmad::mad::ib_user_mad {
    let umad = ibmad::mad::ib_user_mad {
        agent_id: agent_id,
        status: 0x0,
        timeout_ms: timeout,
        retries: retries,
        length: 0,
        addr: ibmad::mad::ib_mad_addr {
            qpn: 0,
            qkey: ibmad::mad::IB_DEFAULT_QKEY.to_be(),
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

fn build_mad(
    mgmt_class: u8,
    method: u8,
    attr_id: ibmad::enums::SmiAttrID,
    attr_mod: u32,
    hop_cnt: u8,
    tid: u64,
) -> ibmad::mad::ib_mad {
    let mad = ibmad::mad::ib_mad {
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

    return mad;
}

fn build_dr_smp(path: [u8; 64]) -> ibmad::mad::dr_smp_mad {
    let dr_smp = ibmad::mad::dr_smp_mad {
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

fn build_dr_smp_umad(
    path: [u8; 64],
    attr_id: ibmad::enums::SmiAttrID,
    attr_mod: u32,
    hop_cnt: u8,
    tid: u64,
    timeout: u32,
    retries: u32,
    agent_id: u32,
) -> ibmad::mad::ib_user_mad {
    let mut dr_smp = build_dr_smp(path);
    let mut mad = build_mad(
        ibmad::enums::MadClasses::DirecteRoute as u8,
        ibmad::enums::Methods::Get as u8,
        attr_id,
        attr_mod,
        hop_cnt,
        tid,
    );
    let mut umad = build_umad(agent_id, timeout, retries);

    dr_smp.initial_path = path;

    // Assemble the UMAD
    let dr_bytes = dr_smp.to_bytes();
    mad.data[..dr_bytes.len()].copy_from_slice(&dr_bytes);

    let mad_bytes = mad.to_bytes();
    umad.data[..mad_bytes.len()].copy_from_slice(&mad_bytes);

    umad
}

fn get_hop_count(path: &[u8; 64]) -> u8 {
    path.iter().skip(1).take_while(|&&p| p != 0).count() as u8
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = env_logger::try_init();
    let mut fabric = Fabric {
        ..Default::default()
    };

    let hca = format!("mlx5_0");
    let mut async_handler = AsyncMadHandler::new(&hca)?;
    async_handler.start()?;

    let agent_id = async_handler.agent_id;
    let timeout = async_handler.timeout;

    let mut tx_chan = async_handler.tx_chan.clone();
    let mut rx_chan = async_handler.recv_rx_chan.take().unwrap();

    let tid = Arc::new(AtomicU32::new(0));
    let requests_sent = Arc::new(AtomicU64::new(0));
    let responses_received = Arc::new(AtomicU64::new(0));

    let mut smp_map: HashMap<u32, TimedSmp> = HashMap::new();
    let mut visited: HashSet<u64> = HashSet::new();

    let path = [0; 64];
    let _ = tx_chan
        .send(AsyncMessage::Send(build_and_store_smp(
            &mut smp_map,
            (path, 0x11, 0),
            tid.clone(),
            None,
            agent_id,
            timeout
        )))
        .await;

    requests_sent.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    let start_ts = Instant::now();
    loop {

        if tx_chan.capacity() == tx_chan.max_capacity() && smp_map.is_empty() {
            log::info!("[main] No more outstanding requests and channel is empty, exiting.");
            break;
        }

        tokio::select! {
            biased;

            maybe_msg = rx_chan.recv() => {
                process_response(
                    maybe_msg,
                    &mut smp_map,
                    requests_sent.clone(),
                    responses_received.clone(),
                    &mut visited,
                    &mut tx_chan,
                    tid.clone(),
                    &mut fabric,
                    agent_id,
                    timeout,
                ).await;
            },
            else => {
                break;
            }
        }

    }

    if log::log_enabled!(log::Level::Debug) {
        let mut nodes: Vec<_> = fabric.nodes.iter().collect();
        nodes.sort_by(|a, b| a.key().cmp(b.key()));

        for node_rwl in nodes {
            match node_rwl.read() {
                Ok(n) => {
                    let desc = match &n.desc {
                        Some(s) => s,
                        _ => &"".to_string(),
                    };

                    log::debug!(
                        "Node Description: {:?}, GUID: 0x{:x}, Ports: {}",
                        desc,
                        n.guid,
                        n.ports.len()
                    );
                    let mut ports: Vec<_> = n.ports.iter().collect();
                    ports.sort_by(|a, b| a.0.cmp(b.0));

                    for p in ports {
                        let port_guard = p.1.read().unwrap();
                        log::debug!(
                            "   - Port: {}, HasPortInfo: {}, HasRemotePortInfo: {}",
                            port_guard.port_num,
                            port_guard.info.is_some(),
                            port_guard.remote.is_some(),
                        );

                        if let Some(remote_port_weak) = &port_guard.remote {
                            if let Some(remote_port_arc) = remote_port_weak.upgrade() {
                                let remote_port = remote_port_arc.read().unwrap();
                                log::debug!(
                                    "     Remote_Port: {}, HasPortInfo: {}, Parent: {:?}",
                                    remote_port.port_num,
                                    remote_port.info.is_some(),
                                    remote_port
                                        .parent
                                        .upgrade()
                                        .unwrap_or_default()
                                        .read()
                                        .unwrap()
                                        .desc,
                                );
                            }
                        }
                    }
                }
                Err(_e) => {}
            };
        }
    }

    let sent = requests_sent.load(std::sync::atomic::Ordering::Relaxed);
    let recv = responses_received.load(std::sync::atomic::Ordering::Relaxed);

    log::info!("[main] Sent: {}, Received: {}", sent, recv);

    let end_ts = start_ts.elapsed();
    log::info!("[main] Elapsed {}", end_ts.as_secs_f64());

    let _ = tx_chan.send(AsyncMessage::Exit()).await;

    Ok(())
}

async fn recv_nodedesc(
    dr_smp: ibmad::mad::dr_smp_mad,
    sent_smp: TimedSmp,
    path: [u8; 64],
    fabric: &mut Fabric,
) {
    let guid = sent_smp.guid.unwrap();
    let node_desc_bytes = &dr_smp.attr_layout[..ibmad::mad::node::NODE_DESC_LENGTH];
    let mut node_desc = String::from_utf8_lossy(node_desc_bytes)
        .trim_end_matches('\0')
        .to_string();

    if let Some(null_idx) = node_desc.find('\0') {
        node_desc.truncate(null_idx);
    }
    log::debug!(
        "[recv_nodedesc] Received NodeDesc MAD for guid 0x{:x}, nodedesc: {}, path: {}",
        guid,
        node_desc,
        format_path(&path)
    );
    let node: NodeRef = fabric.get_node(guid).unwrap();
    {
        let mut guard = node.write().unwrap();
        guard.desc = Some(node_desc);
    }
}

async fn recv_nodeinfo(
    node_info: ibmad::mad::node_info,
    sent_smp: TimedSmp,
    path: [u8; 64],
    requests_sent: Arc<AtomicU64>,
    smp_map: &mut HashMap<u32, TimedSmp>,
    visited: &mut HashSet<u64>,
    tx_chan: &mut tokio::sync::mpsc::Sender<AsyncMessage>,
    tid: Arc<AtomicU32>,
    fabric: &mut Fabric,
    agent_id: u32,
    timeout: u32,
) {
    let guid = node_info.node_guid;
    let local_portnum = node_info.local_port;
    let hop_cnt = get_hop_count(&path) as usize;
    let node_type = node_info.node_type;
    let nports = node_info.nports;

    let mut start = 0;

    match node_type {
        0x1 => {
            log::debug!("[recv_nodeinfo] HCA node type");
            start = 1;
        }
        0x2 => {
            log::debug!("[recv_nodeinfo] Switch node type");
            start = 0;
        }
        _ => {
            log::debug!("[recv_nodeinfo] Unkown node type");
        }
    }

    // Add node to fabric
    log::debug!("[recv_nodeinfo] creating or fetching node: 0x{0:x}", guid);
    let node: NodeRef = fabric.get_or_create_node(guid);
    {
        let mut guard = node.write().unwrap();
        guard.info = Some(node_info);
        if guard.ports.len() == 0 {
            guard.ports = HashMap::with_capacity(nports.into());
            for i in start..=nports {
                guard.ports.insert(
                    i,
                    Arc::new(RwLock::new(Port {
                        port_num: i,
                        info: None,
                        parent: Arc::downgrade(&node),
                        remote: None,
                    })),
                );
            }
        }
    }

    let remote_portnum = path[hop_cnt];
    log::debug!(
        "[recv_nodeinfo] Received NodeInfo Reponse, from_guid: 0x{:x}, guid: 0x{:x}, local_port: {}, path: {:?}, hop_cnt: {}, remote_port: {}",
        sent_smp.guid.unwrap_or(0),
        guid,
        local_portnum,
        format_path(&path),
        hop_cnt,
        remote_portnum
    );

    match sent_smp.guid {
        Some(from_guid) => {
            log::debug!(
                "[recv_nodeinfo] Linking: (0x{:x}:{}) <--> (0x{:x}:{})",
                from_guid,
                local_portnum,
                guid,
                remote_portnum
            );

            let remote_port_rwl = fabric.get_or_create_port(from_guid, remote_portnum);
            let local_port_rwl = fabric.get_or_create_port(guid, local_portnum);
            fabric.link_ports(&remote_port_rwl, &local_port_rwl);
        }
        _ => {}
    }

    if visited.contains(&guid) {
        log::debug!(
            "[recv_nodeinfo] Already visited: 0x{:x}, path: {:?}",
            guid,
            format_path(&path)
        );
        return;
    } else {
        visited.insert(guid);
    }

    // NodeDesc
    log::debug!(
        "[recv_portinfo] Building NodeDesc query through path: {}",
        format_path(&path)
    );
    let entry = (path, 0x10, 0);
    let _ = tx_chan
        .send(AsyncMessage::Send(build_and_store_smp(
            smp_map,
            entry,
            tid.clone(),
            Some(guid),
            agent_id,
            timeout
        )))
        .await;
    requests_sent.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    // PortInfo
    for i in start..=nports {
        if log::log_enabled!(log::Level::Debug) {
            log::debug!(
                "[recv_nodeinfo] Building PortInfo query for port: {}, path: {}",
                i,
                format_path(&path)
            );
        }

        let entry = (path, 0x15, i as u32);
        let _ = tx_chan
            .send(AsyncMessage::Send(build_and_store_smp(
                smp_map,
                entry,
                tid.clone(),
                Some(guid),
                agent_id,
                timeout
            )))
            .await;

        requests_sent.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

async fn recv_portinfo(
    port_info: ibmad::mad::port_info,
    sent_smp: TimedSmp,
    path: [u8; 64],
    requests_sent: Arc<AtomicU64>,
    smp_map: &mut HashMap<u32, TimedSmp>,
    tx_chan: &mut tokio::sync::mpsc::Sender<AsyncMessage>,
    tid: Arc<AtomicU32>,
    fabric: &mut Fabric,
    agent_id: u32,
    timeout: u32,
) {
    let local_port = port_info.local_portnum();

    let port_state = match port_info.port_state() {
        4 => ibmad::enums::IbPortLinkLayerState::Active,
        3 => ibmad::enums::IbPortLinkLayerState::Init,
        _ => ibmad::enums::IbPortLinkLayerState::Down,
    };

    let phy_port_state = match port_info.port_physical_state() {
        5 => ibmad::enums::IbPortPhyState::LinkUp,
        _ => ibmad::enums::IbPortPhyState::Polling,
    };

    let mad = ibmad::mad::ib_mad::from_bytes(&sent_smp.smp.data).unwrap();

    let mut new_path = path.clone();
    let hop_cnt = get_hop_count(&new_path);
    let hop_idx = hop_cnt + 1;
    let this_guid_opt = sent_smp.guid;
    let sent_portnum = u32::from_be(mad.attr_mod) as u8;

    log::debug!(
        "[recv_portinfo] logical_state: {:?}, phy_state: {:?}, path: {}: sent_port: {}, local_port: {}",
        port_state,
        phy_port_state,
        format_path(&path),
        sent_portnum,
        local_port
    );

    let mut actual_portnum = 0;
    match this_guid_opt {
        Some(guid) => {
            let node_rwl = fabric.get_or_create_node(guid);
            {
                let node_guard = node_rwl.write().unwrap();
                match node_guard.info {
                    Some(info) => {
                        log::debug!(
                            "[recv_portinfo] Found node for guid: 0x{:x}, type: {}, path: {}",
                            guid,
                            info.node_type,
                            format_path(&path)
                        );
                        match info.node_type {
                            0x1 => {
                                if path == [0; 64] {
                                    // HCA used for discovery
                                    log::debug!("[recv_portinfo] Discovery HCA");
                                    actual_portnum = 1;
                                    new_path[hop_idx as usize] = sent_portnum; // Expand the path to 0,1
                                } else {
                                    // Other HCA, do not explore past it *,1
                                    log::debug!(
                                        "[recv_portinfo] Reached endpoint HCA, stopping exploration on this path."
                                    );
                                    return;
                                }
                            }
                            0x2 => {
                                actual_portnum = sent_portnum;
                                new_path[hop_idx as usize] = sent_portnum;
                            }
                            _ => {
                                actual_portnum = 1;
                            }
                        }
                    }
                    _ => {
                        log::debug!("[recv_portinfo] node_type undefined");
                    }
                }
            }
            log::debug!(
                "[recv_portinfo] creating port: {}, guid: 0x{:x}",
                actual_portnum,
                guid
            );
            let port_rwl = fabric.get_or_create_port(guid, actual_portnum);
            {
                let mut port_guard = port_rwl.write().unwrap();
                port_guard.info = Some(port_info)
            }
        }
        _ => {}
    }
    log::debug!("[recv_portinfo] hop_cnt: {}, hop_idx: {}", hop_cnt, hop_idx);

    if (port_state == ibmad::enums::IbPortLinkLayerState::Active
        || port_state == ibmad::enums::IbPortLinkLayerState::Init)
        && phy_port_state == ibmad::enums::IbPortPhyState::LinkUp
    {
        // NodeInfo
        log::debug!(
            "[recv_portinfo] Building NodeInfo query through remote_port: {}, path: {:?}",
            local_port,
            new_path
        );
        let entry = (new_path, 0x11, 0x0);
        let _ = tx_chan
            .send(AsyncMessage::Send(build_and_store_smp(
                smp_map,
                entry,
                tid.clone(),
                this_guid_opt,
                agent_id,
                timeout
            )))
            .await;
        requests_sent.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

async fn process_response(
    maybe_msg: Option<AsyncMessage>,
    smp_map: &mut HashMap<u32, TimedSmp>,
    requests_sent: Arc<AtomicU64>,
    responses_received: Arc<AtomicU64>,
    visited: &mut HashSet<u64>,
    tx_chan: &mut tokio::sync::mpsc::Sender<AsyncMessage>,
    tid: Arc<AtomicU32>,
    fabric: &mut Fabric,
    agent_id: u32,
    timeout: u32,
) {
    match maybe_msg {
        Some(AsyncMessage::Recv(recv_t_smp)) => {
            let mad_tid = (recv_t_smp.smp.get_tid().unwrap() & 0x0000_0000_ffff_ffff) as u32;

            if let Some(sent_t_smp) = smp_map.remove(&mad_tid) {
                if log::log_enabled!(log::Level::Debug) {
                    let rtt = recv_t_smp.timestamp.duration_since(sent_t_smp.timestamp);
                    log::debug!(
                        "[process_response] - TID:{} - RTT: {} µs",
                        mad_tid,
                        rtt.as_micros()
                    );
                }
                let mad = ibmad::mad::ib_mad::from_bytes(&recv_t_smp.smp.data).unwrap();
                let attr_id = mad.attr_id;

                if log::log_enabled!(log::Level::Debug) {
                    let byte_string = ibmad::dump_bytes(&mad.to_bytes());
                    log::trace!("[process_response] - TID:{} - Recv UMAD:\n{}", mad_tid, byte_string);
                }

                match attr_id {
                    0x1000 => {
                        //NodeDesc
                        let dr_smp = ibmad::mad::dr_smp_mad::from_bytes(&mad.data).unwrap();
                        let path = dr_smp.initial_path;
                        if log::log_enabled!(log::Level::Debug) {
                            let guid = sent_t_smp.guid;
                            log::debug!(
                                "[process_response] - TID:{} - Received NodeDesc MAD, GUID: {:?}, path: {}",
                                mad_tid,
                                guid,
                                format_path(&path)
                            );
                        }
                        recv_nodedesc(dr_smp, sent_t_smp, path, fabric).await;
                    }
                    0x1100 => {
                        //NodeInfo
                        let dr_smp = ibmad::mad::dr_smp_mad::from_bytes(&mad.data).unwrap();
                        let path = dr_smp.initial_path;
                        let ni = ibmad::mad::node_info::from_bytes(&dr_smp.attr_layout).unwrap();
                        if log::log_enabled!(log::Level::Debug) {
                            let guid = ni.node_guid;
                            log::debug!(
                                "[process_response] - TID:{} - Received NodeInfo MAD, GUID: {:x}, path: {}",
                                mad_tid,
                                guid,
                                format_path(&path)
                            );
                        }

                        recv_nodeinfo(
                            ni,
                            sent_t_smp,
                            path,
                            requests_sent,
                            smp_map,
                            visited,
                            tx_chan,
                            tid,
                            fabric,
                            agent_id,
                            timeout,
                        )
                        .await;
                    }
                    0x1500 => {
                        //PortInfo

                        //Response Packet
                        let dr_smp = ibmad::mad::dr_smp_mad::from_bytes(&mad.data).unwrap();
                        let path = dr_smp.initial_path;
                        let pi = ibmad::mad::port_info::from_bytes(&dr_smp.attr_layout).unwrap();

                        if log::log_enabled!(log::Level::Debug) {
                            let guid = sent_t_smp.guid;
                            log::debug!(
                                "[process_response] - TID:{} - Received PortInfo MAD, guid: {:?}, path: {}",
                                mad_tid,
                                guid,
                                format_path(&path)
                            );
                            log::debug!(
                                "[process_response] - TID:{} - Received PortInfo MAD, state:{}, local_port: {}",
                                mad_tid,
                                pi.port_state(),
                                pi.local_portnum()
                            );
                        }

                        recv_portinfo(
                            pi,
                            sent_t_smp,
                            path,
                            requests_sent,
                            smp_map,
                            tx_chan,
                            tid,
                            fabric,
                            agent_id,
                            timeout,
                        )
                        .await;
                    }
                    _ => {
                        log::trace!("[process_response] - TID:{} - Unknown MAD type.", mad_tid)
                    }
                }
            } else {
                log::warn!(
                    "[process_response]- TID:{} -Received response for unknown TID.",
                    mad_tid
                );
            }

            responses_received.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        Some(AsyncMessage::Timeout()) => {
            log::warn!("[process_response] A receive operation timed out.");
            let request_deadline = Duration::from_millis(1000);

            responses_received.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            smp_map.retain(|tid, request| {
                if request.timestamp.elapsed() > request_deadline {
                    log::error!("[process_response] Request with TID {} has expired, removing...", tid);
                    false 
                } else {
                    true
                }
            });
        }
        Some(msg) => {
            log::info!("[process_response] Received unhandled message: {:?}", msg);
            responses_received.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        None => {
            log::error!("[process_response] A response channel closed prematurely.");
        }
    }
}

fn build_and_store_smp(
    smp_map: &mut HashMap<u32, TimedSmp>,
    entry: ([u8; 64], u16, u32),
    tid: Arc<AtomicU32>,
    guid: Option<u64>,
    agent_id: u32,
    timeout: u32,
) -> TimedSmp {
    let mut hop_cnt = 0;

    if entry.0 != [0; 64] {
        hop_cnt = get_hop_count(&entry.0);
    }

    let attr_id = match entry.1 {
        0x10 => ibmad::enums::SmiAttrID::NodeDesc,
        0x11 => ibmad::enums::SmiAttrID::NodeInfo,
        0x15 => ibmad::enums::SmiAttrID::PortInfo,
        u => {
            panic!("[build_and_store_smp] Unkown ATTR_ID: {}", u)
        }
    };

    let tid = tid.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    if log::log_enabled!(log::Level::Debug) {
        log::debug!(
            "[build_and_store_smp]- TID:{} - AttrId: 0x{:x} Port: {} Hop-count: {} for path: {}",
            tid,
            entry.1,
            entry.2,
            hop_cnt,
            format_path(&entry.0)
        );
    }

    let umad_to_send =
        build_dr_smp_umad(entry.0, attr_id, entry.2, hop_cnt, tid as u64, timeout, 1, agent_id);

    let t_smp = TimedSmp {
        guid: guid,
        smp: umad_to_send,
        timestamp: Instant::now(),
    };

    smp_map.insert(tid, t_smp);
    t_smp
}
