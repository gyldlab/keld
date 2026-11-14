//! Native local macOS packaging and Windows local-test build commands.
//!
//! This is cold-tool orchestration. Core owns literal extraction, Guard owns
//! permissions, the existing boot compiler owns the sealed descriptor, and
//! Pack owns resource capture, code signing, image verification and publication.

#[cfg(windows)]
mod compiler_toolchain;
#[cfg(any(target_os = "macos", windows, test))]
mod config;
#[cfg(windows)]
mod local_archive;
#[cfg(windows)]
mod local_archive_command;
#[cfg(all(windows, test))]
mod local_baseline;
#[cfg(windows)]
mod local_images;
#[cfg(windows)]
mod local_initial_install;
#[cfg(windows)]
mod local_manifest_session;
#[cfg(windows)]
mod local_source;
#[cfg(windows)]
mod windows_archive;
#[cfg(windows)]
mod windows_compiler;
#[cfg(windows)]
mod windows_inputs;
#[cfg(windows)]
mod windows_product;

use std::fmt;

#[cfg(target_os = "macos")]
use std::env;
#[cfg(target_os = "macos")]
use std::fs::File;
#[cfg(target_os = "macos")]
use std::io::Read;
#[cfg(target_os = "macos")]
use std::path::Path;

#[cfg(target_os = "macos")]
use keld_pack::{
    MacosBundleMetadata, MacosExecutableInput, MacosGeneratedFile, MacosGraphCompiled,
    MacosPackageInput, produce_macos_local_with_graph,
};

#[cfg(target_os = "macos")]
use crate::boot::{
    self, compile_boot_descriptor_bytes, compile_installed_graph_boot_descriptor_bytes,
};

/// Typed local build failure. No developer-stage fallback follows a refusal.
#[derive(Debug)]
pub struct BuildError {
    phase: &'static str,
    detail: String,
    usage: bool,
}

impl BuildError {
    #[cfg(any(target_os = "macos", windows, test))]
    fn input(phase: &'static str, detail: impl fmt::Display) -> Self {
        Self {
            phase,
            detail: detail.to_string(),
            usage: false,
        }
    }

    fn invocation(detail: impl fmt::Display) -> Self {
        Self {
            phase: "invocation",
            detail: detail.to_string(),
            usage: true,
        }
    }

    /// Exit 2 for a malformed command, 1 for failed input admission or building.
    #[must_use]
    pub const fn exit_code(&self) -> i32 {
        if self.usage { 2 } else { 1 }
    }
}

impl fmt::Display for BuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "KELD-CLI-052: build input or assembly refused during {}: {}. \
             Use a supported native target with closed literal authoring files. \
             Windows --check-app-inputs checks authoring only; it does not build an application.",
            self.phase, self.detail
        )
    }
}

impl std::error::Error for BuildError {}

#[cfg(any(target_os = "macos", test))]
fn require_local_legacy_profile(
    manifest: &keld_guard::PermissionsManifest,
) -> Result<(), BuildError> {
    match manifest.declared_profile() {
        Some(keld_guard::DeclaredProfile::Legacy) => Ok(()),
        Some(keld_guard::DeclaredProfile::Strict) => Err(BuildError::input(
            "security profile",
            "local macOS packaging does not qualify Strict containment; explicitly select Legacy only if accepting ambient Bun/OS authority",
        )),
        None => Err(BuildError::input(
            "security profile",
            "the first local macOS installed cell requires an explicit root /profile: \"legacy\" in keld.permissions.jsonc; Legacy forfeits the zero-ambient-authority claim and must be consciously selected",
        )),
    }
}

/// The application source and its complete build-only graph expectations must
/// appear together. Missing information never silently selects an ordinary
/// renderer or an unqualified native facade.
#[cfg(target_os = "macos")]
fn require_graph_pairing(
    app: &config::AppConfig,
    build: &config::LocalBuildConfig,
) -> Result<(), BuildError> {
    match (app.renderer_source.as_ref(), build.graph_source.as_ref()) {
        (None, None) | (Some(_), Some(_)) => Ok(()),
        _ => Err(BuildError::input(
            "graph source selection",
            "rendererSource in keld.config.ts and graphSource in keld.build.ts must be provided together",
        )),
    }
}

