#[cfg(test)]
mod mad_io_tests {
    use std::ffi::CString;
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::os::unix::io::FromRawFd;

    use nix::sys::memfd::{memfd_create, MemFdCreateFlag};

    use ibmad::mad::{IbMadPort, ib_user_mad, ib_mad_addr};

    fn create_memfd_port() -> IbMadPort {
        let fd = memfd_create(CString::new("madtest").unwrap(), MemFdCreateFlag::empty()).unwrap();
        let file = unsafe { std::fs::File::from_raw_fd(fd) };
        IbMadPort { file }
    }

    fn sample_umad(agent_id: u32) -> ib_user_mad {
        use ibmad::mad::{dr_smp_mad, ib_mad};

        // build DR SMP MAD content
        let mut dr = dr_smp_mad {
            m_key: 0,
            drslid: 0xffff,
            drdlid: 0xffff,
            reserved: [0; 28],
            attr_layout: [0; 64],
            initial_path: [0; 64],
            return_path: [0; 64],
        };
        dr.initial_path[0] = 0;
        dr.initial_path[1] = 1;
        dr.initial_path[2] = 3;

        // embed DR SMP into MAD payload
        let mut mad = ib_mad {
            base_version: 0x1,
            mgmt_class: ibmad::mad::IB_MGMT_CLASS_DIRECT_ROUTED_SMP,
            class_version: 0x1,
            method: 0x1,
            status: 0,
            hop_ptr: 0,
            hop_cnt: 2,
            tid: 0x11,
            attr_id: 0x0010,
            additional_status: 0,
            attr_mod: 0,
            data: [0; 232],
        };

        let dr_bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(
                &dr as *const dr_smp_mad as *const u8,
                std::mem::size_of::<dr_smp_mad>(),
            )
        };
        mad.data[..dr_bytes.len()].copy_from_slice(dr_bytes);

        let mut umad = ib_user_mad {
            agent_id,
            status: 0,
            timeout_ms: 0,
            retries: 0,
            length: std::mem::size_of::<ib_mad>() as u32,
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

        let mad_bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(
                &mad as *const ib_mad as *const u8,
                std::mem::size_of::<ib_mad>(),
            )
        };
        umad.data[..mad_bytes.len()].copy_from_slice(mad_bytes);

        umad
    }

    #[test]
    fn send_writes_to_memfd() {
        let mut port = create_memfd_port();
        let umad = sample_umad(1);

        let res = ibmad::mad::send(&mut port, &umad).unwrap();
        assert_eq!(res, std::mem::size_of::<ib_user_mad>());

        port.file.seek(SeekFrom::Start(0)).unwrap();
        let mut buf = vec![0u8; std::mem::size_of::<ib_user_mad>()];
        port.file.read_exact(&mut buf).unwrap();
        let bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(
                &umad as *const ib_user_mad as *const u8,
                std::mem::size_of::<ib_user_mad>(),
            )
        };
        assert_eq!(&buf[..], bytes);
    }

    #[test]
    fn recv_reads_from_memfd() {
        let mut port = create_memfd_port();
        let umad = sample_umad(1);
        let bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(
                &umad as *const ib_user_mad as *const u8,
                std::mem::size_of::<ib_user_mad>(),
            )
        };
        port.file.write_all(bytes).unwrap();
        port.file.seek(SeekFrom::Start(0)).unwrap();

        let mut out: ib_user_mad = unsafe { std::mem::zeroed() };
        let res = ibmad::mad::recv(&mut port, &mut out).unwrap();
        assert_eq!(res, std::mem::size_of::<ib_user_mad>());

        let out_bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(
                &out as *const ib_user_mad as *const u8,
                std::mem::size_of::<ib_user_mad>(),
            )
        };
        assert_eq!(out_bytes, bytes);
    }
}

