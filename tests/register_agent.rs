#[test]
fn register_agent_invalid_fd() {
    use std::fs::File;
    use ibmad::mad::{IbMadPort, register_agent};

    // open /dev/null which does not support our ioctl
    let file = File::open("/dev/null").expect("/dev/null should exist");
    let mut port = IbMadPort { file };

    let res = register_agent(&mut port, ibmad::IB_PERFORMANCE_MGMT_CLASS);
    assert!(res.is_err(), "expected error registering agent on invalid fd");
}