fn is_windows_app_input_check(args: &[String]) -> bool {
    matches!(args, [target, check] if target == "--target=windows-x64" && check == "--check-app-inputs")
        || matches!(args, [flag, target, check] if flag == "--target" && target == "windows-x64" && check == "--check-app-inputs")
}

/// User-facing Windows developer build and the retained explicit local-test route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowsProductRoute<'a> {
    Developer,
    DeveloperInitialInstall {
        inputs: &'a str,
        published_at: &'a str,
        create_scaffold: bool,
    },
    LocalTest(&'a str),
}
fn windows_product_session(args: &[String]) -> Result<Option<WindowsProductRoute<'_>>, BuildError> {
    let rest = match args {
        [flag, target, rest @ ..] if flag == "--target" && target == "windows-x64" => Some(rest),
        [target, rest @ ..] if target == "--target=windows-x64" => Some(rest),
        _ => None,
    };
    match rest {
        None => Ok(None),
        Some([]) => Ok(Some(WindowsProductRoute::Developer)),
        Some([flag, path]) if flag == "--local-test-session" && !path.is_empty() => {
            Ok(Some(WindowsProductRoute::LocalTest(path)))
        }
        Some([flag, inputs, stamp_flag, stamp])
            if matches!(
                flag.as_str(),
                "--initial-install-observed-inputs" | "--initial-install-create-scaffold-inputs"
            ) && !inputs.is_empty()
                && stamp_flag == "--published-at"
                && !stamp.is_empty() =>
        {
            Ok(Some(WindowsProductRoute::DeveloperInitialInstall {
                inputs,
                published_at: stamp,
                create_scaffold: flag == "--initial-install-create-scaffold-inputs",
            }))
        }
        Some(_) => Err(BuildError::invocation(
            "Windows build accepts developer-local keld.build.ts, an explicit --local-test-session ABS_JSON, or --initial-install-{observed,create-scaffold}-inputs ABS_JSON --published-at UTC; install requires separate signed catalog and one-use manifest signer",
        )),
    }
}

#[cfg(windows)]
fn run_windows_product(route: WindowsProductRoute<'_>) -> Result<(), BuildError> {
    match route {
        WindowsProductRoute::Developer => windows_product::run_developer(None),
        WindowsProductRoute::DeveloperInitialInstall {
            inputs,
            published_at,
            create_scaffold,
        } => windows_product::run_developer(Some((
            std::path::Path::new(inputs),
            published_at,
            create_scaffold,
        ))),
        WindowsProductRoute::LocalTest(session) => {
            windows_product::run(std::path::Path::new(session))
        }
    }
}

fn parse_args(args: &[String]) -> Result<(), BuildError> {
    match args {
        [flag, target] if flag == "--target" && target == "macos-arm64" => Ok(()),
        [arg] if arg == "--target=macos-arm64" => Ok(()),
        _ => Err(BuildError::invocation(
            "the initial build supports only --target macos-arm64",
        )),
    }
}

const BUILD_USAGE: &str = "usage:
  keld build --target macos-arm64
  keld build --target windows-x64 --check-app-inputs
  keld build --target windows-x64
  keld build --target windows-x64 --local-test-session ABS_JSON
  keld build --target windows-x64 --initial-install-create-scaffold-inputs ABS_JSON --published-at UTC

Targets require their native operating system; macos-arm64 requires Apple Silicon.
Windows --check-app-inputs validates authoring only; it does not build an app.
Windows developer-local builds select an existing certificate through closed
keld.build.ts authoring (developerSigner); the build never imports private keys.
Windows --local-test-session preserves the original explicit LOCAL TEST route.
Windows initial installation requires an existing signed catalog, an authorized
one-use Ed25519 manifest signer and explicit install/update roots; it invokes the
existing verified Host in the same retained build transaction. Build success alone
is not proof of installation, app launch or Strict runtime qualification.
Public release trust and Store publication are not part of this developer build.";

