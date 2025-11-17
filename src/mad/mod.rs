use std::fs;
use std::io::{Read, Write};
use std::os::fd::{AsFd, AsRawFd};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::{io, mem::MaybeUninit};

use crate::{ca::IbCa, ib_user_mad_enable_pkey, ib_user_mad_reg_req};
use crate::{dump_bytes, ib_user_mad_register_agent};

pub mod dr_smp;
pub mod helpers;
pub mod node;
pub mod perf;
pub mod port;
pub mod types;

pub use dr_smp::dr_smp_mad;
pub use node::node_info;
pub use perf::perf_mad;
pub use port::port_info;
pub use types::{ib_mad, ib_mad_addr, ib_user_mad};

use nix::poll::{PollFd, PollFlags, PollTimeout, poll};

pub const IB_MGMT_CLASS_PERFORMANCE: u8 = 0x4;
pub const IB_MGMT_CLASS_LID_ROUTED_SMP: u8 = 0x1;
pub const IB_MGMT_CLASS_DIRECT_ROUTED_SMP: u8 = 0x81;

pub const IB_DEFAULT_QKEY: u32 = 0x80010000;

#[derive(Debug)]
pub struct IbMadPort {
    pub file: fs::File,
}

pub struct IbMadPortAsync {
    pub file: tokio::fs::File,
}

fn open_umad_device(path: &Path) -> Result<IbMadPort, io::Error> {
    match fs::File::options().read(true).write(true).open(path) {
        Ok(file) => {
            let mad_port = IbMadPort { file };
            let fd = mad_port.file.as_raw_fd();
            let r = unsafe { ib_user_mad_enable_pkey(fd) };
            match r {
                Ok(rc) => {
                    log::debug!(
                        "open_umad_device - Successfully enabled PKeys on {:?}, rc: {}",
                        path,
                        rc
                    );
                    Ok(mad_port)
                }
                Err(e) => {
                    log::debug!(
                        "open_umad_device - Error enabling PKeys on {:?}: {}",
                        path,
                        e
                    );
                    Err(io::Error::new(io::ErrorKind::Other, e))
                }
            }
        }
        Err(e) => {
            log::debug!(
                "open_umad_device - Error opening character device {:?}: {}",
                path,
                e
            );
            Err(io::Error::new(io::ErrorKind::Other, e))
        }
    }
}

pub fn open_port(hca: &IbCa) -> Result<IbMadPort, io::Error> {
    if let Some(dev_paths) = &hca.dev_paths {
        if let Some(path) = &dev_paths.umad_dev_path {
            return open_umad_device(path);
        }
        log::debug!(
            "open_port - No UMAD device found for {}, falling back to SMP device if available",
            hca.name
        );
        if let Some(path) = &dev_paths.smi_umad_dev_path {
            return open_umad_device(path);
        }
        log::debug!("open_port - HCA has no UMAD character device");
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            io::Error::other("HCA has no UMAD character device".to_string()),
        ))
    } else {
        log::debug!("open_port - HCA has no character devices");
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            io::Error::other("HCA has no character devices".to_string()),
        ))
    }
}

pub fn open_smp_port(hca: &IbCa) -> Result<IbMadPort, io::Error> {
    if let Some(dev_paths) = &hca.dev_paths {
        if let Some(path) = &dev_paths.smi_umad_dev_path {
            return open_umad_device(path);
        }
        log::debug!(
            "open_smp_port - No SMI UMAD device found for {}, falling back to general UMAD",
            hca.name
        );
        if let Some(path) = &dev_paths.umad_dev_path {
            return open_umad_device(path);
        }
        log::debug!("open_smp_port - HCA has no SMI UMAD character device");
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            io::Error::other("HCA has no SMI UMAD character device".to_string()),
        ))
    } else {
        log::debug!("open_smp_port - HCA has no character devices");
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            io::Error::other("HCA has no character devices".to_string()),
        ))
    }
}

