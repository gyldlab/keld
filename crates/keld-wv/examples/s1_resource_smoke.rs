//! Private macOS child: actual Rust S1 handler, controlled inert test inputs.

#[cfg(target_os = "macos")]
fn main() {
    if let Err(error) = run() {
        eprintln!("S1 native resource fixture: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("S1 native resource fixture requires macOS; no platform proof performed");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    use keld_wv::wkwebview::{VerifiedWebAsset, run_s1_resource_fixture_child};
    use sha2::{Digest as _, Sha256};
    use std::io::Read as _;
    use std::path::{Component, Path, PathBuf};

    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 5 {
        return Err(
            "expected fixture-root manifest-path expected-manifest-sha256 baseline|without-wasm"
                .into(),
        );
    }
    let fixture = PathBuf::from(&args[1]);
    let manifest_path = PathBuf::from(&args[2]);
    let expected = args[3].to_str().ok_or("manifest hash must be Unicode")?;
    if expected != "aac15a1f734d13c146093ed5f470551f0e7f52a9bebdf5cde21bede48528ea5e" {
        return Err(
            "this test binary accepts only the frozen first-party inert fixture capsule".into(),
        );
    }
    let case = args[4].to_str().ok_or("case must be Unicode")?;
    let mut bytes = Vec::new();
    std::fs::File::open(&manifest_path)?
        .take(64 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 64 * 1024 {
        return Err("test manifest exceeds64KiB".into());
    }
    let source_hash = Sha256::digest(&bytes);
    let actual = format!("{source_hash:x}");
    if actual != expected {
        return Err("fixture manifest differs from independent expected hash".into());
    }
    let manifest: serde_json::Value = serde_json::from_slice(&bytes)?;
    if manifest["schema"].as_str() != Some("s1-first-party-source-manifest.v1") {
        return Err("unexpected test-only manifest schema".into());
    }
    let policy = fixture.join("keld.permissions.jsonc");
    let expected_policy = manifest["permissions_file"]["sha256"]
        .as_str()
        .ok_or("fixed policy digest absent")?;
    if expected_policy.len() != 64 {
        return Err("fixed policy digest malformed".into());
    }
    let mut policy_hash = [0_u8; 32];
    for (index, slot) in policy_hash.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&expected_policy[index * 2..index * 2 + 2], 16)?;
    }
    let verified = keld_guard::verified_manifest::load_verified_manifest(
        std::fs::File::open(&policy)?,
        policy,
        policy_hash,
    )?;
    if verified.manifest().declared_profile() != Some(keld_guard::DeclaredProfile::Legacy) {
        return Err(
            "fixture requires its explicit verified Legacy declaration; no fallback".into(),
        );
    }
    let rows = manifest["source_assets"]
        .as_array()
        .ok_or("source assets absent")?;
    if rows.len() != 7 {
        return Err("fixture requires exactly its seven inert source files".into());
    }
    let webapp = fixture.join("webapp");
    let mut assets = Vec::with_capacity(rows.len());
    for row in rows {
        let relative = row["path"].as_str().ok_or("fixture path absent")?;
        if !Path::new(relative)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        {
            return Err("fixture path escapes its retained root".into());
        }
        let expected_size = row["bytes"].as_u64().ok_or("fixture size absent")?;
        if expected_size > 16 * 1024 {
            return Err("inert fixture asset exceeds16KiB".into());
        }
        let mut file = Vec::new();
        std::fs::File::open(webapp.join(relative))?
            .take(expected_size + 1)
            .read_to_end(&mut file)?;
        let digest = Sha256::digest(&file);
        let encoded = format!("{digest:x}");
        if row["bytes"].as_u64() != Some(file.len() as u64)
            || row["sha256"].as_str() != Some(encoded.as_str())
        {
            return Err("inert source differs from frozen pre-admission facts".into());
        }
        assets.push(VerifiedWebAsset {
            path: PathBuf::from(relative),
            size: file.len() as u64,
            sha256: digest.into(),
        });
    }
    let resolved_policy = verified.resolve_window_csp("fixture")?;
    let mut report =
        run_s1_resource_fixture_child(&webapp, assets, source_hash.into(), resolved_policy, case)?;
    report["verified_legacy_declaration_only"] = serde_json::Value::Bool(true);
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}
