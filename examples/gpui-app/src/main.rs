#[cfg(not(target_env = "ohos"))]
fn main() {
    monio_gpui::run_desktop();
}

#[cfg(target_env = "ohos")]
fn main() {}