fn next_tid() -> u64 {
    static NEXT_TID: AtomicU64 = AtomicU64::new(1);
    let mut tid = NEXT_TID.fetch_add(1, Ordering::Relaxed);
    tid &= 0x0000_0000_ffff_ffff;
    if tid == 0 {
        tid = NEXT_TID.fetch_add(1, Ordering::Relaxed) & 0x0000_0000_ffff_ffff;
    }
    tid
}

pub fn query_port_counters_extended(
    port: &mut IbMadPort,
    agent_id: u32,
    timeout_ms: u32,
    retries: u32,
    lid: u16,
    port_select: u8,
) -> Result<perf_mad, io::Error> {
    let mut perf_payload = perf_mad {
        pm_key: 0,
        reserved: [0; 32],
        data: [0; 192],
    };
    perf_payload.set_port_select(port_select);
    perf_payload.set_counter_select(0);
    perf_payload.set_counter_select2(0);

    let tid = next_tid();

    let mut ib_mad_payload = ib_mad {
        base_version: 0x1,
        mgmt_class: IB_MGMT_CLASS_PERFORMANCE,
        class_version: 0x1,
        method: 0x01,
        status: 0,
        hop_ptr: 0,
        hop_cnt: 0,
        tid: tid.to_be(),
        attr_id: 0x001d_u16.to_be(),
        additional_status: 0,
        attr_mod: 0,
        data: [0; 232],
    };

    let perf_bytes = perf_payload.to_bytes();
    ib_mad_payload.data[..perf_bytes.len()].copy_from_slice(&perf_bytes);

    let mut request = ib_user_mad {
        agent_id,
        status: 0,
        timeout_ms,
        retries,
        length: std::mem::size_of::<ib_mad>() as u32,
        addr: ib_mad_addr {
            qpn: (1u32).to_be(),
            qkey: IB_DEFAULT_QKEY.to_be(),
            lid: lid.to_be(),
            sl: 0,
            path_bits: 0,
            grh_present: 0,
            gid_index: 0,
            hop_limit: 0,
            traffic_class: 0,
            gid: [0; 16],
            flow_label: 0,
            pkey_index: 0,
            reserved: [0; 6],
        },
        data: [0; 256],
    };

    let ib_mad_bytes = ib_mad_payload.to_bytes();
    request.data[..ib_mad_bytes.len()].copy_from_slice(&ib_mad_bytes);

    send(port, &request)?;

    let mut response = ib_user_mad {
        agent_id,
        status: 0,
        timeout_ms: 0,
        retries: 0,
        length: 0,
        addr: ib_mad_addr {
            qpn: 0,
            qkey: 0,
            lid: 0,
            sl: 0,
            path_bits: 0,
            grh_present: 0,
            gid_index: 0,
            hop_limit: 0,
            traffic_class: 0,
            gid: [0; 16],
            flow_label: 0,
            pkey_index: 0,
            reserved: [0; 6],
        },
        data: [0; 256],
    };

    recv(port, &mut response, timeout_ms)?;

    let recv_mad = ib_mad::from_bytes(&response.data).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "Failed to parse response MAD")
    })?;

    let status = u16::from_be(recv_mad.status);
    if status != 0 {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Device returned MAD status {:#x}", status),
        ));
    }

    if recv_mad.attr_id != 0x001d_u16.to_be() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "Unexpected attribute ID: expected 0x001d, got 0x{:04x}",
                u16::from_be(recv_mad.attr_id)
            ),
        ));
    }

    perf_mad::from_bytes(&recv_mad.data).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Failed to parse PortCountersExtended payload",
        )
    })
}
pub fn register_agent(port: &mut IbMadPort, mgmt_class: u8) -> Result<u32, io::Error> {
    let mut req = ib_user_mad_reg_req {
        id: 0,
        method_mask: unsafe { MaybeUninit::<[u32; 4]>::zeroed().assume_init() },
        qpn: if mgmt_class == 0x1 || mgmt_class == 0x81 {
            0
        } else {
            1
        },
        mgmt_class,
        mgmt_class_version: 1,
        oui: unsafe { MaybeUninit::<[u8; 3]>::zeroed().assume_init() },
        rmpp_version: 0,
    };

    let req_ptr: *mut ib_user_mad_reg_req = &mut req;
    let fd = port.file.as_raw_fd();
    let r = unsafe { ib_user_mad_register_agent(fd, req_ptr) };
    match r {
        Ok(_rc) => {
            log::debug!("register_agent - registed agent, agent_id: {}", req.id);
            Ok(req.id)
        }
        Err(e) => {
            log::debug!("register_agent - Failed to register agent, errorno: {}", e);
            Err(std::io::Error::new(io::ErrorKind::Other, e))
        }
    }
}

