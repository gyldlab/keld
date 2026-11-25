//! Parent-owned invocation and profile checks shared by default Windows tests and the native fixture.
//! These checks do not create a `WebView`, profile, or child process.

use std::path::Path;

use serde_json::{Value, json};

pub(super) fn verify_parent_binding(
    receipt: &Value,
    expected_nonce: &str,
    child_pid: u32,
    expected_source: &Value,
) -> Result<(), String> {
    if receipt["nonce"].as_str() != Some(expected_nonce) {
        return Err("native receipt nonce differs from the parent invocation".into());
    }
    if receipt["helper_pid"].as_u64() != Some(u64::from(child_pid)) {
        return Err("native receipt helper PID differs from the reaped child".into());
    }
    if &receipt["source"] != expected_source {
        return Err("native receipt compiled source differs from the parent fixture".into());
    }
    Ok(())
}

pub(super) fn verify_profile_absent(profile: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(profile) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err(format!(
            "owned WebView2 profile still exists after child reap: {}",
            profile.display()
        )),
        Err(error) => Err(format!(
            "cannot independently verify profile removal {}: {error}",
            profile.display()
        )),
    }
}

#[cfg(test)]
mod parent_receipt_tests {
    use super::*;

    #[test]
    fn missing_and_forged_child_invocation_fields_are_rejected() {
        let source = json!({"compiled_fixture_sources":[{"path":"test","sha256":"abc"}]});
        let good = json!({"nonce":"parent-attempt","helper_pid":42,"source":source});
        assert!(verify_parent_binding(&good, "parent-attempt", 42, &source).is_ok());
        for field in ["nonce", "helper_pid", "source"] {
            let mut absent = good.clone();
            absent.as_object_mut().expect("receipt").remove(field);
            assert!(
                verify_parent_binding(&absent, "parent-attempt", 42, &source).is_err(),
                "missing {field} was accepted"
            );
        }
        let mut nonce = good.clone();
        nonce["nonce"] = json!("another-attempt");
        assert!(verify_parent_binding(&nonce, "parent-attempt", 42, &source).is_err());
        let mut pid = good.clone();
        pid["helper_pid"] = json!(43);
        assert!(verify_parent_binding(&pid, "parent-attempt", 42, &source).is_err());
        let mut manifest = good.clone();
        manifest["source"] = json!({"compiled_fixture_sources":[]});
        assert!(verify_parent_binding(&manifest, "parent-attempt", 42, &source).is_err());
    }

    #[test]
    fn profile_removal_requires_parent_owned_filesystem_proof() {
        let mut random = [0_u8; 16];
        getrandom::fill(&mut random).expect("unique test root");
        let root = std::env::temp_dir().join(format!(
            "keld-w3-profile-parent-proof-{:032x}",
            u128::from_le_bytes(random)
        ));
        std::fs::create_dir(&root).expect("create proof root");
        let profile = root.join("owned-webview2-profile");
        assert!(
            verify_profile_absent(&profile).is_ok(),
            "absent profile refused"
        );
        std::fs::create_dir(&profile).expect("create retained profile control");
        assert!(
            verify_profile_absent(&profile).is_err(),
            "retained profile accepted"
        );
        std::fs::remove_dir(&profile).expect("remove control profile");
        assert!(
            verify_profile_absent(&profile).is_ok(),
            "removed profile refused"
        );
        std::fs::remove_dir(&root).expect("remove proof root");
    }
}
