#[cfg(test)]
mod mad_io_tests {
    use std::{fs, io::{Read, Seek, SeekFrom, Write}};

    use nix::sys::memfd::{memfd_create, MFdFlags};

    use ibmad::mad::{ib_mad_addr, ib_user_mad, IbMadPort, IB_DEFAULT_QKEY};

    fn create_memfd_port() -> IbMadPort {
        let owned_fd = memfd_create("mad_test", MFdFlags::empty()).unwrap();
        let file = fs::File::from(owned_fd);
        IbMadPort { file }
    }

    fn sample_umad_attr(agent_id: u32, attr_id: u16) -> ib_user_mad {
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
            tid: (0x1337 as u64).to_be(),
            attr_id: (attr_id as u16).to_be(),
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

    fn sample_umad(agent_id: u32) -> ib_user_mad {
        sample_umad_attr(agent_id, 0x0010)
    }

    fn write_direction(port: &mut IbMadPort, direction: u8) {
        let method_offset = std::mem::size_of::<u32>() * 5
            + std::mem::size_of::<ib_mad_addr>()
            + 2;
        
        port.file
            .seek(SeekFrom::Start(method_offset as u64))
            .unwrap();

        let mut buf: [u8; 1] = [0x0];
        let _ = port.file.read_exact(&mut buf);

        buf[0] = buf[0] | direction;

        port.file.write_all(&buf).unwrap();
    }

    fn write_status(port: &mut IbMadPort, status: u16) {
        let status_offset = std::mem::size_of::<u32>() * 5
            + std::mem::size_of::<ib_mad_addr>()
            + 4;
        
        port.file
            .seek(SeekFrom::Start(status_offset as u64))
            .unwrap();

        port.file.write_all(&status.to_be_bytes()).unwrap();
    }

    fn update_tid(port: &mut IbMadPort, mask: u64) {
        let tid_offset = std::mem::size_of::<u32>() * 5
            + std::mem::size_of::<ib_mad_addr>()
            + 8;
        
        port.file
            .seek(SeekFrom::Start(tid_offset as u64))
            .unwrap();

        let mut tid_bytes: [u8; 8] = [0; 8];
        port.file.read_exact(&mut tid_bytes).unwrap();
        let tid = u64::from_be_bytes(tid_bytes) | mask;

        port.file
            .seek(SeekFrom::Start(tid_offset as u64))
            .unwrap();
        port.file.write_all(&tid.to_be_bytes()).unwrap();
    }

    fn write_node_desc(port: &mut IbMadPort, desc: &[u8]) {
        let attr_offset = std::mem::size_of::<u32>() * 5
            + std::mem::size_of::<ib_mad_addr>()
            + (std::mem::size_of::<ibmad::mad::ib_mad>() - std::mem::size_of::<[u8; 232]>())
            + 40;
        port.file
            .seek(SeekFrom::Start(attr_offset as u64))
            .unwrap();
        port.file.write_all(desc).unwrap();
    }

    #[repr(C, packed)]
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct NodeInfo {
        base_version: u8,
        class_version: u8,
        node_type: u8,
        nports: u8,
        system_guid: u64,
        node_guid: u64,
        port_guid: u64,
        partition_cap: u16,
        device_id: u16,
        revision: u32,
        local_port: u8,
        vendor_id: [u8; 3],
        reserved: [u8; 24],
    }

    fn write_node_info(port: &mut IbMadPort, info: &NodeInfo) {
        let attr_offset = std::mem::size_of::<u32>() * 5
            + std::mem::size_of::<ib_mad_addr>()
            + (std::mem::size_of::<ibmad::mad::ib_mad>() - std::mem::size_of::<[u8; 232]>())
            + 40;

        port.file
            .seek(SeekFrom::Start(attr_offset as u64))
            .unwrap();
        let bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(
                info as *const NodeInfo as *const u8,
                std::mem::size_of::<NodeInfo>(),
            )
        };
        port.file.write_all(bytes).unwrap();
    }

    #[test]
    fn send_writes_to_memfd_success() {

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

    #[test]
    fn recv_reads_modified_mad_success() {

        let _ = env_logger::try_init();

        let mut port = create_memfd_port();
        let umad = sample_umad(2);

        // write initial MAD bytes to the memfd
        let bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(
                &umad as *const ib_user_mad as *const u8,
                std::mem::size_of::<ib_user_mad>(),
            )
        };
        port.file.write_all(bytes).unwrap();

        // Modify the status, TID and NodeDesc
        write_status(&mut port, 0x04);
        update_tid(&mut port, 0xfefe_fefe_0000_0000);

        // NodeDesc == 'switch'
        const ATTR_BYTES: [u8; 6] = [0x73, 0x77, 0x69, 0x74, 0x63, 0x68];
        write_node_desc(&mut port, &ATTR_BYTES);

        // rewind for reading
        port.file.seek(SeekFrom::Start(0)).unwrap();

        let mut recv_umad = sample_umad(0);
        let res = ibmad::mad::recv(&mut port, &mut recv_umad).unwrap();
        assert_eq!(res, std::mem::size_of::<ib_user_mad>());

        let dr: &ibmad::mad::dr_smp_mad = unsafe {
            &*(recv_umad.data[24..].as_ptr() as *const ibmad::mad::dr_smp_mad)
        };

        let node_desc_bytes = &dr.attr_layout[..ATTR_BYTES.len()];
        let node_desc = String::from_utf8_lossy(node_desc_bytes);

        log::debug!("recv_reads_modified_mad - Read NodeDesc: '{}'", node_desc);
        assert_eq!(&dr.attr_layout[..ATTR_BYTES.len()], &ATTR_BYTES);
    }

