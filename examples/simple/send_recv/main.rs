// cargo add nix -F ioctl
// cargo add clap -F derive
use std::io::{Read, Write};
use std::mem::MaybeUninit;
use std::os::fd::AsRawFd;

use nix::{ioctl_none, ioctl_readwrite, ioctl_write_int};
use clap::Parser;

const PORT_POSITION: usize = 129;
const IB_QKEY: u32 = 0x80010000;
const IB_EN_PKEY: u8 = 3;
const IB_METHOD_GET: u8 = 1;

const IB_IOCTL_MAGIC: u8 = 0x1b as u8;
const IB_REG_AGENT: u64 = 1;
const IB_UNREG_AGENT: u64 = 2;

ioctl_readwrite!(ib_register_agent, IB_IOCTL_MAGIC, IB_REG_AGENT, ib_user_mad_reg_req);
ioctl_write_int!(ib_unregister_agent, IB_IOCTL_MAGIC, IB_UNREG_AGENT);
ioctl_none!(ib_enable_pkey, IB_IOCTL_MAGIC, IB_EN_PKEY);

#[derive(Parser)]
struct Args {
    #[arg(long)]
    hca: String, // TODO: lookup character device

    #[arg(long, default_value_t = 1000)]
    timeout: u32,

    #[arg(long, default_value_t = 0)]
    pkey: u16,

    #[arg(long)]
    lid: u16,

    #[arg(long)]
    port: u8,

    #[arg(long, default_value_t = false)]
    verbose: bool,
}


#[derive(Debug, Clone)]
#[repr(C)]
#[allow(non_camel_case_types)]
struct ib_user_mad_reg_req {
    id: u32,
    method_mask: [u32; 4],
    qpn: u8,
    mgmt_class: u8,
    mgmt_class_version: u8,
    oui: [u8; 3],
    rmpp_version: u8, 
}

#[derive(Debug, Copy, Clone)]
#[repr(C, packed)]
#[allow(non_camel_case_types)]
pub struct ib_mad {
	pub base_version: u8,
	pub mgmt_class: u8,
	pub class_version: u8,
	pub method: u8,
	pub status: u16,
	pub hop_ptr: u8,
	pub hop_cnt: u8,
	pub tid: u64,
	pub attr_id: u16,
	pub resv: u16,
	pub attr_mod: u32,
	pub data:  [u8; 232]
}

#[derive(Copy, Clone)]
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

