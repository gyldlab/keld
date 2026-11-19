//! Byte receipts were derived from the exact pre-extraction template and rendering rule.
use super::*;
use sha2::{Digest, Sha256};

#[test]
fn windows_entry_installs_facts_and_normalizes_argv_only_after_identity_checks()
-> Result<(), Box<dyn std::error::Error>> {
    let entry = "src/deep/main.mjs";
    let windows = String::from_utf8(windows_original_entry_bootstrap_source(entry)?)?;
    let existing = render_original_entry(entry)?;
    let install = concat!(
        "await import(\"./node_modules/electron/dist/index.js\");\n",
        "// The verified wrapper is not a user argument to the original entry.\n",
        "process.argv.splice(1, 1);\n",
    );
    assert_eq!(windows.matches(install).count(), 1);
    assert_eq!(windows.replacen(install, "", 1), existing);
    let after_identity = windows
        .split_once("Selected original does not match sealed bootstrap.")
        .ok_or("missing selected identity check")?
        .1;
    let capture = "await bootHostSelectedEntry(() => import(\"./src/deep/main.mjs\"));";
    let before_capture = after_identity
        .split_once(capture)
        .ok_or("missing runtime capture")?
        .0;
    assert!(before_capture.ends_with(install));
    assert!(windows_original_entry_bootstrap_source("src/../unsealed.mjs").is_err());
    Ok(())
}

#[test]
fn shared_renderer_preserves_pre_extraction_bytes() -> Result<(), HostAppError> {
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
        let text = render_original_entry(relative)?;
        assert_eq!(format!("{:x}", Sha256::digest(text.as_bytes())), expected);
        assert!(!text.contains("__KELD_SELECTED_ENTRY__"));
        assert!(text.contains(&format!(
            "await bootHostSelectedEntry(() => import(\"./{relative}\"));"
        )));
        assert!(text.contains(&format!("join(canonicalRoot, \"{relative}\")")));
    }
    Ok(())
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
            windows_original_entry_bootstrap_source(invalid).is_err(),
            "unsupported import spelling accepted: {invalid:?}"
        );
    }
}

#[path = "packed_windows.rs"]
mod packed_windows;
