use std::fs;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::{io, mem::MaybeUninit};
use crate::{dump_bytes, ib_user_mad_register_agent};
use crate::{cas::IbCa, ib_user_mad_enable_pkey, ib_user_mad_reg_req};


pub const IB_MGMT_CLASS_PERFORMANCE: u8 = 0x4;
pub const IB_MGMT_CLASS_LID_ROUTED_SMP: u8 = 0x1;
pub const IB_MGMT_CLASS_DIRECT_ROUTED_SMP: u8 = 0x81;


#[derive(Debug, Copy, Clone)]
#[repr(C, packed)]
#[allow(non_camel_case_types)]
pub struct ib_mad {
	pub base_version: u8,
	pub mgmt_class: u8,
	pub class_version: u8,
	pub method: u8, // & 0x80 = Response Bit
	pub status: u16,
	pub hop_ptr: u8,
	pub hop_cnt: u8,
	pub tid: u64,
	pub attr_id: u16,
	pub additional_status: u16,
	pub attr_mod: u32,
	pub data:  [u8; 232]
}

#[derive(Clone)]
#[repr(C)]
#[allow(non_camel_case_types)]
pub struct ib_user_mad {
    pub agent_id: u32,
    pub status: u32,
    pub timeout_ms: u32,
    pub retries: u32,
    pub length: u32,
    pub addr: ib_mad_addr,
    pub data: [u8; 256],
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
#[allow(non_camel_case_types)]
pub struct ib_mad_addr {
    pub qpn: u32,
    pub qkey: u32,
    pub lid: u16,
    pub sl: u8,
    pub path_bits: u8,
    pub grh_present: u8,
    pub gid_index: u8,
    pub hop_limit: u8,
    pub traffic_class: u8,
    pub gid: [u8; 16],
    pub flow_label: u32,
    pub pkey_index: u16,
    pub reserved: [u8; 6],
}

#[derive(Copy, Clone)]
#[repr(C)]
#[allow(non_camel_case_types)]
pub struct dr_smp_mad {
    pub m_key: u64,
    pub drslid: u16,
    pub drdlid: u16,
    pub reserved: [u8; 28],
    pub attr_layout: [u8; 64],
    pub initial_path: [u8; 64],
    pub return_path: [u8; 64],
}


#[derive(Debug)]
pub struct  IbMadPort {
    pub file: fs::File,
}

pub fn open_port(hca: &IbCa) -> Result<IbMadPort, io::Error> {

    if let Some(dev_paths) = &hca.dev_paths {

        match &dev_paths.umad_dev_path {
            Some(path) => {
                match fs::File::options().read(true).write(true).open(path) {
                    Ok(file) => {
                        let mad_port = IbMadPort{
                            file: file,
                        };

                        let fd = mad_port.file.as_raw_fd();

                        // Enable PKeys
                        let r = unsafe {
                            ib_user_mad_enable_pkey(fd)
                        };

                        match r {
                            Ok(rc) =>{
                                log::debug!("open_port - Successfully enabled PKeys,  rc: {}",  rc);
                                return Ok(mad_port);
                            }
                            Err(e)=>{
                                log::debug!("open_port - Error enabling PKeys : {}",  e);
                                let err = std::io::Error::new(io::ErrorKind::Other, e);
                                return Err(err)
                            }
                        }

                    },
                    Err(e) => {
                        log::debug!("open_port - Error opening character device: {}",  e);
                        let err = std::io::Error::new(io::ErrorKind::Other, e);
                        return Err(err)
                    }
                }
            },
            None => {
                log::debug!("open_port - HCA has no UMAD character device");
                let err = std::io::Error::new(
                    io::ErrorKind::NotFound,
                    io::Error::other("HCA has no UMAD character device".to_string())
                );
                return Err(err)
            }
        }
    } else {
        log::debug!("open_port - HCA has no character devices");
        let err = std::io::Error::new(
            io::ErrorKind::NotFound,
            io::Error::other("HCA has no character devices".to_string())
        );
        return Err(err)
    }
    
}

pub fn register_agent(port: &mut IbMadPort, mgmt_class: u8) -> Result<u32, io::Error> {
    let mut req = ib_user_mad_reg_req {
        id: 0,
        method_mask: unsafe { MaybeUninit::<[u32; 4]>::zeroed().assume_init() },
        qpn: if mgmt_class == 0x1 || mgmt_class == 0x81 { 0 } else { 1 },
        mgmt_class: mgmt_class,
        mgmt_class_version: 1,
        oui: unsafe { MaybeUninit::<[u8; 3]>::zeroed().assume_init() },
        rmpp_version: 0,
    };

    let req_ptr: *mut ib_user_mad_reg_req = &mut req;

    let fd = port.file.as_raw_fd();

    // Register agent IOCTL call
    let r = unsafe { 
        ib_user_mad_register_agent(fd, req_ptr)
    };


    match r {
        Ok(_rc) => {
            log::debug!("register_agent - registed agent, agent_id: {}", req.id);
            Ok(req.id)
        }
        Err(e)=>{
            log::debug!("register_agent - Failed to register agent, errorno: {}", e);
            Err(std::io::Error::new(io::ErrorKind::Other, e))
        }
    }
}


pub fn send(port: &mut IbMadPort) {

    let mut mad = ib_mad{
        base_version: 0x1,
        mgmt_class: (0x81 as u8).to_be(),
        class_version: 0x1,
        method: (0x1 as u8).to_be(),
        status: 0x0,
        hop_ptr: 0x00,
        hop_cnt: (0x02 as u8).to_be(),
        tid: (0x11 as u64).to_be(),
        attr_id: (0x0010 as u16).to_be(),
        additional_status: 0x0000,
        attr_mod: 0x0000_0000,
        data: [0; 232],
    };

    let mut dr_smp_mad = dr_smp_mad {
        m_key: 0x0,
        drslid: 0xffff,
        drdlid: 0xffff,
        reserved: [0; 28],
        attr_layout: [0; 64],
        initial_path: [0; 64],
        return_path: [0; 64],
    };

    let mut umad = ib_user_mad{
        agent_id: 0,
        status: 0,
        timeout_ms: 100,
        retries: 1,
        length: 0,
        addr: ib_mad_addr { 
            qpn: 0, 
            qkey: (0x80010000 as u32).to_be(), 
            lid: 0xffff, 
            sl: 0, 
            path_bits: 0, 
            grh_present: 0, 
            gid_index: 0, 
            hop_limit: 64, 
            traffic_class: 0, 
            gid: [0; 16], 
            flow_label: 0, 
            pkey_index: 0, 
            reserved: [0; 6]
        },
        data: [0; 256],
    };

    dr_smp_mad.initial_path[0] = 0;
    dr_smp_mad.initial_path[1] = 1;
    dr_smp_mad.initial_path[2] = 3;

    //dr_mad.initial_path.reverse();

    let dr_smp_ptr = &mut dr_smp_mad as *mut dr_smp_mad as *mut u8;

    unsafe {
        std::ptr::copy_nonoverlapping(dr_smp_ptr, mad.data.as_mut_ptr(), std::mem::size_of::<[u8; 232 as usize] >());
    };

    let mad_ptr = &mut mad as *mut ib_mad as *mut u8;

    unsafe {
        std::ptr::copy_nonoverlapping(mad_ptr, umad.data.as_mut_ptr(), std::mem::size_of::<[u8; 256 as usize] >());
    };

    let mut mad_bytes: &mut [u8] = unsafe {
        std::slice::from_raw_parts_mut(
            &umad as *const ib_user_mad as *mut u8,
            std::mem::size_of::<ib_user_mad>(),
        )
    };

    dump_bytes(mad_bytes);

    let r  = port.file.write(&mad_bytes);
    match r {
        Ok(rc) => {
            log::debug!("send - write rc: {}", rc);

        },
        Err(e) => {
            log::debug!("send - write error: {:?}", e);

        }
    }

    println!("");
    let r  = port.file.read(&mut mad_bytes);
    match r {
        Ok(rc) => {
            log::debug!("send - read rc: {}", rc);

        },
        Err(e) => {
            log::debug!("send - read error: {:?}", e);

        }
    }
    dump_bytes(mad_bytes);


}