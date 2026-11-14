//! Byte receipts were derived from the exact pre-extraction template and rendering rule.
use super::*;
use sha2::{Digest, Sha256};

#[test]
fn shared_renderer_preserves_pre_extraction_bytes() {
    assert_eq!(
        ORIGINAL_RUNTIME_ENTRY_PATH,
        "keld-installed-runtime-entry.mjs"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(BOOT_ENTRY_TEMPLATE.as_bytes())),
        "22f3cd586c17ecef697c09fe37e4e85fff12d8133e10f2dd548c9793e93cc429",
        "the existing canonical template was not replaced"
    );
    for (relative, expected) in [
        (
            "main.js",
            "133c8cf538c492b4162f7e109bf2ddef3538b3814322558e00b8cf26c22e277e",
        ),
        (
            "src/main.ts",
            "2cb569e86a3b1e3678d41ccace60bfdd592840b84788e843e1532d9681f0da5f",
        ),
        (
            "src/deep/main.mjs",
            "abc72f63047353a0d0252db91ff9afff4ebc1e552920549d534c1aee9ee0cca7",
        ),
        (
            "src/main.cjs",
            "ac47bfe96bc20a19af34c71077662021a1ab7b851caf65c2a6979c04f81ba63c",
        ),
    ] {
        let source = original_entry_bootstrap_source(relative).unwrap();
        assert_eq!(format!("{:x}", Sha256::digest(&source)), expected);
        let text = std::str::from_utf8(&source).unwrap();
        assert!(!text.contains("__KELD_SELECTED_ENTRY__"));
        assert!(text.contains(&format!(
            "await bootHostSelectedEntry(() => import(\"./{relative}\"));"
        )));
        assert!(text.contains(&format!("join(canonicalRoot, \"{relative}\")")));
    }
}

#[test]
fn shared_entry_refuses_noncanonical_or_url_sensitive_imports() {
    for invalid in [
        "",
        "/absolute.js",
        ".",
        "src/../other.mjs",
        "src/./entry.js",
        "src\\entry.js",
        "src/entry#fragment.mjs",
        "src/entry?query.mjs",
        "src/%2e%2e/outside.mjs",
        "src/with space.mjs",
        "src/\"injected\".mjs",
        "src/entry\n.js",
    ] {
        assert!(
            original_entry_bootstrap_source(invalid).is_err(),
            "unsupported import spelling accepted: {invalid:?}"
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn historical_macos_aliases_preserve_the_same_owner_bytes() {
    use super::super::launch_facts;
    assert_eq!(
        launch_facts::MACOS_LOCAL_RUNTIME_ENTRY_PATH,
        ORIGINAL_RUNTIME_ENTRY_PATH
    );
    for relative in ["main.js", "src/main.ts"] {
        assert_eq!(
            launch_facts::macos_local_original_entry_bootstrap_source(relative).unwrap(),
            original_entry_bootstrap_source(relative).unwrap()
        );
    }
    let sources = launch_facts::macos_local_launch_bootstrap_sources();
    assert_eq!(sources.len(), 4);
    assert_eq!(
        sources[3],
        (ORIGINAL_RUNTIME_ENTRY_PATH, BOOT_ENTRY_TEMPLATE.as_bytes())
    );
}
