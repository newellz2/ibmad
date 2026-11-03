//! Regression tests for response matching on a UMAD fd.
//!
//! A `UnixDatagram` pair stands in for the umad character device: like the
//! kernel, it preserves packet boundaries, so short packets (the kernel's
//! send-timeout notifications) arrive as distinct reads.

use std::{
    fs::File,
    io::{Read, Write},
    os::{fd::OwnedFd, unix::net::UnixDatagram},
    thread,
};

use ibmad::mad::{
    self, IB_MGMT_MAD_HDR, IbMadPort, UMAD_HDR_SIZE, ib_mad, ib_user_mad, perf_mad,
};

const UMAD_SIZE: usize = std::mem::size_of::<ib_user_mad>();

fn port_pair() -> (IbMadPort, File) {
    let (a, b) = UnixDatagram::pair().unwrap();
    (
        IbMadPort {
            file: File::from(OwnedFd::from(a)),
        },
        File::from(OwnedFd::from(b)),
    )
}

fn tid_of(umad: &ib_user_mad) -> u64 {
    umad.get_tid().unwrap() & 0xffff_ffff
}

/// A PortCountersExtended response for `tid` whose PortXmitData is `xmit`.
fn perf_response(tid: u64, xmit: u64) -> Vec<u8> {
    let mut perf = perf_mad {
        pm_key: 0,
        reserved: [0; 32],
        data: [0; 192],
    };
    perf.set_port_xmit_data(xmit);

    let mut mad_ = ib_mad {
        base_version: 1,
        mgmt_class: mad::IB_MGMT_CLASS_PERFORMANCE,
        class_version: 1,
        method: 0x81,
        status: 0,
        hop_ptr: 0,
        hop_cnt: 0,
        tid: tid.to_be(),
        attr_id: 0x001d_u16.to_be(),
        additional_status: 0,
        attr_mod: 0,
        data: [0; 232],
    };
    let pb = perf.to_bytes();
    mad_.data[..pb.len()].copy_from_slice(&pb);

    let mut buf = vec![0u8; UMAD_SIZE];
    let mb = mad_.to_bytes();
    buf[UMAD_HDR_SIZE..UMAD_HDR_SIZE + mb.len()].copy_from_slice(&mb);
    buf
}

/// What the kernel queues when it gives up on a send: UMAD header with
/// `status` set, followed by only the 24-byte MAD header.
fn timeout_notice(tid: u64) -> Vec<u8> {
    let mut buf = vec![0u8; UMAD_HDR_SIZE + IB_MGMT_MAD_HDR];
    buf[4..8].copy_from_slice(&110u32.to_ne_bytes()); // status = ETIMEDOUT
    buf[UMAD_HDR_SIZE + 8..UMAD_HDR_SIZE + 16].copy_from_slice(&tid.to_be_bytes());
    buf
}

/// Issue #1: a stale packet (late response or timeout notice for an earlier
/// query) queued ahead of the real answer must not be returned as this
/// query's counters.
#[test]
fn perf_query_skips_stale_packets_and_returns_matching_tid() {
    let (mut port, mut peer) = port_pair();

    let responder = thread::spawn(move || {
        let mut req = [0u8; UMAD_SIZE];
        let n = peer.read(&mut req).unwrap();
        assert_eq!(n, UMAD_SIZE);
        let req = ib_user_mad::from_bytes(&req).unwrap();
        let tid = tid_of(&req);

        // Leftovers from earlier queries, then the real answer.
        peer.write_all(&timeout_notice(tid.wrapping_add(7))).unwrap();
        peer.write_all(&perf_response(tid.wrapping_add(3), 0xBAD)).unwrap();
        peer.write_all(&perf_response(tid, 0x600D)).unwrap();
    });

    let resp = mad::query_port_counters_extended(&mut port, 1, 200, 0, 42, 1, 0, 1)
        .expect("matching response should be returned");
    responder.join().unwrap();

    assert_eq!(resp.port_xmit_data(), 0x600D);
}

/// The kernel's timeout notice for *our* TID ends the query promptly as a
/// timeout, rather than being parsed as (all-zero) counters.
#[test]
fn perf_query_reports_timeout_on_kernel_notice_for_own_tid() {
    let (mut port, mut peer) = port_pair();

    let responder = thread::spawn(move || {
        let mut req = [0u8; UMAD_SIZE];
        peer.read(&mut req).unwrap();
        let tid = tid_of(&ib_user_mad::from_bytes(&req).unwrap());
        peer.write_all(&timeout_notice(tid)).unwrap();
    });

    let err = mad::query_port_counters_extended(&mut port, 1, 200, 0, 42, 1, 0, 0)
        .expect_err("kernel timeout notice must surface as an error");
    responder.join().unwrap();

    assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
}

/// Issue #3: short notices are readable (TID + status) via `recv_packet`,
/// while plain `recv` keeps its old full-size-only contract.
#[test]
fn recv_packet_accepts_short_timeout_notice() {
    let (mut port, mut peer) = port_pair();
    peer.write_all(&timeout_notice(0x1234)).unwrap();
    peer.write_all(&timeout_notice(0x5678)).unwrap();

    let mut umad = ib_user_mad::from_bytes(&[0u8; UMAD_SIZE]).unwrap();
    let n = mad::recv_packet(&mut port, &mut umad, 100).unwrap();
    assert_eq!(n, UMAD_HDR_SIZE + IB_MGMT_MAD_HDR);
    let status = umad.status;
    assert_eq!(status, 110);
    assert_eq!(tid_of(&umad), 0x1234);

    let err = mad::recv(&mut port, &mut umad, 100).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
}
