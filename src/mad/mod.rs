use std::fs;
use std::io::{Read, Write};
use std::os::fd::{AsFd, AsRawFd};
use std::{io, mem::MaybeUninit};

use crate::{dump_bytes, ib_user_mad_register_agent};
use crate::{ca::IbCa, ib_user_mad_enable_pkey, ib_user_mad_reg_req};

pub mod helpers;
pub mod types;
pub mod dr_smp;
pub mod node;
pub mod port;

pub use types::{ib_mad, ib_mad_addr, ib_user_mad};
pub use dr_smp::dr_smp_mad;
pub use node::node_info;
pub use port::port_info;

use nix::poll::{poll, PollFd, PollFlags, PollTimeout};

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

pub fn open_port(hca: &IbCa) -> Result<IbMadPort, io::Error> {
    if let Some(dev_paths) = &hca.dev_paths {
        match &dev_paths.umad_dev_path {
            Some(path) => {
                match fs::File::options().read(true).write(true).open(path) {
                    Ok(file) => {
                        let mad_port = IbMadPort { file };
                        let fd = mad_port.file.as_raw_fd();
                        let r = unsafe { ib_user_mad_enable_pkey(fd) };
                        match r {
                            Ok(rc) => {
                                log::debug!("open_port - Successfully enabled PKeys,  rc: {}", rc);
                                Ok(mad_port)
                            }
                            Err(e) => {
                                log::debug!("open_port - Error enabling PKeys :{}", e);
                                Err(std::io::Error::new(io::ErrorKind::Other, e))
                            }
                        }
                    }
                    Err(e) => {
                        log::debug!("open_port - Error opening character device: {}", e);
                        Err(std::io::Error::new(io::ErrorKind::Other, e))
                    }
                }
            }
            None => {
                log::debug!("open_port - HCA has no UMAD character device");
                Err(std::io::Error::new(
                    io::ErrorKind::NotFound,
                    io::Error::other("HCA has no UMAD character device".to_string()),
                ))
            }
        }
    } else {
        log::debug!("open_port - HCA has no character devices");
        Err(std::io::Error::new(
            io::ErrorKind::NotFound,
            io::Error::other("HCA has no character devices".to_string()),
        ))
    }
}

pub fn register_agent(port: &mut IbMadPort, mgmt_class: u8) -> Result<u32, io::Error> {
    let mut req = ib_user_mad_reg_req {
        id: 0,
        method_mask: unsafe { MaybeUninit::<[u32; 4]>::zeroed().assume_init() },
        qpn: if mgmt_class == 0x1 || mgmt_class == 0x81 { 0 } else { 1 },
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
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid file descriptor"));
    }
    if umad.length as usize > umad.data.len() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "length exceeds buffer"));
    }
    let bytes = umad.to_bytes();
    log::debug!("send - MAD bytes:\n{}", dump_bytes(&bytes));
    port.file.write(&bytes)
}

pub fn recv(port: &mut IbMadPort, umad: &mut ib_user_mad, timeout_ms: u32) -> io::Result<usize> {

    let fd = port.file.as_fd();

    if fd.as_raw_fd() < 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid file descriptor"));
    }

    let poll_timeout = PollTimeout::try_from(timeout_ms).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    let mut poll_fd: [PollFd<'_>; 1] = [PollFd::new(fd, PollFlags::POLLIN)];

    let rc = poll(&mut poll_fd, poll_timeout).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    if rc == 0 {
        return Err(
            io::Error::new(io::ErrorKind::TimedOut, "read timeout")
        );
    }

    let mut buf = vec![0u8; std::mem::size_of::<ib_user_mad>()];

    let rc = port.file.read(&mut buf)?;

    log::debug!("recv - MAD bytes: length ({}) \n{}", buf.len(), dump_bytes(&buf));

    if rc != buf.len() {
        return Err(io::Error::new(io::ErrorKind::TimedOut, format!("short read timeout, bytes read: {}, expected: {}", rc, buf.len())));
    }
    if let Some(val) = ib_user_mad::from_bytes(&buf) {
        *umad = val;
    } else {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "error converting to umad"));
    }

    Ok(rc)
}


pub fn send_wfile(port: &mut std::fs::File, umad: &ib_user_mad) -> io::Result<usize> {
    if port.as_raw_fd() < 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid file descriptor"));
    }
    if umad.length as usize > umad.data.len() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "length exceeds buffer"));
    }
    let bytes = umad.to_bytes();
    log::debug!("send - MAD bytes:\n{}", dump_bytes(&bytes));
    port.write(&bytes)
}


pub fn recv_wfile(port: &mut std::fs::File, umad: &mut ib_user_mad) -> io::Result<usize> {

    let mut buf = vec![0u8; std::mem::size_of::<ib_user_mad>()];

    let rc = port.read(&mut buf)?;

    log::debug!("recv - MAD bytes: length ({}) \n{}", buf.len(), dump_bytes(&buf));

    if rc == 0 {
        // A read of 0 bytes is a valid EOF, handle as you see fit.
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "read 0 bytes, connection may be closed"));
    }

    if rc != buf.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData, // More appropriate than TimedOut
            format!("short read, bytes read: {}, expected: {}", rc, buf.len())
        ));
    }
    if let Some(val) = ib_user_mad::from_bytes(&buf) {
        *umad = val;
    } else {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "error converting bytes to umad"));
    }

    Ok(rc)
}