pub fn send(port: &mut IbMadPort, umad: &ib_user_mad) -> io::Result<usize> {
    if port.file.as_raw_fd() < 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid file descriptor",
        ));
    }
    if umad.length as usize > umad.data.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "length exceeds buffer",
        ));
    }
    let bytes = umad.to_bytes();
    log::debug!("send - MAD bytes:\n{}", dump_bytes(&bytes));
    port.file.write(&bytes)
}

pub fn recv(port: &mut IbMadPort, umad: &mut ib_user_mad, timeout_ms: u32) -> io::Result<usize> {
    let fd = port.file.as_fd();

    if fd.as_raw_fd() < 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid file descriptor",
        ));
    }

    let poll_timeout =
        PollTimeout::try_from(timeout_ms).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    let mut poll_fd: [PollFd<'_>; 1] = [PollFd::new(fd, PollFlags::POLLIN)];

    let rc =
        poll(&mut poll_fd, poll_timeout).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    if rc == 0 {
        return Err(io::Error::new(io::ErrorKind::TimedOut, "read timeout"));
    }

    let mut buf = vec![0u8; std::mem::size_of::<ib_user_mad>()];

    let rc = port.file.read(&mut buf)?;

    log::debug!(
        "recv - MAD bytes: length ({}) \n{}",
        buf.len(),
        dump_bytes(&buf)
    );

    if rc != buf.len() {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            format!(
                "short read timeout, bytes read: {}, expected: {}",
                rc,
                buf.len()
            ),
        ));
    }
    if let Some(val) = ib_user_mad::from_bytes(&buf) {
        *umad = val;
    } else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "error converting to umad",
        ));
    }

    Ok(rc)
}

pub fn send_wfile(port: &mut std::fs::File, umad: &ib_user_mad) -> io::Result<usize> {
    if port.as_raw_fd() < 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid file descriptor",
        ));
    }
    if umad.length as usize > umad.data.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "length exceeds buffer",
        ));
    }
    let bytes = umad.to_bytes();
    log::debug!("send - MAD bytes:\n{}", dump_bytes(&bytes));
    port.write(&bytes)
}

pub fn recv_wfile(port: &mut std::fs::File, umad: &mut ib_user_mad) -> io::Result<usize> {
    let mut buf = vec![0u8; std::mem::size_of::<ib_user_mad>()];

    let rc = port.read(&mut buf)?;

    log::debug!(
        "recv - MAD bytes: length ({}) \n{}",
        buf.len(),
        dump_bytes(&buf)
    );

    if rc == 0 {
        // A read of 0 bytes is a valid EOF, handle as you see fit.
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "read 0 bytes, connection may be closed",
        ));
    }

    if rc != buf.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData, // More appropriate than TimedOut
            format!("short read, bytes read: {}, expected: {}", rc, buf.len()),
        ));
    }
    if let Some(val) = ib_user_mad::from_bytes(&buf) {
        *umad = val;
    } else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "error converting bytes to umad",
        ));
    }

    Ok(rc)
}
