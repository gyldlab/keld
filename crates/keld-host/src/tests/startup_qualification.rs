//! Bounded macOS factory qualification. These observations do not acquire a
//! trusted production runtime or qualify the UNKNOWN binary-token inventory.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::io::Write as _;
use std::os::unix::fs::MetadataExt as _;
use std::path::Path;
use std::process::{Command, Stdio};

use super::super::{freeze_guardian_environment, guardian_bun_command};

mod warm_cache;

const BUN_NAME_INVENTORY: &str = include_str!("fixtures/bun-startup-identifiers.json");

fn isolated_environment(root: &Path) -> Result<Vec<(OsString, OsString)>, String> {
    freeze_guardian_environment([
        (
            "PATH".into(),
            std::env::var_os("PATH")
                .ok_or_else(|| "development Bun selection PATH is absent".to_owned())?,
        ),
        ("HOME".into(), root.as_os_str().to_owned()),
        (
            "BUN_INSTALL_CACHE_DIR".into(),
            root.join("cache").into_os_string(),
        ),
    ])
    .map_err(|error| error.to_string())
}

fn factory_command(
    root: &Path,
    entry: &Path,
    environment: &[(OsString, OsString)],
) -> Result<Command, String> {
    let identity = fs::metadata(root.join(entry)).map_err(|error| error.to_string())?;
    guardian_bun_command(
        &Ok(root.to_path_buf()),
        root,
        entry,
        (identity.dev(), identity.ino()),
        true,
        environment,
        "/tmp/startup-qualification#fixture-only",
    )
    .map_err(|error| error.to_string())
}

fn binary_identifier_tokens(bytes: &[u8]) -> BTreeSet<String> {
    bytes
        .split(|byte| !(byte.is_ascii_alphanumeric() || *byte == b'_'))
        .filter(|token| token.starts_with(b"BUN_") || token.starts_with(b"NODE_"))
        .map(|token| token.iter().copied().map(char::from).collect())
        .collect()
}

fn unclassified_tokens<'a>(
    tokens: &'a BTreeSet<String>,
    classifications: &serde_json::Map<String, serde_json::Value>,
) -> Vec<&'a str> {
    tokens
        .iter()
        .filter(|token| !classifications.contains_key(*token))
        .map(String::as_str)
        .collect()
}

