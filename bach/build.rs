fn main() {
    let ac = autocfg::new();
    ac.emit_path_cfg("core::task::Waker::data", "feature_waker_data");
}