#[derive(Debug)]
#[repr(C, packed)]
#[allow(non_camel_case_types)]
pub struct cable_info {
    pub reserved: [u8; 40],
    pub i2c_device_address: u8, //40h
    pub page_number: u8,
    pub device_address: u16,
    pub res1: u16,
    pub size: u16,
    pub res2: u64,
    pub data: [u32; 12]
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

impl ib_user_mad {
    pub fn new() -> ib_user_mad {
        ib_user_mad {
            agent_id: 0,
            status: 0,
            timeout_ms: 0,
            retries: 0,
            length: 0,
            addr: ib_mad_addr{
                qpn: 0,
                qkey: 0,
                lid: 0,
                sl: 0,
                path_bits: 0,
                grh_present: 0,
                gid_index: 0,
                hop_limit: 0,
                traffic_class: 0,
                gid: unsafe { std::mem::zeroed() },
                flow_label: 0,
                pkey_index: 0,
                reserved: unsafe { std::mem::zeroed()  },
            },
            data: unsafe { std::mem::zeroed() },
        }
    }

}

fn dump_bytes(buf: &[u8]) {
    print!("0x0000: ");
    let len = buf.len();
    for (i, &byte) in buf.iter().enumerate() {
        print!("{:02x} ", byte);
        if (i + 1) % 8 == 0{
            print!(" ");
        }
        if (i + 1) % 16 == 0 { 
            println!(); // Add a newline after every 8 bytes
            if i < len - 1 {
                print!("0x{:04x}: ", i);
            }
        }
    }
}

fn main() {

    let args = Args::parse();

    // Use mut because the kernel might write the agent ID back into `id`
    let mut req = ib_user_mad_reg_req {
        id: 0,
        method_mask: unsafe { MaybeUninit::<[u32; 4]>::zeroed().assume_init() },
        qpn: 0,
        mgmt_class: 1,
        mgmt_class_version: 1,
        oui: unsafe { MaybeUninit::<[u8; 3]>::zeroed().assume_init() },
        rmpp_version: 0,
    };

    // TODO lookup HCA umad character device
    match std::fs::File::options().read(true).write(true).open("/dev/infiniband/umad0") {
        Ok(mut file) => {
            let fd = file.as_raw_fd();

            let req_ptr: *mut ib_user_mad_reg_req = &mut req;

            println!(
                "Attempting ioctl on fd {} with request code {:#x} and data pointer {:p}",
                fd,
                IB_IOCTL_MAGIC,
                req_ptr
            );

            // Enable PKeys
            let r = unsafe {
                ib_enable_pkey(fd)
            };

            match r {
                Ok(i) => {
                    println!("PKey ioctl Successful");
                }
                Err(_) =>{}
            }

            // Register agent
            let r = unsafe { 
                ib_register_agent(fd, req_ptr)
            };
            

            match r {
                Ok(i) => {
                    println!("Register Agent ioctl Successful: {:?} {:?}", i, req);

                    let mut new_umad = ib_user_mad::new();

                    new_umad.addr.lid = args.lid.to_be();
                    new_umad.addr.pkey_index = args.pkey;
                    new_umad.addr.qpn = 0;
                    new_umad.addr.qkey = IB_QKEY.to_be();
                    new_umad.timeout_ms = args.timeout;
                    new_umad.agent_id = req.id;
                    new_umad.retries = 0;

                    let umad_ptr: *mut ib_mad = new_umad.data.as_mut_ptr() as *mut ib_mad;

                    unsafe {
                        (*umad_ptr).base_version = 1;
                        (*umad_ptr).mgmt_class = 1;
                        (*umad_ptr).class_version = 1;
                        (*umad_ptr).method = IB_METHOD_GET;
                        (*umad_ptr).tid = (0xff as u64).to_be();
                        (*umad_ptr).attr_id = (0xff60 as u16).to_be();
                        (*umad_ptr).attr_mod = (args.port as u32).to_be();
                    }

                    let mut cable_info = cable_info {
                        reserved:  unsafe { std::mem::MaybeUninit::<[u8 ;40]>::zeroed().assume_init() },
                        i2c_device_address: 0x50 as u8,
                        page_number: 0,
                        device_address: (128 as u16).to_be(),
                        res1: 0,
                        size: (48 as u16).to_be(),
                        res2: 0,
                        data:  unsafe { std::mem::MaybeUninit::<[u32; 12]>::zeroed().assume_init() },
                    };
                
                    let ci_ptr = &mut cable_info as *mut cable_info as *mut u8;

                    unsafe {
                        std::ptr::copy_nonoverlapping(ci_ptr, (*umad_ptr).data.as_mut_ptr(), std::mem::size_of::<[u8; 104 as usize] >());

                    };

                    let mad_bytes: &mut [u8] = unsafe {
                        std::slice::from_raw_parts_mut(
                            &new_umad as *const ib_user_mad as *mut u8,
                            std::mem::size_of::<ib_user_mad>(),
                        )
                    };


                    let port_byte_opt = mad_bytes.get_mut(PORT_POSITION);
                    if let Some(port_byte) = port_byte_opt {
                        *port_byte = args.port;
                    }

                    println!("Sending MAD");
                    dump_bytes(&mad_bytes);

                    let start_ts = std::time::Instant::now();
                    match file.write(mad_bytes) {
                        Ok(_) => {
                            //std::thread::sleep(std::time::Duration::new(0, 1000));
                            let mut buf: [u8; 320 as usize] = [0; 320];
                            let read_res = file.read(&mut buf);
                            let elapsed = start_ts.elapsed();
                            println!("Response received, time elapsed: {:?}", elapsed);

                            if let Ok(size) = read_res {
                                println!("Read {} bytes", size);
                                dump_bytes(&buf);
                            };
            

                        }
                        Err(e) => {
                            println!("Err: {:?}", e);
                        }
                    }

                    let _ = unsafe { ib_unregister_agent(fd, IB_UNREG_AGENT) };

                },
                Err(e) => {
                    println!("ioctl error: {:?}", e);
                }
            }

        }
        Err(e) => {
            eprintln!("Failed to open /dev/infiniband/umad0: {}", e);
        }
        
    }
}
