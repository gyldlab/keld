//! Dedicated debug child for the existing held native-preload Quit regression.

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn main() {
    if let Err(error) = run() {
        eprintln!("native custody fixture: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(all(target_os = "macos", feature = "profile-test-hooks")))]
fn main() {
    eprintln!("requires macOS and debug profile-test-hooks; no native proof performed");
    std::process::exit(2);
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    use keld_wv::wkwebview::{VerifiedWebAsset, run_s1_resource_fixture_child};
    use sha2::{Digest as _, Sha256};
    use std::fs::{self, File};
    use std::path::PathBuf;

    const HTML: &[u8] = b"<!doctype html><html><head><meta charset=\"utf-8\"></head><body>Native custody fixture</body></html>\n";
    const POLICY: &[u8] =
        br#"{"profile":"legacy","windows":{"fixture":{"web":{"csp":"original-editor-offline"}}}}"#;
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 2 {
        return Err("expected one new absolute fixture directory".into());
    }
    let root = PathBuf::from(&args[1]);
    if !root.is_absolute() {
        return Err("fixture directory must be absolute".into());
    }
    fs::create_dir(&root)?;
    let webapp = root.join("webapp");
    fs::create_dir(&webapp)?;
    fs::write(webapp.join("index.html"), HTML)?;
    let policy_path = root.join("keld.permissions.jsonc");
    fs::write(&policy_path, POLICY)?;
    let manifest = keld_guard::verified_manifest::load_verified_manifest(
        File::open(&policy_path)?,
        policy_path,
        Sha256::digest(POLICY).into(),
    )?;
    let report = run_s1_resource_fixture_child(
        &webapp,
        vec![VerifiedWebAsset {
            path: PathBuf::from("index.html"),
            size: HTML.len() as u64,
            sha256: Sha256::digest(HTML).into(),
        }],
        Sha256::digest(b"gh1404-held-native-quit-fixture-v1").into(),
        manifest.resolve_window_csp("fixture")?,
        "preload-held-quit",
    )?;
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}