/// Builds the selected local cell using the invocation directory.
///
/// The command validates its target and native platform before any file write.
/// Successful output is published only by Pack after its signature/DMG checks.
///
/// # Errors
/// Refuses missing or dynamic authoring, unsupported platform/target/signing,
/// missing input pins, invalid permissions and native tool failures. Independent
/// source/runtime qualification remains the caller's separate evidence obligation.
pub fn run_build(args: &[String]) -> Result<(), BuildError> {
    if matches!(args, [flag] if flag == "--help" || flag == "-h") {
        println!("{BUILD_USAGE}");
        return Ok(());
    }
    if args.starts_with(&[
        "--target".into(),
        "windows-x64".into(),
        "--prepare-local-source-test-archive".into(),
    ]) {
        #[cfg(windows)]
        {
            return local_archive_command::run(&args[3..]);
        }
        #[cfg(not(windows))]
        {
            return Err(BuildError::invocation(
                "local Windows archive assembly requires native Windows",
            ));
        }
    }
    if args.starts_with(&[
        "--target".into(),
        "windows-x64".into(),
        "--prepare-local-source-test-images".into(),
    ]) {
        #[cfg(windows)]
        {
            return local_images::run(&args[3..]);
        }
        #[cfg(not(windows))]
        {
            return Err(BuildError::invocation(
                "local-source image preparation requires native Windows; nothing was signed",
            ));
        }
    }
    if args.starts_with(&[
        "--target".into(),
        "windows-x64".into(),
        "--prepare-local-source-test".into(),
    ]) {
        #[cfg(windows)]
        {
            return local_source::run(&args[3..]);
        }
        #[cfg(not(windows))]
        {
            return Err(BuildError::invocation(
                "local-source test preparation requires native Windows; no source was read or built",
            ));
        }
    }
    if is_windows_app_input_check(args) {
        #[cfg(windows)]
        {
            return windows_inputs::run();
        }
        #[cfg(not(windows))]
        {
            return Err(BuildError::invocation(
                "Windows app-input checking requires native Windows; no inputs were read",
            ));
        }
    }
    if let Some(route) = windows_product_session(args)? {
        #[cfg(windows)]
        {
            return run_windows_product(route);
        }
        #[cfg(not(windows))]
        {
            let _ = route;
            return Err(BuildError::invocation(
                "Windows product build requires native Windows; no source was built or signed",
            ));
        }
    }
    parse_args(args)?;
    if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        return Err(BuildError::invocation(
            "macos-arm64 requires native Apple Silicon macOS; no cross-host signing",
        ));
    }
    #[cfg(target_os = "macos")]
    {
        run_macos_native()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err(BuildError::invocation(
            "macos-arm64 packaging is available only on macOS",
        ))
    }
}

#[cfg(target_os = "macos")]
fn read_authoring(root: &Path, name: &'static str) -> Result<String, BuildError> {
    use rustix::fs::{Mode, OFlags, open, openat};

    // The opened directory and source file are held throughout the bounded read.
    // Do not follow a swapped symlink or accept a FIFO/device as configuration.
    let directory: File = open(
        root,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| BuildError::input(name, error))?
    .into();
    let source: File = openat(
        &directory,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
        Mode::empty(),
    )
    .map_err(|error| BuildError::input(name, error))?
    .into();
    let metadata = source
        .metadata()
        .map_err(|error| BuildError::input(name, error))?;
    if !metadata.is_file() {
        return Err(BuildError::input(
            name,
            "authoring input must be a regular file",
        ));
    }
    boot::ensure_unix_metadata_owned_by(
        &root.join(name),
        &metadata,
        rustix::process::geteuid().as_raw(),
    )
    .map_err(|error| BuildError::input(name, error))?;
    let mut bytes = Vec::new();
    source
        .take(64 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| BuildError::input(name, error))?;
    if bytes.len() > 64 * 1024 {
        return Err(BuildError::input(name, "authoring file exceeds 64 KiB"));
    }
    String::from_utf8(bytes).map_err(|error| BuildError::input(name, error))
}

#[cfg(target_os = "macos")]
fn select_installed_graph_capture(
    app: &config::AppConfig,
    build: &config::LocalBuildConfig,
) -> Result<
    Option<(
        keld_core::app_session::SelectedRendererSource,
        keld_core::app_session::ApprovedGraphCaptureInput,
    )>,
    BuildError,
