fn main() {
    println!("cargo::rerun-if-env-changed=CARGO_CFG_TARGET_ENV");

    #[cfg(target_env = "ohos")]
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("ohos") {
        napi_build_ohos::setup();
        println!("cargo:rustc-link-lib=dylib=c++_shared");
    }
}
