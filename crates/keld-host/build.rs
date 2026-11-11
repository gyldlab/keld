//! The actual Windows Host and process-main tests use Common Controls v6.
//! Merge only its assembly dependency into the linker's default manifest.

fn main() {
    println!("cargo:rerun-if-changed=windows.manifest");
    if std::env::var("CARGO_CFG_TARGET_OS").ok().as_deref() != Some("windows")
        || std::env::var("CARGO_CFG_TARGET_ENV").ok().as_deref() != Some("msvc")
    {
        return;
    }
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/windows.manifest");
    for argument in [
        "/MANIFEST:EMBED".to_owned(),
        format!("/MANIFESTINPUT:{manifest}"),
    ] {
        println!("cargo:rustc-link-arg-bin=keld-host={argument}");
        println!("cargo:rustc-link-arg-tests={argument}");
    }
}