> {
    match (app.renderer_source.as_ref(), build.graph_source.as_ref()) {
        (Some(renderer), Some(graph)) => {
            let selected = keld_core::app_session::SelectedRendererSource::new(
                &renderer.root,
                &renderer.entry,
                &graph.preload.path,
                graph.preload.sha256,
            )
            .map_err(|error| BuildError::input("selected renderer source", error))?;
            let expected = keld_core::app_session::ApprovedGraphCaptureInput {
                expected_package_sha256: graph.package_sha256,
                expected_lock_sha256: graph.lock_sha256,
                expected_package_trees: graph.provider_trees,
            };
            Ok(Some((selected, expected)))
        }
        (None, None) => Ok(None),
        _ => Err(BuildError::input(
            "graph source selection",
            "rendererSource/graphSource pairing changed during closed input capture",
        )),
    }
}

#[cfg(target_os = "macos")]
fn compile_local_boot_bytes(
    graph_selected: bool,
    app_source: &str,
    app: &config::AppConfig,
    permissions: &[u8],
) -> Result<Vec<u8>, BuildError> {
    // Installed graph boot is compiled only from the *copied* graph inventory
    // inside Pack's retained, unsigned candidate. Ordinary 1–4 stay byte-exact.
    if graph_selected {
        Ok(Vec::new())
    } else {
        compile_boot_descriptor_bytes(
            app_source,
            &app.name,
            &app.entry,
            app.renderer.as_deref(),
            permissions,
        )
        .map_err(|error| BuildError::input("boot compiler", error))
    }
}

