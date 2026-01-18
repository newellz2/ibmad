use std::path::Path;

pub fn setup() {
    let _ = env_logger::builder().is_test(true).try_init();
}

#[allow(dead_code)]
pub fn can_run_ib_tests() -> bool {
    Path::new("/sys/class/infiniband").exists()
}

#[allow(dead_code)]
pub fn can_run_umad_tests() -> bool {
    Path::new("/dev/infiniband/umad0").exists()
}
