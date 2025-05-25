#[cfg(test)]
mod ioctl_tests {
    use std::mem::MaybeUninit;


    #[test]
    fn ib_enable_pkey_success() {
        
        match std::fs::File::options().read(true).write(true).open("/dev/infiniband/umad0") {
            Ok(mut file) => {
                let fd = std::os::fd::AsRawFd::as_raw_fd(&file);

                // Enable PKeys
                let r = unsafe {
                    ibmad::ib_enable_pkey(fd)
                };

                match r {
                    Ok(i) => {
                        assert!(i > -1, "PKey enabled")
                    }
                    Err(_) =>{
                        assert!(false, "Failed to enable Pkeys")
                    }
                }
            }
            Err(_) => {
                //Failed
            }

        }
    }

    #[test]
    fn ib_user_mad_reg_req_success() {
        
        let mut req = ibmad::ib_user_mad_reg_req {
            id: 0,
            method_mask: unsafe { MaybeUninit::<[u32; 4]>::zeroed().assume_init() },
            qpn: 0,
            mgmt_class: 1,
            mgmt_class_version: 1,
            oui: unsafe { MaybeUninit::<[u8; 3]>::zeroed().assume_init() },
            rmpp_version: 0,
        };

        match std::fs::File::options().read(true).write(true).open("/dev/infiniband/umad0") {
            Ok(mut file) => {
                let fd = std::os::fd::AsRawFd::as_raw_fd(&file);

                let req_ptr: *mut ibmad::ib_user_mad_reg_req = &mut req;

                // Enable PKeys
                let r = unsafe {
                    ibmad::ib_enable_pkey(fd)
                };

                match r {
                    Ok(i) => {
                        assert!(i > -1, "PKey enabled")
                    }
                    Err(_) =>{
                        assert!(false, "Failed to enable Pkeys")
                    }
                }

                // Register agent
                let r = unsafe { 
                    ibmad::ib_register_agent(fd, req_ptr)
                };

                match r {
                    Ok(i) => {
                        assert!(i > -1, "Agent registered")
                    }
                    Err(_) =>{
                        assert!(false, "Failed to register agent")
                    }
                }
            }
            Err(_) => {
                //Failed
            }

        }
    }

    #[test]
    fn ib_user_mad_reg_req2_success() {
        
        let mut req = ibmad::ib_user_mad_reg_req2 {
            id: 0,
            qpn: 0,
            mgmt_class: 1,
            mgmt_class_version: 1,
            res: 0,
            flags: 0,
            method_mask: unsafe { MaybeUninit::<[u64; 2]>::zeroed().assume_init() },
            oui: 0,
            rmpp_version: 0,
            reserved: unsafe { MaybeUninit::<[u8; 3]>::zeroed().assume_init() },
        };

        match std::fs::File::options().read(true).write(true).open("/dev/infiniband/umad0") {
            Ok(mut file) => {
                let fd = std::os::fd::AsRawFd::as_raw_fd(&file);

                let req_ptr: *mut ibmad::ib_user_mad_reg_req2 = &mut req;

                // Enable PKeys
                let r = unsafe {
                    ibmad::ib_enable_pkey(fd)
                };

                match r {
                    Ok(i) => {
                        assert!(i > -1, "PKey enabled")
                    }
                    Err(_) =>{
                        assert!(false, "Failed to enable Pkeys")
                    }
                }

                // Register agent
                let r = unsafe { 
                    ibmad::ib_register_agent2(fd, req_ptr)
                };

                match r {
                    Ok(i) => {
                        assert!(i > -1, "Agent registered")
                    }
                    Err(_) =>{
                        assert!(false, "Failed to register agent")
                    }
                }
            }
            Err(_) => {
                //Failed
            }
        }
    }
}