#[cfg(target_os = "macos")]
fn run_macos_native() -> Result<(), BuildError> {
    let root = env::current_dir()
        .and_then(|path| path.canonicalize())
        .map_err(|error| BuildError::input("project directory", error))?;
    boot::ensure_current_principal_owns(&root)
        .map_err(|error| BuildError::input("project directory", error))?;

    let app_source = read_authoring(&root, "keld.config.ts")?;
    let build_source = read_authoring(&root, "keld.build.ts")?;
    let app = config::parse_app(&app_source)?;
    let build = config::parse_build(&build_source)?;
    require_graph_pairing(&app, &build)?;
    let permissions = boot::capture_required_packaging_permissions(&root)
        .map_err(|error| BuildError::input("keld.permissions.jsonc", error))?;
    let manifest = keld_guard::parse_manifest(
        std::str::from_utf8(&permissions)
            .map_err(|error| BuildError::input("keld.permissions.jsonc", error))?,
    )
    .map_err(|error| BuildError::input("keld.permissions.jsonc", error))?;
    require_local_legacy_profile(&manifest)?;

    let graph_capture = select_installed_graph_capture(&app, &build)?;
    let boot = compile_local_boot_bytes(graph_capture.is_some(), &app_source, &app, &permissions)?;
    let metadata = MacosBundleMetadata {
        app_id: app.id,
        name: app.name,
        version: app.version,
        bundle_version: build.bundle_version,
        minimum_system_version: build.minimum_system_version,
        icon: build.icon.is_some(),
        bun_sha256: build.bun.sha256,
    };
    metadata
        .validate()
        .map_err(|error| BuildError::input("Apple bundle metadata", error))?;
    let package = MacosPackageInput {
        project_root: root,
        metadata,
        host: MacosExecutableInput {
            path: build.host.path,
            sha256: build.host.sha256,
            version: build.host.version,
        },
        bun: MacosExecutableInput {
            path: build.bun.path,
            sha256: build.bun.sha256,
            version: build.bun.version,
        },
        entry: app.entry,
        renderer: app.renderer,
        resources: build.resources,
        icon: build.icon,
        boot,
        permissions,
        generated: keld_core::app_session::macos_local_launch_bootstrap_sources()
            .into_iter()
            .map(|(path, bytes)| MacosGeneratedFile {
                path: path.to_owned(),
                bytes: bytes.to_vec(),
            })
            .collect(),
        out_dir: build.out_dir,
    };
    let result = if let Some((selected, expected)) = graph_capture {
        // Pack is the sole copier and exposes its already-copied app-root
        // handle plus verified member receipt to exactly one finalizer.
        // There is no second source-tree capture, dev lease or release grant.
        produce_macos_local_with_graph(&package, |copied_root, root_fd, _copied_members| {
            let graph = keld_core::app_session::make_graph_leaf_from_retained_root(
                copied_root,
                root_fd,
                &expected,
                &selected,
                &package.entry,
            )
            .map_err(|error| BuildError::input("copied graph admission", error))?;
            let boot = compile_installed_graph_boot_descriptor_bytes(
                &app_source,
                &package.metadata.name,
                &package.entry,
                &package.permissions,
                &graph,
                &selected,
            )
            .map_err(|error| BuildError::input("installed boot-6 compiler", error))?;
            Ok::<MacosGraphCompiled, BuildError>(MacosGraphCompiled { graph, boot })
        })
        .map_err(|error| BuildError::input("macOS graph assembly", error))?
    } else {
        keld_pack::produce_macos_local(&package)
            .map_err(|error| BuildError::input("macOS assembly", error))?
    };
    println!("Built app: {}", result.app.display());
    println!("Built DMG: {}", result.dmg.display());
    println!("Build receipt: {}", result.receipt.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_succeeds_without_target_or_project_inputs() {
        for flag in ["--help", "-h"] {
            run_build(&[flag.into()]).expect("build help does not require a project");
        }
        assert!(BUILD_USAGE.contains("--target windows-x64 --local-test-session ABS_JSON"));
        assert!(BUILD_USAGE.contains("developer-local"));
        assert!(BUILD_USAGE.contains("installation, app launch"));
        assert!(BUILD_USAGE.contains("does not build an app"));
    }

    #[test]
    fn windows_initial_install_requires_explicit_closed_invocation() {
        for (flag, create_scaffold) in [
            ("--initial-install-observed-inputs", false),
            ("--initial-install-create-scaffold-inputs", true),
        ] {
            for target in [
                vec!["--target", "windows-x64"],
                vec!["--target=windows-x64"],
            ] {
                let mut args: Vec<String> = target.into_iter().map(str::to_owned).collect();
                args.extend([
                    flag.to_owned(),
                    "D:/owned/install-inputs.json".to_owned(),
                    "--published-at".to_owned(),
                    "2026-11-13T15:00:00Z".to_owned(),
                ]);
                assert_eq!(
                    windows_product_session(&args).unwrap(),
                    Some(WindowsProductRoute::DeveloperInitialInstall {
                        inputs: "D:/owned/install-inputs.json",
                        published_at: "2026-11-13T15:00:00Z",
                        create_scaffold,
                    })
                );
            }
        }
        for rest in [
            vec!["--initial-install-create-scaffold-inputs", ""],
            vec!["--initial-install-observed-inputs", "D:/observed.json"],
            vec![
                "--initial-install-create-scaffold-inputs",
                "D:/observed.json",
                "--published-at",
            ],
            vec![
                "--initial-install-create-scaffold-inputs",
                "D:/observed.json",
                "--published-at",
                "",
            ],
            vec![
                "--initial-install-create-scaffold-inputs",
                "D:/observed.json",
                "--published-at",
                "2026-11-13T15:00:00Z",
                "--sign",
            ],
            vec![
                "--local-test-session",
                "D:/test.json",
                "--initial-install-observed-inputs",
                "D:/observed.json",
            ],
            vec![
                "--initial-install-create-scaffold-inputs",
                "D:/observed.json",
                "--sign",
                "2026-11-13T15:00:00Z",
            ],
        ] {
            let mut args = vec!["--target".to_owned(), "windows-x64".to_owned()];
            args.extend(rest.into_iter().map(str::to_owned));
            assert!(windows_product_session(&args).is_err(), "{args:?}");
        }
        assert_eq!(
            windows_product_session(&["--target=windows-x64".into()]).unwrap(),
            Some(WindowsProductRoute::Developer)
        );
    }

    #[test]
    fn help_does_not_silently_accept_extra_arguments() {
        for args in [
            vec!["--help".into(), "--install".into()],
            vec!["-h".into(), "garbage".into()],
        ] {
            assert_eq!(
                run_build(&args)
                    .expect_err("invalid help invocation")
                    .exit_code(),
                2
            );
        }
    }

    #[test]
    fn local_package_requires_explicit_legacy_policy_at_guard_boundary() {
        for (source, expect) in [
            ("{}", Some("explicit root /profile")),
            (r#"{"profile":"strict"}"#, Some("Strict containment")),
            (r#"{"profile":"legacy"}"#, None),
        ] {
            let parsed = keld_guard::parse_manifest(source).expect("valid policy data");
            match expect {
                Some(fragment) => {
                    let error = require_local_legacy_profile(&parsed)
                        .expect_err("unqualified local profile");
                    assert!(error.to_string().contains(fragment), "{error}");
                }
                None => require_local_legacy_profile(&parsed).expect("conscious Legacy"),
            }
        }
    }

    #[test]
    fn windows_app_check_is_explicit_and_never_an_ordinary_build() {
        assert!(is_windows_app_input_check(&[
            "--target".into(),
            "windows-x64".into(),
            "--check-app-inputs".into()
        ]));
        assert!(is_windows_app_input_check(&[
            "--target=windows-x64".into(),
            "--check-app-inputs".into()
        ]));
        for args in [
            vec!["--target".into(), "windows-x64".into()],
            vec![
                "--target".into(),
                "windows-x64".into(),
                "--check-app-inputs".into(),
                "--sign".into(),
            ],
            vec![
                "--target".into(),
                "macos-arm64".into(),
                "--check-app-inputs".into(),
            ],
        ] {
            assert!(!is_windows_app_input_check(&args));
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn windows_input_check_refuses_foreign_os_before_reading_inputs() {
        let error = run_build(&["--target=windows-x64".into(), "--check-app-inputs".into()])
            .expect_err("native Windows required");
        assert_eq!(error.exit_code(), 2);
        assert!(error.to_string().contains("no inputs were read"));
    }

    #[test]
    fn only_one_concrete_mac_target_is_accepted() {
        assert!(parse_args(&["--target".into(), "macos-arm64".into()]).is_ok());
        assert!(parse_args(&["--target=macos-arm64".into()]).is_ok());
        for args in [
            vec![],
            vec!["macos-arm64".into()],
            vec!["--target".into(), "macos-x64".into()],
            vec!["--target".into(), "windows-x64".into()],
            vec!["--target".into(), "macos-arm64".into(), "--sign".into()],
            vec!["--sign".into(), "adhoc-local".into()],
        ] {
            let err = parse_args(&args).expect_err("unsupported invocation");
            assert_eq!(err.exit_code(), 2);
            assert!(err.to_string().contains("KELD-CLI-052"));
        }
    }

    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    #[test]
    fn unsupported_host_refuses_before_reading_cwd_or_writing_output() {
        let error =
            run_build(&["--target".into(), "macos-arm64".into()]).expect_err("foreign platform");
        assert_eq!(error.exit_code(), 2);
        assert!(error.to_string().contains("Apple Silicon"));
    }
}

#[cfg(test)]
mod windows_product_route_tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| (*s).to_owned()).collect()
    }
    #[test]
    fn ordinary_windows_route_accepts_developer_and_retains_closed_test_mode() {
        for a in [
            args(&[
                "--target",
                "windows-x64",
                "--local-test-session",
                "D:/session.json",
            ]),
            args(&[
                "--target=windows-x64",
                "--local-test-session",
                "D:/session.json",
            ]),
        ] {
            assert_eq!(
                windows_product_session(&a).unwrap(),
                Some(WindowsProductRoute::LocalTest("D:/session.json"))
            );
        }
        for a in [
            args(&["--target", "windows-x64", "--sign", "production"]),
            args(&[
                "--target",
                "windows-x64",
                "--local-test-session",
                "D:/s",
                "--install",
            ]),
        ] {
            assert!(windows_product_session(&a).is_err());
        }
        assert_eq!(
            windows_product_session(&args(&["--target", "windows-x64"])).unwrap(),
            Some(WindowsProductRoute::Developer)
        );
        assert_eq!(
            windows_product_session(&args(&["--target=windows-x64"])).unwrap(),
            Some(WindowsProductRoute::Developer)
        );
        assert_eq!(
            windows_product_session(&args(&["--target", "macos-arm64"])).unwrap(),
            None
        );
        parse_args(&args(&["--target", "macos-arm64"])).unwrap();
    }
}
