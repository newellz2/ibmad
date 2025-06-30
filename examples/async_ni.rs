use std::{collections::HashMap, io::{self, Read, Write}, sync::Arc, time::{Duration, Instant}};

use ibmad::{enums, mad::{self, ib_user_mad}};
use tokio::{io::{unix::AsyncFd, Interest}, sync};


#[derive(Debug, Copy, Clone)]
pub struct TimedSmp {
    smp: ib_user_mad,
    timestamp: Instant,
}

#[derive(Debug)]
pub enum AsyncMessage {
    Exit(),
    Send(TimedSmp),
    Recv(TimedSmp),
}

fn build_umad(agent_id: u32, timeout: u32, retries: u32) -> mad::ib_user_mad {
    let umad = mad::ib_user_mad{
        agent_id: agent_id,
        status: 0x0,
        timeout_ms: timeout,
        retries: retries,
        length: 0,
        addr:  mad::ib_mad_addr {
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

fn build_mad(
    mgmt_class: u8,
    method: u8,
    attr_id: enums::SmiAttrID,
    attr_mod: u32,
    hop_cnt: u8,
    tid: u64,
) -> mad::ib_mad {
    let mad = mad::ib_mad{
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

    return mad
}

fn build_dr_smp(path: [u8; 64]) -> mad::dr_smp_mad {
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

fn build_dr_smp_umad( 
    path: [u8; 64], 
    attr_id: enums::SmiAttrID, 
    attr_mod: u32,
    hop_cnt: u8,
    tid : u64,
    timeout: u32,
    retries: u32,
    agent_id: u32,
    ) -> mad::ib_user_mad {
    
    let mut dr_smp = build_dr_smp(path);
    let mut mad = build_mad(
        enums::MadClasses::DirecteRoute as u8,
        enums::Methods::Get as u8,
        attr_id, 
        attr_mod, 
        hop_cnt,
        tid,
    );
    let mut umad = build_umad(
        agent_id,
        timeout,
        retries,
    );

    dr_smp.initial_path = path;

    // Assemble the UMAD
    let dr_bytes = dr_smp.to_bytes();
    mad.data[..dr_bytes.len()].copy_from_slice(&dr_bytes);

    let mad_bytes = mad.to_bytes();
    umad.data[..mad_bytes.len()].copy_from_slice(&mad_bytes);

    umad
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = env_logger::try_init();

    log::info!("Searching for InfiniBand CAs...");
    let cas = ibmad::ca::get_cas()?;
    let ib_ca = cas.first().ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "No InfiniBand CAs found")
    })?;
    log::info!("Found CA: {}", ib_ca.name);

    let mut port = ibmad::mad::open_port(&ib_ca)?;
    log::info!("Port opened successfully.");
    let agent_id = ibmad::mad::register_agent(&mut port, 0x81)?;
    log::info!("Successfully registered agent with ID: {}", agent_id);

    let barrier = Arc::new(sync::Barrier::new(4));
    
    let read_file = port.file;
    let write_file = read_file.try_clone()?;

    log::info!("Successfully cloned file descriptor for concurrent access.");

    let reader_port = AsyncFd::new(read_file)?;
    let writer_port = AsyncFd::new(write_file)?;

    let (main_tx, mut writer_rx) = tokio::sync::mpsc::channel::<AsyncMessage>(32);
    let (writer_tx, mut reader_rx) = tokio::sync::mpsc::channel::<AsyncMessage>(64);
    let (reader_tx, mut processor_rx) = tokio::sync::mpsc::channel::<AsyncMessage>(64);


    let reader_barrier = barrier.clone();
    let _reader_handle = tokio::spawn(async move {
        let mut iteration = 0;
        let reader_port = reader_port;
        log::info!("[Reader] Task started.");

        reader_barrier.wait().await;

        while let Some(msg) =  reader_rx.recv().await {

            match msg {
                AsyncMessage::Exit() => {
                    let _ = reader_tx.send(
                        AsyncMessage::Exit()
                    ).await;
                    break;
                }
                AsyncMessage::Recv(t_smp) =>{
                    let _ = reader_tx.send(AsyncMessage::Send(t_smp)).await;
                }
                _ => {},
            }

            log::info!("[Reader] iteration: {}, umad: {:?}", iteration, msg);
            iteration += 1;

            match reader_port.ready(Interest::READABLE).await {
                Ok(mut guard) => {
                    let mut buf: [u8; 320] = [0u8; 320];

                    match guard.try_io(|inner| inner.get_ref().read(&mut buf)) {
                        Ok(Ok(bytes_read)) => {
                            log::info!("[Reader] Received {} bytes.", bytes_read);
                            if bytes_read == 320 {
                                let umad= mad::ib_user_mad::from_bytes(&buf).unwrap();
                                let mad = mad::ib_mad::from_bytes(&umad.data).unwrap();
                                let dr_mad = mad::dr_smp_mad::from_bytes(&mad.data).unwrap();
                                let ni = mad::node_info::from_bytes(&dr_mad.attr_layout).unwrap();
                                log::info!("[Reader] NodeInfo {:?}", ni);
                                let _ = reader_tx.send(
                                    AsyncMessage::Recv(
                                        TimedSmp { smp: umad, timestamp: Instant::now() }
                                    )
                                ).await;

                            } else {
                                log::info!("[Reader] Read fewer than 320 bytes.");
                            }
                            guard.clear_ready();
                        }

                        Ok(Err(e)) => {

                            match e.kind() {
                                io::ErrorKind::UnexpectedEof => {
                                    log::error!("[Reader] Unexpected EOF: {}", e);
                                    guard.clear_ready();
                                }
                                _ => {
                                    log::error!("[Reader] I/O error: {}", e);
                                    guard.clear_ready();
                                }
                            }

                        }

                        Err(ref would_block) => {
                            log::error!("[Reader] would block: {:?}", would_block);
                            guard.clear_ready();
                        }
                    }
                    
                }
                // Unable to get guard
                Err(e) => {
                    log::error!("[Reader] Await readiness failed: {}", e);
                    break;
                }
            }
        }

        log::info!("[Reader] Exiting");
        reader_barrier.wait().await;

    });

    let write_barrier = barrier.clone();
    let writer_handle = tokio::spawn(async move {
        let writer_port = writer_port;
        log::info!("[Writer] Task started. Waiting for packets to send.");

        write_barrier.wait().await;

        while let Some(msg) = writer_rx.recv().await {
            match msg {
                AsyncMessage::Exit() => {
                    let _ = writer_tx.send(AsyncMessage::Exit()).await;
                    break;
                }
                _ => {},
            }

            loop {
                match msg {
                    AsyncMessage::Send(t_smp) => {
                        match writer_port.ready(Interest::WRITABLE).await {
                            Ok(mut guard) => {
                                
                                match writer_port.get_ref().write(&t_smp.smp.to_bytes()) {
                                    Ok(bytes_written) => {
                                        log::info!("[Writer] Successfully wrote {} bytes.", bytes_written);
                                        let _ = writer_tx.send(
                                            AsyncMessage::Recv(t_smp)
                                        ).await;
                                        break;
                                    }
                                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                                        guard.clear_ready();
                                        continue;
                                    }
                                    Err(e) => {
                                        log::error!("[Writer] I/O error: {}", e);
                                        break;
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!("[Writer] Await readiness failed: {}", e);
                                break;
                            }
                        }
                    },
                    _ => {}
                }

            }
        }

        write_barrier.wait().await;
        log::info!("[Writer] Channel closed. Shutting down writer task.");
    });


    let processor_barrier = barrier.clone();

    let _processor_handle = tokio::spawn(async move {
        log::info!("[Processor] Task started.");
        processor_barrier.wait().await;

        let mut smps: HashMap<u64, TimedSmp> = HashMap::new();

        loop {
            tokio::select! {
                Some(msg) = processor_rx.recv() => {
                    match msg {

                        AsyncMessage::Send(sent_smp) => {
                            if let Some(mad) = mad::ib_mad::from_bytes(&sent_smp.smp.data) {
                                let tid = u64::from_be(mad.tid);
                                log::info!("[Processor] Tracking sent packet with TID: {}", tid);
                                smps.insert(tid, sent_smp);
                            } else {
                                log::warn!("[Processor] Failed to parse MAD from a sent notification.");
                            }
                        },

                        AsyncMessage::Recv(received_smp) => {
                            if let Some(mad) = mad::ib_mad::from_bytes(&received_smp.smp.data) {
                                let tid = u64::from_be(mad.tid) & 0x0000_ffff;

                                if let Some(sent_smp) = smps.remove(&tid) {
                                    let rtt = received_smp.timestamp.duration_since(sent_smp.timestamp);
                                    log::info!("[Processor] Matched response for TID: {}. RTT: {:?}", tid, rtt);
                                } else {
                                    log::warn!("[Processor] Received an unmatched response with TID: {}", tid);
                                }
                            } else {
                                log::warn!("[Processor] Failed to parse MAD from a received packet.");
                            }
                        },

                        AsyncMessage::Exit() => {
                            log::info!("[Processor] Exit message received.");
                            break; // Exit the loop
                        }
                    }
                },

                else => {
                    break;
                }
            }
        }
        if !smps.is_empty() {
            log::warn!("[Processor] Shutting down with {} requests still in flight.", smps.len());
        }

        processor_barrier.wait().await;
        log::info!("[Processor] Task finished.");
    });

    log::info!("[Main] Will send 3 packets to the Writer task.");
    barrier.wait().await;
    for i in 0..20_000 {
        log::info!("[Main] Building packet #{}", i);
        let mut path: [u8; 64] = [0; 64];
        path[1] = 1; // HCA
        path[2] = 3; // Switch

        let hop_cnt = 2;

        let umad_to_send = build_dr_smp_umad(
            path, enums::SmiAttrID::NodeInfo, 0x0, hop_cnt,
            i, 
            50, 1, agent_id,
        );

        if let Err(e) = main_tx.send(
            AsyncMessage::Send(
                TimedSmp { 
                    smp: umad_to_send,
                    timestamp: Instant::now()
                }
            )
        ).await {
            log::error!("[Main] Failed to send packet to writer channel: {}", e);
            break;
        }

        tokio::time::sleep(Duration::from_millis(0)).await;
    }

    log::info!("[Main] Send exit message.");
    if let Err(e) = main_tx.send(
        AsyncMessage::Exit()
    ).await {
        log::error!("[Main] Failed to Exit message to writer channel: {}", e);
    }
    barrier.wait().await;


    drop(main_tx);
    log::info!("[Main] Dropped transmitter. Writer task will shut down after sending remaining packets.");

    let _ = writer_handle.await;
    log::info!("[Main] Writer task finished.");


    log::info!("[Main] Exiting...");

    Ok(())
}