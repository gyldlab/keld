//! Registers the native acceptance-observer cfg and admits only explicit macOS
//! dev-profile builds; Rust debug assertions separately gate operational support.

#[cfg(not(test))]
fn main() {
    println!("cargo:rustc-check-cfg=cfg(keld_native_fs_observer_eligible)");
    for name in [
        "CARGO_FEATURE_FS_ACCEPTANCE_OBSERVER",
        "CARGO_CFG_TARGET_OS",
        "PROFILE",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    if eligible(
        std::env::var_os("CARGO_FEATURE_FS_ACCEPTANCE_OBSERVER").is_some(),
        std::env::var("CARGO_CFG_TARGET_OS").ok().as_deref(),
        std::env::var("PROFILE").ok().as_deref(),
    ) {
        println!("cargo:rustc-cfg=keld_native_fs_observer_eligible");
    }
}

fn eligible(feature: bool, target: Option<&str>, profile: Option<&str>) -> bool {
    feature && target == Some("macos") && profile == Some("debug")
}

#[cfg(test)]
mod tests {
    use super::eligible;

    #[test]
    fn only_explicit_mac_debug_facts_admit_support() {
        assert!(eligible(true, Some("macos"), Some("debug")));
        for (feature, target, profile) in [
            (false, Some("macos"), Some("debug")),
            (true, Some("linux"), Some("debug")),
            (true, Some("windows"), Some("debug")),
            (true, Some("macos"), Some("release")),
            (true, Some("macos"), Some("custom")),
            (true, None, Some("debug")),
            (true, Some("macos"), None),
        ] {
            assert!(!eligible(feature, target, profile));
        }
    }
}
