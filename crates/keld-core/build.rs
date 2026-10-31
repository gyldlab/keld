//! Registers the private acceptance-target build identity; never activates it.
fn main() {
    println!("cargo:rustc-check-cfg=cfg(keld_core_native_renderer_main)");
}