// Hash the same already-read bytes the scanner consumes. This native macOS
// observer adds no Cargo edge and does not reopen the Bun path.
fn digest_observation(binary: &[u8]) -> Result<std::process::Output, String> {
    let mut observer = Command::new("/usr/bin/shasum")
        .args(["-a", "256"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    let input_result = match observer.stdin.take() {
        Some(mut input) => input.write_all(binary),
        None => Err(std::io::Error::other("observer stdin pipe is absent")),
    };
    // The input closes before waiting. Reap even when writing failed.
    let output = observer
        .wait_with_output()
        .map_err(|error| error.to_string())?;
    input_result.map_err(|error| error.to_string())?;
    Ok(output)
}

#[test]
fn guardian_pinned_bun_identifier_census_has_explicit_classifications() {
    let temp = tempfile::tempdir().expect("pinned binary observer fixture");
    let entry = Path::new("entry.ts");
    fs::write(temp.path().join(entry), "console.log(process.execPath);\n")
        .expect("actual executable-path observer");
    let environment = isolated_environment(temp.path()).expect("fixture environment");
    let output = factory_command(temp.path(), entry, &environment)
        .expect("command from the shipping factory")
        .output()
        .expect("execute the selected factory command");
    assert!(
        output.status.success(),
        "runtime observer failed: {output:?}"
    );
    let selected_path = String::from_utf8(output.stdout).expect("executable-path receipt");
    let selected_path = selected_path.trim_end_matches(['\r', '\n']);
    assert!(
        Path::new(selected_path).is_absolute(),
        "absolute executable path"
    );
    let binary = fs::read(selected_path).expect("selected Bun binary bytes");
    let inventory: serde_json::Value =
        serde_json::from_str(BUN_NAME_INVENTORY).expect("fixed qualification inventory");
    let digest_output = digest_observation(&binary).expect("native SHA256 observation");
    assert!(
        digest_output.status.success(),
        "SHA256 observer failed: {digest_output:?}"
    );
    let digest_receipt = String::from_utf8(digest_output.stdout).expect("SHA256 receipt");
    let mut digest_fields = digest_receipt.split_whitespace();
    let actual_digest = digest_fields.next().expect("SHA256 receipt has a digest");
    assert_eq!(actual_digest.len(), 64, "exact SHA256 hexadecimal width");
    assert!(
        actual_digest.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "SHA256 hexadecimal receipt"
    );
    assert_eq!(digest_fields.next(), Some("-"), "digest input was stdin");
    assert!(digest_fields.next().is_none(), "one digest receipt only");
    assert_eq!(
        actual_digest,
        inventory["binary_sha256"]
            .as_str()
            .expect("pinned binary SHA256"),
        "a different development Bun is unqualified; do not skip or refresh the inventory automatically"
    );
    assert_eq!(
        u64::try_from(binary.len()).expect("binary size fits u64"),
        inventory["binary_bytes"]
            .as_u64()
            .expect("pinned binary size")
    );
    let tokens = binary_identifier_tokens(&binary);
    let classifications = inventory["classifications"]
        .as_object()
        .expect("fixed token classifications");
    for (token, classification) in classifications {
        assert!(
            matches!(
                classification.as_str(),
                Some("UNKNOWN" | "REMOVED_BUN_OPTIONS" | "REFUSED_LOADER_CONTROL")
            ),
            "{token} has no supported explicit classification"
        );
    }
    assert_eq!(
        classifications
            .get("BUN_OPTIONS")
            .and_then(serde_json::Value::as_str),
        Some("REMOVED_BUN_OPTIONS")
    );
    for token in [
        "BUN_INSPECT_PRELOAD",
        "NODE_PATH",
        "BUN_INSPECT",
        "BUN_INSPECT_CONNECT_TO",
        "BUN_INSPECT_NOTIFY",
    ] {
        assert_eq!(
            classifications
                .get(token)
                .and_then(serde_json::Value::as_str),
            Some("REFUSED_LOADER_CONTROL")
        );
    }
    let unknown_count = classifications
        .values()
        .filter(|value| value.as_str() == Some("UNKNOWN"))
        .count();
    assert_eq!(unknown_count, 193, "UNKNOWN tokens remain unqualified");
    let unclassified = unclassified_tokens(&tokens, classifications);
    assert!(
        unclassified.is_empty(),
        "new binary tokens need owner review: {unclassified:?}"
    );
    let fixed_tokens: BTreeSet<_> = classifications.keys().cloned().collect();
    assert_eq!(tokens, fixed_tokens, "the fixed binary census changed");
    assert_eq!(tokens.len(), 199, "deterministic token inventory size");
    println!("BUN_IDENTIFIER_CENSUS:sha256={actual_digest};tokens=199;unknown={unknown_count}");
}

#[test]
fn guardian_binary_census_preserves_whole_ascii_tokens() {
    let tokens = binary_identifier_tokens(
        b"BUN_OPTIONS\0NODE_PATH-\xffBUN_ENVNODE_ENV BUN_OPTIONS NODE_ prefixBUN_IGNORED node_lower",
    );
    let expected: BTreeSet<_> = ["BUN_ENVNODE_ENV", "BUN_OPTIONS", "NODE_", "NODE_PATH"]
        .map(str::to_owned)
        .into_iter()
        .collect();
    assert_eq!(tokens, expected);
}

#[test]
fn guardian_binary_census_rejects_an_unclassified_token() {
    let inventory: serde_json::Value =
        serde_json::from_str(BUN_NAME_INVENTORY).expect("fixed qualification inventory");
    let classifications = inventory["classifications"]
        .as_object()
        .expect("fixed token classifications");
    let mut tokens: BTreeSet<_> = classifications.keys().cloned().collect();
    assert_eq!(
        unclassified_tokens(&tokens, classifications),
        [] as [&str; 0]
    );
    let new_token = "BUN_KELD_UNCLASSIFIED_NEGATIVE_CONTROL";
    assert!(tokens.insert(new_token.to_owned()));
    assert_eq!(unclassified_tokens(&tokens, classifications), [new_token]);
}

#[test]
fn guardian_refuses_staged_force_install_before_constructing_bun() {
    let temp = tempfile::tempdir().expect("force-install config fixture");
    let entry = Path::new("entry.ts");
    fs::write(temp.path().join(entry), "console.log('ENTRY_EXECUTED');\n").expect("entry observer");
    let environment = isolated_environment(temp.path()).expect("fixture environment");
    let output = factory_command(temp.path(), entry, &environment)
        .expect("command from the shipping factory")
        .output()
        .expect("healthy actual Bun counterweight");
    assert!(output.status.success(), "healthy entry failed: {output:?}");
    assert_eq!(output.stdout, b"ENTRY_EXECUTED\n");
    let identity = fs::metadata(temp.path().join(entry)).expect("unchanged entry identity");
    for name in ["bunfig.toml", ".bunfig.toml"] {
        let config = temp.path().join(name);
        fs::write(&config, "[install]\nauto = \"force\"\n")
            .expect("fixture-owned force-install config");
        let error = guardian_bun_command(
            &Ok(temp.path().to_path_buf()),
            temp.path(),
            entry,
            (identity.dev(), identity.ino()),
            true,
            &environment,
            "/tmp/startup-qualification#fixture-only",
        )
        .expect_err("force-install config refuses before a candidate command exists");
        let diagnostic = error.to_string();
        assert!(diagnostic.contains("KELD-CORE-037"), "{diagnostic}");
        assert!(diagnostic.contains(name), "{diagnostic}");
        fs::remove_file(config).expect("remove only the owned force-install fixture");
    }
}

// The scenario keeps every product outcome visible; the adjacent fixture owns
// registry, cache preparation, bounded child execution and teardown.
#[test]
fn guardian_warm_cache_missing_package_requires_no_install() {
    let fixture = warm_cache::Fixture::prepare().expect("isolated real Bun cache fixture");
    let entry = Path::new("entry.ts");
    let guarded = factory_command(&fixture.stage, entry, &fixture.environment)
        .expect("actual Guardian factory");
    let before = fixture.request_count();
    let denied = fixture
        .run("guarded", guarded)
        .expect("bounded guarded Bun");
    assert_eq!(
        denied.status.code(),
        Some(1),
        "missing alias unexpectedly resolved: {denied:?}"
    );
    let error = String::from_utf8_lossy(&denied.stderr);
    assert!(
        error.contains("Cannot find package") && error.contains(warm_cache::PACKAGE),
        "{error}"
    );
    for observed in [&denied.stdout, &denied.stderr] {
        assert!(
            !observed
                .windows(warm_cache::MARKER.len())
                .any(|s| s == warm_cache::MARKER.as_bytes())
        );
        assert!(
            !observed
                .windows(b"ENTRY_RAN".len())
                .any(|s| s == b"ENTRY_RAN")
        );
    }
    assert_eq!(
        fixture.request_count(),
        before,
        "denied command reached its registry"
    );

    // Only remove the selected flag AFTER exercising the real guarded outcome.
    let original = factory_command(&fixture.stage, entry, &fixture.environment)
        .expect("actual factory counterweight");
    let counterweight = warm_cache::without_install_flag(&original)
        .expect("exact one-flag mutation, same executable/cwd/frozen environment");
    let before = fixture.request_count();
    let allowed = fixture
        .run("counterweight", counterweight)
        .expect("bounded counterweight");
    assert!(
        allowed.status.success(),
        "warm-cache counterweight failed: {allowed:?}"
    );
    assert_eq!(allowed.stdout, b"CACHE_PACKAGE_EVALUATED\nENTRY_RAN\n");
    assert_eq!(
        fixture.request_count(),
        before,
        "counterweight fetched instead of using its warm cache"
    );

    let installed = fixture
        .materialize_installed()
        .expect("copy exact Bun-installed bytes");
    let healthy = factory_command(&installed, entry, &fixture.environment)
        .expect("actual factory installed-package positive");
    let before = fixture.request_count();
    let pass = fixture
        .run("installed", healthy)
        .expect("bounded installed-package positive");
    assert!(pass.status.success(), "installed package failed: {pass:?}");
    assert_eq!(pass.stdout, b"CACHE_PACKAGE_EVALUATED\nENTRY_RAN\n");
    assert_eq!(
        fixture.request_count(),
        before,
        "installed positive reached its registry"
    );
    println!(
        "KEL75_E5_ACTUAL: Bun1.4.4;seed_registry_requests={};no_install_denied;cache_counterweight_ran;installed_positive",
        fixture.request_count()
    );
    fixture
        .finish()
        .expect("owned registry terminates without masking a failure");
}
