#[cfg(test)]
mod mad_io_tests {
    use std::{fs, io::{Read, Seek, SeekFrom}};

    use nix::sys::memfd::{memfd_create, MFdFlags};

    use ibmad::mad::{ib_mad_addr, ib_user_mad, IbMadPort, IB_DEFAULT_QKEY};

    fn create_memfd_port() -> IbMadPort {
        let owned_fd = memfd_create("mad_test", MFdFlags::empty()).unwrap();
        let file = fs::File::from(owned_fd);
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
            mgmt_class: ibmad::mad::IB_MGMT_CLASS_DIRECT_ROUTED_SMP.to_be(),
            class_version: 0x1,
            method: 0x1,
            status: 0,
            hop_ptr: 0,
            hop_cnt: 1,
            tid: (0x11 as u64).to_be(),
            attr_id: (0x0010 as u16).to_be(),
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
            timeout_ms: 50,
            retries: 1,
            length: std::mem::size_of::<ib_mad>() as u32,
            addr: ib_mad_addr {
                qpn: 0,
                qkey: IB_DEFAULT_QKEY.to_be(),
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

        let _ = env_logger::try_init();

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

        log::debug!("tests - send_writes_to_memfd -  Read MAD:\n{}", ibmad::dump_bytes(bytes));

        assert_eq!(&buf[..], bytes);

    }

}