    #[test]
    fn send_recv_nodedesc_success() {

        let _ = env_logger::try_init();

        let mut port = create_memfd_port();
        let send_mad = sample_umad(0);

        let bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(
                &send_mad as *const ib_user_mad as *const u8,
                std::mem::size_of::<ib_user_mad>(),
            )
        };

        log::debug!("tests - send_recv_nodedesc_success - SendMAD:\n{}", ibmad::dump_bytes(bytes));

        // send
        let res = ibmad::mad::send(&mut port, &send_mad).unwrap();
        assert_eq!(res, std::mem::size_of::<ib_user_mad>());

        // rewind
        port.file.seek(SeekFrom::Start(0)).unwrap();

        write_direction(&mut port, 0x80);
        write_status(&mut port, 0x04);
        update_tid(&mut port, 0xdead_beef_0000_0000);

        // NodeDesc == 'switch-test'
        const ATTR_BYTES: [u8; 11] = [0x73, 0x77, 0x69, 0x74, 0x63, 0x68, 0x2d, 0x74, 0x65, 0x73, 0x74];
        write_node_desc(&mut port, &ATTR_BYTES);

        port.file.seek(SeekFrom::Start(0)).unwrap();

        // recv
        let mut recv_umad = sample_umad(0);
        let res = ibmad::mad::recv(&mut port, &mut recv_umad).unwrap();

        assert_eq!(res, std::mem::size_of::<ib_user_mad>());

        let bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(
                &recv_umad as *const ib_user_mad as *const u8,
                std::mem::size_of::<ib_user_mad>(),
            )
        };

        log::debug!("tests - send_recv_nodedesc_success - RecvMAD:\n{}", ibmad::dump_bytes(bytes));

        let dr: &ibmad::mad::dr_smp_mad = unsafe {
            &*(recv_umad.data[24..].as_ptr() as *const ibmad::mad::dr_smp_mad)
        };

        let node_desc_bytes = &dr.attr_layout[..ATTR_BYTES.len()];
        let node_desc = String::from_utf8_lossy(node_desc_bytes);

        log::debug!("recv_reads_modified_mad - Read NodeDesc: '{}'", node_desc);

    }

    #[test]
    fn send_recv_nodeinfo_success() {

        let _ = env_logger::try_init();

        let mut port = create_memfd_port();
        let send_mad = sample_umad_attr(0, 0x0011);

        // send request
        let res = ibmad::mad::send(&mut port, &send_mad).unwrap();
        assert_eq!(res, std::mem::size_of::<ib_user_mad>());

        // prepare response
        port.file.seek(SeekFrom::Start(0)).unwrap();
        write_direction(&mut port, 0x80);
        write_status(&mut port, 0x04);
        update_tid(&mut port, 0xfeed_beef_0000_0000);

        let node_info = NodeInfo {
            base_version: 1,
            class_version: 1,
            node_type: 2,
            nports: 1,
            system_guid: 0x0102_0304_0506_0708,
            node_guid: 0x1111_2222_3333_4444,
            port_guid: 0x5555_6666_7777_8888,
            partition_cap: 0x12,
            device_id: 0x3456,
            revision: 0xdead_beef,
            local_port: 1,
            vendor_id: [0xaa, 0xbb, 0xcc],
            reserved: [0; 24],
        };
        write_node_info(&mut port, &node_info);

        port.file.seek(SeekFrom::Start(0)).unwrap();

        let mut recv_umad = sample_umad_attr(0, 0x0011);
        let res = ibmad::mad::recv(&mut port, &mut recv_umad).unwrap();
        assert_eq!(res, std::mem::size_of::<ib_user_mad>());

        let dr: &ibmad::mad::dr_smp_mad = unsafe {
            &*(recv_umad.data[24..].as_ptr() as *const ibmad::mad::dr_smp_mad)
        };
        let recv_info: &NodeInfo = unsafe { &*(dr.attr_layout.as_ptr() as *const NodeInfo) };

        assert_eq!(recv_info, &node_info);
    }

}

