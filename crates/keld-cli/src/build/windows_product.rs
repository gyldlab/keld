//! Ordinary Windows local-test product route. No installer, key creation or
//! trust activation. Project authoring remains the same four-file contract.
use super::{
    BuildError, config, local_archive_command, local_images, local_source, windows_inputs,
};
use serde::Deserialize;
use std::fs::File;
use std::io::Read as _;
use std::path::{Component, Path, PathBuf, Prefix};
const MAX_INPUT: u64 = 64 * 1024;

/// The ephemeral test session may identify only absolute local-disk tools.
/// In particular, UNC/network, device-namespace and traversing paths must not
/// be admitted as executable or public-key locators before source preparation.
/// This is a syntactic admission; retained file/namespace guards still apply.
fn absolute_local_disk_locator(path: &Path) -> bool {
    let mut components = path.components();
    matches!(
        components.next(),
        Some(Component::Prefix(prefix))
            if matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_))
    ) && matches!(components.next(), Some(Component::RootDir))
        && components.all(|part| matches!(part, Component::Normal(_)))
}
fn refuse(e: impl std::fmt::Display) -> BuildError {
    BuildError::input("Windows local-test build", e)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolPin {
    path: PathBuf,
    sha256: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
enum CacheMode {
    Snapshot,
    MutableDeveloper,
}
/// Explicit ephemeral transaction input, never another persistent project file.
/// Public key and existing certificate references only; no secret is parsed.
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct TestSession {
    schema: String,
    expected_source_commit: String,
    git: PathBuf,
    cargo: PathBuf,
    rustc: PathBuf,
    cargo_home: PathBuf,
    output_nonce: String,
    sign_tool: ToolPin,
    certificate_thumbprint: String,
    publisher_scope: String,
    release_public_key: PathBuf,
    published_at: String,
    cache_mode: CacheMode,
}
fn parse_session(bytes: &[u8]) -> Result<TestSession, BuildError> {
    if bytes.is_empty() || bytes.len() as u64 > MAX_INPUT {
        return Err(refuse("session exceeds bounds"));
    }
    let session: TestSession = serde_json::from_slice(bytes).map_err(refuse)?;
    if session.schema != "keld.windows-local-test-session/v1"
        || session.output_nonce.is_empty()
        || session.output_nonce.len() > 64
        || !session
            .output_nonce
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    {
        return Err(refuse(
            "explicit test-session schema and bounded output nonce required",
        ));
    }
    for path in [
        &session.git,
        &session.cargo,
        &session.rustc,
        &session.cargo_home,
        &session.sign_tool.path,
        &session.release_public_key,
    ] {
        if !absolute_local_disk_locator(path) {
            return Err(refuse(
                "session tools and public-key locators must be absolute local-disk paths without traversal",
            ));
        }
    }
    Ok(session)
}
fn require_source_commit(expected: &str, recipe: &str) -> Result<(), BuildError> {
    if expected != recipe {
        return Err(refuse(
            "recipe source commit differs from explicitly selected integrated Host source",
        ));
    }
    Ok(())
}
fn resolve(root: &Path, path: &Path) -> Result<PathBuf, BuildError> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err(refuse("noncanonical build locator"));
    }
    if path.is_absolute() {
        if !absolute_local_disk_locator(path) {
            return Err(refuse("absolute build locator must be on a local disk"));
        }
        Ok(path.to_owned())
    } else if path
        .components()
        .any(|c| matches!(c, Component::Prefix(_) | Component::RootDir))
    {
        Err(refuse("rooted-relative build locator"))
    } else {
        Ok(root.join(path))
    }
}
fn read(pin: &mut local_source::PinnedFile) -> Result<Vec<u8>, BuildError> {
    let mut bytes = Vec::new();
    (&mut pin.file)
        .take(MAX_INPUT + 1)
        .read_to_end(&mut bytes)
        .map_err(refuse)?;
    if bytes.len() as u64 > MAX_INPUT {
        return Err(refuse("input grew beyond bound"));
    }
    Ok(bytes)
}
/// Retains the actual project/config/session namespace throughout the same
/// source/image/archive transaction. Config cannot choose a command or installer.
pub(super) fn run(session_path: &Path) -> Result<(), BuildError> {
    if !session_path.is_absolute() {
        return Err(refuse(
            "--local-test-session requires an absolute ephemeral input locator",
        ));
    }
    let root = std::env::current_dir().map_err(refuse)?;
    let _project = windows_inputs::pin_input_root(&root)?;
    let mut build = local_source::pin_file(&root.join("keld.build.ts"), MAX_INPUT)?;
    let bytes = read(&mut build)?;
    let recipe = config::parse_windows_build(std::str::from_utf8(&bytes).map_err(refuse)?)?;
    if recipe.signing != "test-local-untimestamped" {
        return Err(refuse(
            "explicit local-test session cannot select developer signer",
        ));
    }
    let parent = session_path
        .parent()
        .ok_or_else(|| refuse("session input parent missing"))?;
    let _session_parents = windows_inputs::pin_input_root(parent)?;
    let mut session_pin = local_source::pin_file(session_path, MAX_INPUT)?;
    let session = parse_session(&read(&mut session_pin)?)?;
    require_source_commit(&session.expected_source_commit, &recipe.source_commit)?;
    let out_dir = resolve(&root, &recipe.out_dir)?;
    // Existing output root is admitted, not silently created/repaired with a new
    // policy. Same source owner creates only the fresh transaction child below it.
    let _output: Vec<File> = windows_inputs::pin_input_root(&out_dir)?;
    let source = local_source::Request {
        source: resolve(&root, &recipe.source_root)?,
        commit: recipe.source_commit,
        session: out_dir.join(format!("session-{}", session.output_nonce)),
        git: session.git,
        cargo: session.cargo,
        rustc: session.rustc,
        cargo_home: session.cargo_home,
        cache_mode: match session.cache_mode {
            CacheMode::Snapshot => local_source::CacheMode::Snapshot,
            CacheMode::MutableDeveloper => local_source::CacheMode::MutableDeveloper,
        },
    };
    let images = local_images::ImageRequest {
        source,
        app_root: root.clone(),
        channel: recipe.channel,
        release_public_key: session.release_public_key,
        sign_tool: session.sign_tool.path,
        sign_tool_sha256: session.sign_tool.sha256,
        certificate_thumbprint: session.certificate_thumbprint,
        publisher_scope: session.publisher_scope,
        profile_admission: windows_inputs::ProfileAdmission::ExplicitStrict,
    };
    let compiler = local_archive_command::CompilerRequest {
        typescript_root: resolve(&root, &recipe.typescript_root)?,
        bun: resolve(&root, &recipe.bun.path)?,
        bun_sha256: recipe.bun.sha256,
        tsc: resolve(&root, &recipe.tsc.path)?,
        tsc_sha256: recipe.tsc.sha256,
    };
    println!(
        "Building Windows LOCAL TEST package with an existing test signer; not an official release or installation."
    );
    local_archive_command::run_product(images, compiler, &recipe.modules, &session.published_at)
}
/// Ordinary Windows developer build: select only an existing signer from the
/// project's closed public config and create a fresh, unpredictable output leaf.
/// The existing authenticated source, image and archive owners are reused.
pub(super) fn run_developer(install: Option<(&Path, &str, bool)>) -> Result<(), BuildError> {
    // Fail a malformed install request before compiler/source or filesystem work.
    if let Some((input, published_at, _)) = install {
        local_archive_command::validate_stamp(published_at)?;
        if !absolute_local_disk_locator(input) {
            return Err(refuse(
                "developer initial-install inputs must be absolute local-disk paths without traversal",
            ));
        }
    }
    let root = std::env::current_dir().map_err(refuse)?;
    let _project = windows_inputs::pin_input_root(&root)?;
    let mut file = local_source::pin_file(&root.join("keld.build.ts"), MAX_INPUT)?;
    let recipe =
        config::parse_windows_build(std::str::from_utf8(&read(&mut file)?).map_err(refuse)?)?;
    if recipe.signing != "developer-local" {
        return Err(refuse(
            "ordinary Windows build requires signing: 'developer-local' in keld.build.ts",
        ));
    }
    let signer = recipe
        .developer_signer
        .ok_or_else(|| refuse("developerSigner absent"))?;
    let mut nonce_bytes = [0_u8; 16];
    getrandom::fill(&mut nonce_bytes).map_err(refuse)?;
    let nonce = local_source::observed_hex(&nonce_bytes);
    let out_dir = resolve(&root, &recipe.out_dir)?;
    let _output = windows_inputs::pin_input_root(&out_dir)?;
    let source = local_source::Request {
        source: resolve(&root, &recipe.source_root)?,
        commit: recipe.source_commit,
        session: out_dir.join(format!("session-{nonce}")),
        git: resolve(&root, &signer.git)?,
        cargo: resolve(&root, &signer.cargo)?,
        rustc: resolve(&root, &signer.rustc)?,
        cargo_home: resolve(&root, &signer.cargo_home)?,
        cache_mode: local_source::CacheMode::MutableDeveloper,
    };
    let images = local_images::ImageRequest {
        source,
        app_root: root.clone(),
        channel: recipe.channel,
        release_public_key: resolve(&root, &signer.release_public_key)?,
        sign_tool: resolve(&root, &signer.sign_tool.path)?,
        sign_tool_sha256: local_source::observed_hex(&signer.sign_tool.sha256),
        certificate_thumbprint: signer.certificate_thumbprint,
        publisher_scope: signer.publisher_scope,
        profile_admission: windows_inputs::ProfileAdmission::UndeclaredDeveloper,
    };
    let compiler = local_archive_command::CompilerRequest {
        typescript_root: resolve(&root, &recipe.typescript_root)?,
        bun: resolve(&root, &recipe.bun.path)?,
        bun_sha256: recipe.bun.sha256,
        tsc: resolve(&root, &recipe.tsc.path)?,
        tsc_sha256: recipe.tsc.sha256,
    };
    println!(
        "Building Windows DEVELOPER LOCAL application using the explicitly selected existing certificate; no public release or key creation."
    );
    if let Some((inputs, published_at, create_scaffold)) = install {
        // The external signer must respond to the exact one-use emitted manifest.
        // Only the retained, authenticated signed Host may commit the baseline.
        local_archive_command::run_developer_initial_install(
            images,
            compiler,
            &recipe.modules,
            published_at,
            inputs,
            create_scaffold,
        )
    } else {
        local_archive_command::run_developer(images, compiler, &recipe.modules)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn session() -> serde_json::Value {
        serde_json::json!({"schema":"keld.windows-local-test-session/v1","expectedSourceCommit":"1".repeat(40),"git":"D:/git.exe","cargo":"D:/cargo.exe","rustc":"D:/rustc.exe","cargoHome":"D:/cache","outputNonce":"test-1","signTool":{"path":"D:/signtool.exe","sha256":"2".repeat(64)},"certificateThumbprint":"3".repeat(40),"publisherScope":"4".repeat(64),"releasePublicKey":"D:/public.raw","publishedAt":"2026-11-11T00:00:00Z","cacheMode":"mutable-developer"})
    }
    #[test]
    fn ephemeral_input_cannot_override_project_or_create_authority() {
        let value = session();
        assert!(parse_session(&serde_json::to_vec(&value).unwrap()).is_ok());
        for key in [
            "privateKey",
            "command",
            "install",
            "source",
            "modules",
            "appId",
            "createCertificate",
        ] {
            let mut bad = value.clone();
            bad[key] = serde_json::json!("override");
            assert!(parse_session(&serde_json::to_vec(&bad).unwrap()).is_err());
        }
        for nonce in ["..", "../other", "a/b", ""] {
            let mut bad = value.clone();
            bad["outputNonce"] = serde_json::json!(nonce);
            assert!(parse_session(&serde_json::to_vec(&bad).unwrap()).is_err());
        }
    }
    #[test]
    fn local_test_session_refuses_network_and_traversing_tool_locators() {
        let good = session();
        for unsafe_path in [
            r"\\server\share\git.exe",
            r"\\?\UNC\server\share\git.exe",
            "D:/tools/../git.exe",
            "D:tools/git.exe",
        ] {
            for key in ["git", "cargo", "rustc", "cargoHome", "releasePublicKey"] {
                let mut bad = good.clone();
                bad[key] = serde_json::json!(unsafe_path);
                assert!(
                    parse_session(&serde_json::to_vec(&bad).unwrap()).is_err(),
                    "{key} accepted an unqualified Windows locator"
                );
            }
            let mut bad = good.clone();
            bad["signTool"]["path"] = serde_json::json!(unsafe_path);
            assert!(
                parse_session(&serde_json::to_vec(&bad).unwrap()).is_err(),
                "signTool accepted an unqualified Windows locator"
            );
        }
        assert!(resolve(Path::new("D:/project"), Path::new(r"\\server\share\out")).is_err());
        assert!(resolve(Path::new("D:/project"), Path::new("E:/out/../other")).is_err());
    }
    #[test]
    fn developer_install_refuses_invalid_stamp_or_external_locator_before_project_io() {
        let bad_stamp = run_developer(Some((
            Path::new("D:/fresh/inputs.json"),
            "not-a-utc-timestamp",
            true,
        )))
        .expect_err("invalid publishedAt must refuse before project input");
        assert!(bad_stamp.to_string().contains("UTC"), "{bad_stamp}");

        for path in [
            r"\\server\share\inputs.json",
            "D:/outside/../inputs.json",
            "D:relative.json",
        ] {
            let error = run_developer(Some((Path::new(path), "2026-11-13T15:00:00Z", true)))
                .expect_err("untrusted install locator must refuse before project input");
            assert!(error.to_string().contains("local-disk"), "{error}");
        }
    }

    #[test]
    fn stale_host_commit_cannot_replace_selected_integrated_source() {
        require_source_commit(&"1".repeat(40), &"1".repeat(40)).unwrap();
        assert!(require_source_commit(&"1".repeat(40), &"2".repeat(40)).is_err());
    }
    #[test]
    fn relative_recipe_paths_have_one_project_root_without_traversal() {
        let root = Path::new("D:/app");
        assert_eq!(resolve(root, Path::new("out")).unwrap(), root.join("out"));
        assert!(resolve(root, Path::new("../out")).is_err());
        assert!(resolve(root, Path::new("./out")).is_err());
    }
}
