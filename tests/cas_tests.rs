#[cfg(test)]
mod ioctl_tests {
    use std::{fmt::format, mem::MaybeUninit};


    #[test]
    fn ib_enable_pkey_success() {
        
        match std::fs::File::options().read(true).write(true).open("/dev/infiniband/umad0") {
            Ok(file) => {
                let fd = std::os::fd::AsRawFd::as_raw_fd(&file);

                // Enable PKeys
                let r = unsafe {
                    ibmad::ib_user_mad_enable_pkey(fd)
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
    fn get_cas_names_success() {
        match  ibmad::get_cas_names(){
            Ok(cas) =>{
                assert!( cas.len() > 0, "No CAs found.");
                for ca in cas.iter(){
                    println!("{}", ca);
                }
            }
            Err(e) => {
                assert!(false, "{}", format!("Error finding CAs: {:?}", e));
            }
        }
    }


}
