//! One owned native packaging transaction. Tool arguments never pass through a shell.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::time::Duration;

use rustix::fs::{Dir, Mode, OFlags, RenameFlags, openat, renameat_with};
use sha2::Digest as _;

use crate::{
    MACOS_APP_RELATIVE_PATH, MACOS_BUN_RELATIVE_PATH, MACOS_HOST_EXECUTABLE, MACOS_LOCAL_TARGET,
};

use super::capture::{
    Capture, FileReceipt, Identity, copy_executable, create_0755_directory, digest_file,
    digest_retained_file, enforce_0755_directory, executable_mode, hex, open_absolute,
};
use super::tool_output::{ToolOutput, command, plist_json, run, run_with_timeout};
use super::{MacosGraphCompiled, MacosPackError, MacosPackageInput, ProducedMacosLocal};

type NoGraphCompiler =
    fn(&Path, &File, &[FileReceipt]) -> Result<MacosGraphCompiled, MacosPackError>;

struct AppAssembly {
    input_host: FileReceipt,
    input_bun: FileReceipt,
    captured: Vec<FileReceipt>,
    signed: Vec<FileReceipt>,
    tools: BTreeMap<&'static str, ToolOutput>,
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), MacosPackError> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o644)
        .open(path)
        .map_err(|error| MacosPackError::new("generated output", error))?;
    file.write_all(bytes)
        .and_then(|()| file.set_permissions(fs::Permissions::from_mode(0o644)))
        .and_then(|()| file.sync_all())
        .map_err(|error| MacosPackError::new("generated output", error))
}

pub(super) fn produce(input: &MacosPackageInput) -> Result<ProducedMacosLocal, MacosPackError> {
    produce_transaction(input, None::<NoGraphCompiler>)
}

pub(super) fn produce_with_graph<E, F>(
    input: &MacosPackageInput,
    finalize: F,
) -> Result<ProducedMacosLocal, MacosPackError>
where
    E: std::fmt::Display,
    F: FnOnce(&Path, &File, &[FileReceipt]) -> Result<MacosGraphCompiled, E>,
{
    produce_transaction(
        input,
        Some(move |path: &Path, root: &File, members: &[FileReceipt]| {
            finalize(path, root, members)
                .map_err(|error| MacosPackError::new("graph compilation", error))
        }),
    )
}

fn produce_transaction<F>(
    input: &MacosPackageInput,
    finalize: Option<F>,
) -> Result<ProducedMacosLocal, MacosPackError>
where
    F: FnOnce(&Path, &File, &[FileReceipt]) -> Result<MacosGraphCompiled, MacosPackError>,
{
    input
        .metadata
        .validate()
        .map_err(|error| MacosPackError::new("metadata", error))?;
    if input.metadata.bun_sha256 != input.bun.sha256 || input.metadata.icon != input.icon.is_some()
    {
        return Err(MacosPackError::new(
            "metadata",
            "sealed Bun/icon facts do not match the admitted inputs",
        ));
    }
    let app_name = artifact_name(&input.metadata.name, "app")?;
    let dmg_name = artifact_name(&input.metadata.name, "dmg")?;
    let capture = if finalize.is_some() {
        Capture::plan_graph(input)?
    } else {
        Capture::plan(input)?
    };
    let (output, output_parent, output_name) = output_path(&input.project_root, &input.out_dir)?;
    capture.reject_output_overlap(&output, input)?;
    let parent = open_absolute(&output_parent, true)?;
    let parent_identity = parent
        .metadata()
        .map_err(|error| MacosPackError::new("output parent", error))?;
    let staging = tempfile::Builder::new()
        .prefix(".keld-macos-build-")
        .tempdir_in(&output_parent)
        .map_err(|error| MacosPackError::new("owned staging", error))?;
    let publish = staging.path().join("publish");
    let dmg_source = staging.path().join("image-source");
    fs::create_dir(&publish)
        .and_then(|()| fs::create_dir(&dmg_source))
        .map_err(|error| MacosPackError::new("owned staging", error))?;
    let app = dmg_source.join(&app_name);
    let mut assembly = assemble_signed_app(input, &capture, &app, finalize)?;
    symlink("/Applications", dmg_source.join("Applications"))
        .map_err(|error| MacosPackError::new("Applications shortcut", error))?;
    let dmg = publish.join(&dmg_name);
    assembly.tools.insert(
        "dmg-create",
        run(
            command("/usr/bin/hdiutil")
                .args(["create", "-srcfolder"])
                .arg(&dmg_source)
                .args(["-format", "UDZO", "-volname"])
                .arg(&input.metadata.name)
                .arg(&dmg),
            "DMG creation",
        )?,
    );
    assembly.tools.insert(
        "dmg-verify",
        run(
            command("/usr/bin/hdiutil")
                .args(["verify", "-nocache"])
                .arg(&dmg),
            "DMG integrity",
        )?,
    );
    let dmg_round_trip = verify_dmg_copy(
        staging.path(),
        &dmg,
        &app_name,
        &assembly.signed,
        input.bun.sha256,
    )?;
    assembly.tools.extend(dmg_round_trip);
    write_receipt(input, &app, &dmg, &app_name, &assembly, &publish)?;
    fs::rename(&app, publish.join(&app_name))
        .map_err(|error| MacosPackError::new("final bundle staging", error))?;
    let reopened_parent = open_absolute(&output_parent, true)?;
    let current_identity = reopened_parent
        .metadata()
        .map_err(|error| MacosPackError::new("output parent recheck", error))?;
    if (parent_identity.dev(), parent_identity.ino())
        != (current_identity.dev(), current_identity.ino())
    {
        return Err(MacosPackError::new(
            "output parent recheck",
            "destination parent changed during the build",
        ));
    }
    let staging_root = open_absolute(staging.path(), true)?;
    renameat_with(
        &staging_root,
        "publish",
        &parent,
        &output_name,
        RenameFlags::NOREPLACE,
    )
    .map_err(|error| MacosPackError::new("exclusive atomic publication", error))?;
    Ok(ProducedMacosLocal {
        app: output.join(app_name),
        dmg: output.join(dmg_name),
        receipt: output.join("artifact-receipt.json"),
        directory: output,
    })
}

/// Assemble and sign one app candidate; the returned inventory reflects bytes
/// and modes observed after the outer signature, never the unsigned inputs.
fn assemble_signed_app<F>(
    input: &MacosPackageInput,
    capture: &Capture,
    app: &Path,
    finalize: Option<F>,
) -> Result<AppAssembly, MacosPackError>
where
    F: FnOnce(&Path, &File, &[FileReceipt]) -> Result<MacosGraphCompiled, MacosPackError>,
{
    for directory in [
        PathBuf::new(),
        PathBuf::from("Contents"),
        PathBuf::from("Contents/MacOS"),
        PathBuf::from("Contents/Helpers"),
        PathBuf::from("Contents/Resources"),
        PathBuf::from(MACOS_APP_RELATIVE_PATH),
    ] {
        let target = app.join(directory);
        create_0755_directory(&target)
            .map_err(|error| MacosPackError::new("bundle layout", error))?;
    }
    let app_root = app.join(MACOS_APP_RELATIVE_PATH);
    let captured = capture.materialize(&app_root)?;
    let (captured, graph_snapshot) = match finalize {
        Some(compile) => {
            let mut snapshot = CopiedTree::capture(&app_root)?;
            if snapshot.observation.members != captured {
                return Err(MacosPackError::new(
                    "copied graph inputs",
                    "copied content differs from the sole capture owner's receipt",
                ));
            }
            let compiled = compile(&app_root, &snapshot.root, &captured)?;
            snapshot.recheck()?;
            snapshot.write_compiled(compiled)?;
            (snapshot.observation.members.clone(), Some(snapshot))
        }
        None => (captured, None),
    };
    if let Some(icon) = &input.icon {
        capture.copy_icon(icon, &app.join("Contents/Resources/app.icns"))?;
    }
    let host_path = app.join("Contents/MacOS").join(MACOS_HOST_EXECUTABLE);
    let bun_path = app.join(MACOS_BUN_RELATIVE_PATH);
    let input_host = copy_executable(&input.project_root, &input.host, &host_path)?;
    let input_bun = copy_executable(&input.project_root, &input.bun, &bun_path)?;
    let mut tools = BTreeMap::new();
    tools.insert(
        "host-build-commands",
        verify_macho(&host_path, &input.metadata.minimum_system_version)?,
    );
    tools.insert(
        "bun-build-commands",
        verify_macho(&bun_path, &input.metadata.minimum_system_version)?,
    );
    let plist = input
        .metadata
        .to_plist()
        .map_err(|error| MacosPackError::new("metadata", error))?;
    let plist_path = app.join("Contents/Info.plist");
    write_new(&plist_path, &plist)?;
    tools.insert(
        "plist",
        run(
            command("/usr/bin/plutil").arg("-lint").arg(&plist_path),
            "Info.plist validation",
        )?,
    );
    if let Some(snapshot) = &graph_snapshot {
        snapshot.recheck()?;
    }
    tools.extend(sign_bundle(input, app, &bun_path)?);
    // Signing may update the Host outside the graph, but never its retained
    // app payload. Detect even byte-identical member replacement during sealing.
    if let Some(snapshot) = &graph_snapshot {
        snapshot.recheck()?;
    }
    // Apple's codesign may create its signature directory and CodeResources
    // under the caller's restrictive umask. Normalize owned metadata through
    // no-follow handles before final signature and copied-inventory verification.
    normalize_bundle_directories(app)?;
    for path in [
        "Contents/Info.plist",
        "Contents/_CodeSignature/CodeResources",
    ] {
        normalize_generated_file_mode(&app.join(path))?;
    }
    tools.insert("outer-verification", verify_app(app)?);
    executable_mode(&host_path)?;
    executable_mode(&bun_path)?;
    if digest_file(&bun_path)? != input.bun.sha256 {
        return Err(MacosPackError::new(
            "post-sign Bun pin",
            "outer signing changed the independently pinned Bun bytes",
        ));
    }
    Ok(AppAssembly {
        input_host,
        input_bun,
        captured,
        signed: inventory(app)?,
        tools,
    })
}

/// Verify already signed nested Bun before signing the containing application.
/// Sign the outer app once, without recursive signing of nested code.
fn sign_bundle(
    input: &MacosPackageInput,
    app: &Path,
    bun_path: &Path,
) -> Result<BTreeMap<&'static str, ToolOutput>, MacosPackError> {
    let mut tools = BTreeMap::new();
    tools.insert(
        "bun-input-signature",
        run(
            command("/usr/bin/codesign")
                .args(["--verify", "--strict", "--verbose=2"])
                .arg(bun_path),
            "nested Bun signature",
        )?,
    );
    tools.insert(
        "bun-signature-facts",
        run(
            command("/usr/bin/codesign")
                .args(["--display", "--verbose=4", "--entitlements", "-"])
                .arg(bun_path),
            "nested Bun signature facts",
        )?,
    );
    tools.insert(
        "outer-sign",
        run(
            command("/usr/bin/codesign")
                .args(["--sign", "-", "--force", "--timestamp=none", "--identifier"])
                .arg(&input.metadata.app_id)
                .arg(app),
            "outer application signing",
        )?,
    );
    let facts = run(
        command("/usr/bin/codesign")
            .args(["--display", "--verbose=4"])
            .arg(app),
        "outer signature facts",
    )?;
    verify_signature_facts(&facts, &input.metadata.app_id)?;
    tools.insert("outer-signature-facts", facts);
    Ok(tools)
}

fn normalize_bundle_directories(root: &Path) -> Result<(), MacosPackError> {
    enforce_0755_directory(root)?;
    for entry in
        fs::read_dir(root).map_err(|error| MacosPackError::new("signed directories", error))?
    {
        let entry = entry.map_err(|error| MacosPackError::new("signed directories", error))?;
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| MacosPackError::new("signed directories", error))?;
        if metadata.is_dir() {
            normalize_bundle_directories(&entry.path())?;
        } else if !metadata.is_file() {
            return Err(MacosPackError::new(
                "signed directories",
                "bundle contains a symlink or special file while normalizing directory modes",
            ));
        }
    }
    Ok(())
}

fn normalize_generated_file_mode(path: &Path) -> Result<(), MacosPackError> {
    let file = open_absolute(path, false)?;
    if !file
        .metadata()
        .map_err(|error| MacosPackError::new("generated metadata mode", error))?
        .is_file()
    {
        return Err(MacosPackError::new(
            "generated metadata mode",
            "generated signed metadata must remain a regular file",
        ));
    }
    file.set_permissions(fs::Permissions::from_mode(0o644))
        .map_err(|error| MacosPackError::new("generated metadata mode", error))?;
    if file
        .metadata()
        .map_err(|error| MacosPackError::new("generated metadata mode", error))?
        .mode()
        & 0o7777
        != 0o644
    {
        return Err(MacosPackError::new(
            "generated metadata mode",
            "generated signed metadata must have exact mode 0644",
        ));
    }
    Ok(())
}

/// An external evidence record: only the installed Core/Guardian seal and
/// same opened inputs may grant launch authority, never this JSON receipt.
fn write_receipt(
    input: &MacosPackageInput,
    app: &Path,
    dmg: &Path,
    app_name: &str,
    assembly: &AppAssembly,
    publish: &Path,
) -> Result<(), MacosPackError> {
    let final_host = hex(&digest_file(
        &app.join("Contents/MacOS").join(MACOS_HOST_EXECUTABLE),
    )?);
    let final_bun = hex(&digest_file(&app.join(MACOS_BUN_RELATIVE_PATH))?);
    let dmg_sha256 = hex(&digest_file(dmg)?);
    let os = run(
        command("/usr/bin/sw_vers").arg("-productVersion"),
        "OS version receipt",
    )?;
    let developer_tools = run(
        command("/usr/bin/xcrun").arg("--version"),
        "developer tools receipt",
    )?;
    let tool_digests = [
        "/usr/bin/codesign",
        "/usr/bin/hdiutil",
        "/usr/bin/plutil",
        "/usr/bin/ditto",
    ]
    .into_iter()
    .map(|path| Ok((path, hex(&digest_file(Path::new(path))?))))
    .collect::<Result<BTreeMap<_, _>, MacosPackError>>()?;
    let receipt = serde_json::json!({
        "schema": 1,
        "artifactClass": "local-adhoc",
        "target": MACOS_LOCAL_TARGET,
        "application": { "id": input.metadata.app_id, "name": input.metadata.name, "declaredVersion": input.metadata.version, "declaredBundleVersion": input.metadata.bundle_version, "declaredMinimumSystemVersion": input.metadata.minimum_system_version },
        "inputHost": { "declaredVersion": input.host.version, "file": assembly.input_host },
        "inputBun": { "declaredVersion": input.bun.version, "file": assembly.input_bun },
        "imagePayload": { "application": app_name, "applicationsShortcut": "/Applications" },
        "capturedApplicationFiles": assembly.captured,
        "signedApplicationFiles": assembly.signed,
        "finalHostSha256": final_host, "finalBunSha256": final_bun,
        "dmgSha256": dmg_sha256,
        "osVersion": os.stdout.trim(), "developerTools": developer_tools.stdout.trim(),
        "toolSha256": tool_digests, "nativeToolResults": assembly.tools,
        "versionQualification": "declared labels only; externally qualified input versions and original app/provider commits require independent evidence",
        "installedWorkflowQualification": "required separately; assembly and copy checks do not establish application behavior"
    });
    let receipt_bytes = serde_json::to_vec_pretty(&receipt)
        .map_err(|error| MacosPackError::new("artifact receipt", error))?;
    write_new(&publish.join("artifact-receipt.json"), &receipt_bytes)
}

fn artifact_name(name: &str, extension: &str) -> Result<String, MacosPackError> {
    if name.len() > 240 || matches!(name, "." | "..") || name.contains(['/', '\\', ':']) {
        return Err(MacosPackError::new(
            "artifact name",
            "application name cannot form a macOS artifact filename",
        ));
    }
    Ok(format!("{name}.{extension}"))
}

fn output_path(
    root: &Path,
    configured: &Path,
) -> Result<(PathBuf, PathBuf, std::ffi::OsString), MacosPackError> {
    let full = root.join(configured);
    if full.components().any(|part| {
        !matches!(
            part,
            std::path::Component::RootDir | std::path::Component::Normal(_)
        )
    }) {
        return Err(MacosPackError::new(
            "output path",
            "output contains a non-normal component",
        ));
    }
    let name = full
        .file_name()
        .ok_or_else(|| MacosPackError::new("output path", "output must name a new directory"))?
        .to_owned();
    let parent = full
        .parent()
        .ok_or_else(|| MacosPackError::new("output path", "output parent is missing"))?;
    let parent = fs::canonicalize(parent).map_err(|error| {
        MacosPackError::new(
            "output path",
            format!("output parent must already exist: {error}"),
        )
    })?;
    let output = parent.join(&name);
    match fs::symlink_metadata(&output) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Ok(_) => {
            return Err(MacosPackError::new(
                "output path",
                "output already exists; refusing overwrite or reuse",
            ));
        }
        Err(error) => return Err(MacosPackError::new("output path", error)),
    }
    Ok((output, parent, name))
}

fn version_tuple(value: &str) -> Result<[u32; 3], MacosPackError> {
    let mut tuple = [0; 3];
    let pieces: Vec<_> = value.split('.').collect();
    if pieces.is_empty() || pieces.len() > tuple.len() {
        return Err(MacosPackError::new("minimum OS", "invalid version tuple"));
    }
    for (slot, piece) in tuple.iter_mut().zip(pieces) {
        *slot = piece
            .parse()
            .map_err(|error| MacosPackError::new("minimum OS", error))?;
    }
    Ok(tuple)
}

fn verify_macho(path: &Path, configured_minimum: &str) -> Result<ToolOutput, MacosPackError> {
    let architectures = run(
        command("/usr/bin/xcrun").args(["lipo", "-archs"]).arg(path),
        "input architecture",
    )?;
    if architectures.stdout.trim() != "arm64" {
        return Err(MacosPackError::new(
            "input architecture",
            "first native cell accepts exactly arm64 Mach-O inputs",
        ));
    }
    let build = run(
        command("/usr/bin/xcrun")
            .args(["vtool", "-show-build"])
            .arg(path),
        "input deployment floor",
    )?;
    let mut platforms = Vec::new();
    let mut minimums = Vec::new();
    for line in build.stdout.lines() {
        let words: Vec<_> = line.split_whitespace().collect();
        if let ["platform", platform] = words.as_slice() {
            platforms.push(*platform);
        }
        if let ["minos", minimum] = words.as_slice() {
            minimums.push(*minimum);
        }
    }
    if platforms.as_slice() != ["MACOS"] || minimums.len() != 1 {
        return Err(MacosPackError::new(
            "input deployment floor",
            "input must declare one macOS LC_BUILD_VERSION floor",
        ));
    }
    if version_tuple(configured_minimum)? < version_tuple(minimums[0])? {
        return Err(MacosPackError::new(
            "input deployment floor",
            "minimumSystemVersion understates a pinned executable requirement",
        ));
    }
    Ok(build)
}

fn verify_app(app: &Path) -> Result<ToolOutput, MacosPackError> {
    run(
        command("/usr/bin/codesign")
            .args(["--verify", "--deep", "--strict", "--verbose=2"])
            .arg(app),
        "full application signature",
    )
}

fn verify_signature_facts(facts: &ToolOutput, app_id: &str) -> Result<(), MacosPackError> {
    let combined = format!("{}\n{}", facts.stdout, facts.stderr);
    let identifiers: Vec<_> = combined
        .lines()
        .filter_map(|line| line.strip_prefix("Identifier="))
        .collect();
    if identifiers.as_slice() != [app_id] || !combined.lines().any(|line| line == "Signature=adhoc")
    {
        return Err(MacosPackError::new(
            "outer signature facts",
            "outer signature must be ad-hoc with the configured canonical identifier",
        ));
    }
    Ok(())
}

pub(super) fn inventory(root: &Path) -> Result<Vec<FileReceipt>, MacosPackError> {
    Ok(CopiedTree::capture(root)?.observation.members)
}

/// One inventory owner supplies both copy comparison and the stronger physical
/// stability proof required around the graph compiler. It never copies data.
#[derive(Debug, PartialEq, Eq)]
struct TreeObservation {
    members: Vec<FileReceipt>,
    identities: BTreeMap<String, Identity>,
}

struct CopiedTree {
    path: PathBuf,
    root: File,
    observation: TreeObservation,
}

impl CopiedTree {
    fn capture(path: &Path) -> Result<Self, MacosPackError> {
        let root = open_absolute(path, true)?;
        let observation = observe_tree(path, &root)?;
        Ok(Self {
            path: path.to_owned(),
            root,
            observation,
        })
    }

    fn recheck(&self) -> Result<(), MacosPackError> {
        if observe_tree(&self.path, &self.root)? != self.observation {
            return Err(MacosPackError::new(
                "copied graph identity",
                "copied root or member identity, membership, bytes or modes changed",
            ));
        }
        Ok(())
    }

    fn write_compiled(&mut self, compiled: MacosGraphCompiled) -> Result<(), MacosPackError> {
        if compiled.graph.is_empty()
            || compiled.graph.len() > 8 * 1024 * 1024
            || compiled.boot.is_empty()
            || compiled.boot.len() > 64 * 1024
        {
            return Err(MacosPackError::new(
                "compiled graph bounds",
                "graph must contain 1..=8 MiB and boot 1..=64 KiB; empty placeholders are forbidden",
            ));
        }
        super::capture::require_inert_generated("keld.stage-graph.json", &compiled.graph)?;
        super::capture::require_inert_generated("keld.boot.json", &compiled.boot)?;
        for (name, bytes) in [
            ("keld.stage-graph.json", compiled.graph),
            ("keld.boot.json", compiled.boot),
        ] {
            let mut file: File = openat(
                &self.root,
                name,
                OFlags::RDWR | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::RUSR | Mode::WUSR,
            )
            .map_err(|error| MacosPackError::new("exclusive graph leaf", error))?
            .into();
            file.write_all(&bytes)
                .and_then(|()| file.set_permissions(fs::Permissions::from_mode(0o644)))
                .and_then(|()| file.sync_all())
                .map_err(|error| MacosPackError::new("compiled graph write", error))?;
            let metadata = file
                .metadata()
                .map_err(|error| MacosPackError::new("compiled graph identity", error))?;
            if metadata.len() != bytes.len() as u64 || metadata.mode() & 0o7777 != 0o644 {
                return Err(MacosPackError::new(
                    "compiled graph identity",
                    "generated leaf length or mode changed",
                ));
            }
            let digest = digest_retained_file(&file)?;
            if digest != <[u8; 32]>::from(sha2::Sha256::digest(&bytes)) {
                return Err(MacosPackError::new(
                    "compiled graph identity",
                    "generated bytes changed while written",
                ));
            }
            self.observation.members.push(FileReceipt::file(
                name.to_owned(),
                bytes.len() as u64,
                0o644,
                hex(&digest),
            ));
            if self
                .observation
                .identities
                .insert(name.to_owned(), Identity::of(&metadata))
                .is_some()
            {
                return Err(MacosPackError::new(
                    "compiled graph identity",
                    "generated authority leaf was already present",
                ));
            }
        }
        // Only these two exclusive root-leaf creates may change directory times.
        // The retained root's inode and mode, and every pre-existing member's
        // complete identity, remain fixed. Do not recapture a replaced tree.
        let current_root = Identity::of(
            &self
                .root
                .metadata()
                .map_err(|error| MacosPackError::new("compiled graph root", error))?,
        );
        let expected_root = self.observation.identities.get_mut(".").ok_or_else(|| {
            MacosPackError::new("compiled graph root", "retained root identity missing")
        })?;
        if !expected_root.same_object_and_mode(&current_root) {
            return Err(MacosPackError::new(
                "compiled graph root",
                "copied root changed during fixed leaf writes",
            ));
        }
        *expected_root = current_root;
        self.observation.members.sort_by(|a, b| a.path.cmp(&b.path));
        self.recheck()
    }
}

fn observe_tree(path: &Path, root: &File) -> Result<TreeObservation, MacosPackError> {
    let before = Identity::of(
        &root
            .metadata()
            .map_err(|error| MacosPackError::new("inventory root", error))?,
    );
    check_root_path(path, &before)?;
    let mut observation = TreeObservation {
        members: Vec::new(),
        identities: BTreeMap::new(),
    };
    let mut bytes = 0_u64;
    observe_member(root, ".", &mut observation, &mut bytes)?;
    if Identity::of(
        &root
            .metadata()
            .map_err(|error| MacosPackError::new("inventory root", error))?,
    ) != before
    {
        return Err(MacosPackError::new(
            "inventory root",
            "root changed while observed",
        ));
    }
    check_root_path(path, &before)?;
    observation.members.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(observation)
}

fn check_root_path(path: &Path, expected: &Identity) -> Result<(), MacosPackError> {
    let reopened = open_absolute(path, true)?;
    if Identity::of(
        &reopened
            .metadata()
            .map_err(|error| MacosPackError::new("inventory root path", error))?,
    ) != *expected
    {
        return Err(MacosPackError::new(
            "inventory root path",
            "path no longer identifies the retained copied root",
        ));
    }
    Ok(())
}

fn inventory_names(file: &File, count: usize) -> Result<Vec<String>, MacosPackError> {
    let mut names = Vec::new();
    for entry in
        Dir::read_from(file).map_err(|error| MacosPackError::new("inventory directory", error))?
    {
        let entry = entry.map_err(|error| MacosPackError::new("inventory directory", error))?;
        let name = entry
            .file_name()
            .to_str()
            .map_err(|error| MacosPackError::new("inventory name", error))?;
        if matches!(name, "." | "..") {
            continue;
        }
        if names.len() + count >= 100_032 {
            return Err(MacosPackError::new(
                "inventory bound",
                "bundle inventory member limit exceeded",
            ));
        }
        names.push(name.to_owned());
    }
    names.sort();
    Ok(names)
}

fn observe_member(
    file: &File,
    relative: &str,
    observed: &mut TreeObservation,
    bytes: &mut u64,
) -> Result<(), MacosPackError> {
    if observed.members.len() >= 100_032 || relative.split('/').count() > 64 {
        return Err(MacosPackError::new(
            "inventory bound",
            "bundle inventory member or depth limit exceeded",
        ));
    }
    if relative != "." {
        super::capture::relative(relative)?;
    }
    let metadata = file
        .metadata()
        .map_err(|error| MacosPackError::new("inventory member", error))?;
    let identity = Identity::of(&metadata);
    if metadata.is_dir() {
        if metadata.mode() & 0o7777 != 0o755 {
            return Err(MacosPackError::new(
                "directory mode",
                "owned bundle directory must have exact mode 0755",
            ));
        }
        observed
            .members
            .push(FileReceipt::directory(relative.to_owned(), 0o755));
        for name in inventory_names(file, observed.members.len())? {
            let child: File = openat(
                file,
                name.as_str(),
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(|error| MacosPackError::new("inventory no-follow open", error))?
            .into();
            let child_name = if relative == "." {
                name
            } else {
                format!("{relative}/{name}")
            };
            observe_member(&child, &child_name, observed, bytes)?;
        }
    } else if metadata.is_file() {
        *bytes = bytes
            .checked_add(metadata.len())
            .filter(|size| *size <= 10 * 1024 * 1024 * 1024)
            .ok_or_else(|| {
                MacosPackError::new("inventory bound", "bundle inventory byte limit exceeded")
            })?;
        observed.members.push(FileReceipt::file(
            relative.to_owned(),
            metadata.len(),
            metadata.mode() & 0o7777,
            hex(&digest_retained_file(file)?),
        ));
    } else {
        return Err(MacosPackError::new(
            "inventory member",
            "bundle contains a symlink or special file",
        ));
    }
    if Identity::of(
        &file
            .metadata()
            .map_err(|error| MacosPackError::new("inventory recheck", error))?,
    ) != identity
    {
        return Err(MacosPackError::new(
            "inventory recheck",
            "member changed while observed",
        ));
    }
    if observed
        .identities
        .insert(relative.to_owned(), identity)
        .is_some()
    {
        return Err(MacosPackError::new(
            "inventory member",
            "duplicate member name",
        ));
    }
    Ok(())
}

struct MountedImage {
    device: String,
    attached: bool,
}

fn valid_disk_device(path: &str) -> bool {
    let Some(rest) = path.strip_prefix("/dev/disk") else {
        return false;
    };
    let disk_digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    let rest = &rest[disk_digits..];
    disk_digits > 0
        && (rest.is_empty()
            || rest.strip_prefix('s').is_some_and(|partition| {
                !partition.is_empty() && partition.bytes().all(|byte| byte.is_ascii_digit())
            }))
}

fn attached_device(entities: &[serde_json::Value]) -> Option<String> {
    let devices: Vec<_> = entities
        .iter()
        .filter_map(|entity| entity.get("dev-entry").and_then(serde_json::Value::as_str))
        .filter(|device| valid_disk_device(device))
        .collect();
    // hdiutil may report a whole image device followed by a mounted partition.
    // Detach the owning image device, never a guessed disk or a mountpoint string.
    devices
        .iter()
        .copied()
        .find(|path| {
            path.strip_prefix("/dev/disk")
                .is_some_and(|suffix| suffix.bytes().all(|byte| byte.is_ascii_digit()))
        })
        .or_else(|| devices.first().copied())
        .map(str::to_owned)
}

fn recover_observed_attachment(dmg: &Path) -> Result<Option<MountedImage>, MacosPackError> {
    let output = run(
        command("/usr/bin/hdiutil").args(["info", "-plist"]),
        "DMG attachment recovery",
    )?;
    let value = plist_json(&output.stdout, "DMG attachment recovery")?;
    let images = value
        .get("images")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| MacosPackError::new("DMG attachment recovery", "missing images array"))?;
    let mut observed = None;
    for image in images {
        if image.get("image-path").and_then(serde_json::Value::as_str) != dmg.to_str() {
            continue;
        }
        let Some(entities) = image
            .get("system-entities")
            .and_then(serde_json::Value::as_array)
        else {
            continue;
        };
        // The image path is unique to this transaction. An attach failure can
        // leave a device with no mounted volume; ownership is still observable
        // through hdiutil's exact backing image identity.
        let device = attached_device(entities).ok_or_else(|| {
            MacosPackError::new(
                "DMG attachment recovery",
                "observed image device is invalid",
            )
        })?;
        if observed.replace(device).is_some() {
            return Err(MacosPackError::new(
                "DMG attachment recovery",
                "multiple matching attached images require manual inspection",
            ));
        }
    }
    Ok(observed.map(|device| MountedImage {
        device,
        attached: true,
    }))
}

fn cleanup_early_attachment(failure: MacosPackError, dmg: &Path) -> MacosPackError {
    match recover_observed_attachment(dmg) {
        Ok(Some(mut observed)) => match observed.detach() {
            Ok(_) => failure,
            Err(error) => MacosPackError::new(
                "DMG cleanup",
                format!("{failure}; observed device detach failed: {error}"),
            ),
        },
        Ok(None) => failure,
        Err(error) => MacosPackError::new(
            "DMG cleanup",
            format!("{failure}; could not establish whether a device remains mounted: {error}"),
        ),
    }
}

impl MountedImage {
    fn detach(&mut self) -> Result<ToolOutput, MacosPackError> {
        let output = run(
            command("/usr/bin/hdiutil").arg("detach").arg(&self.device),
            "DMG exact-device detach",
        )?;
        self.attached = false;
        Ok(output)
    }
}

impl Drop for MountedImage {
    fn drop(&mut self) {
        if self.attached {
            // Error cleanup owns only this observed attachment. The normal path
            // reports detach failures; this best-effort retry cannot block Drop
            // forever or claim that an unconfirmed attachment was removed.
            let _ = run_with_timeout(
                command("/usr/bin/hdiutil").arg("detach").arg(&self.device),
                "DMG bounded error cleanup",
                Duration::from_secs(15),
            );
        }
    }
}

fn verify_dmg_copy(
    staging: &Path,
    dmg: &Path,
    app_name: &str,
    expected: &[FileReceipt],
    bun_digest: [u8; 32],
) -> Result<BTreeMap<&'static str, ToolOutput>, MacosPackError> {
    let mountpoint = staging.join("mounted-image");
    fs::create_dir(&mountpoint).map_err(|error| MacosPackError::new("DMG mountpoint", error))?;
    let attachment = run(
        command("/usr/bin/hdiutil")
            .args([
                "attach",
                "-readonly",
                "-nobrowse",
                "-noautoopen",
                "-plist",
                "-mountpoint",
            ])
            .arg(&mountpoint)
            .arg(dmg),
        "read-only DMG attach",
    )
    .map_err(|error| cleanup_early_attachment(error, dmg))?;
    let value = plist_json(&attachment.stdout, "DMG attachment identity")
        .map_err(|error| cleanup_early_attachment(error, dmg))?;
    let entities = value
        .get("system-entities")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            cleanup_early_attachment(
                MacosPackError::new("DMG attachment identity", "missing system entities"),
                dmg,
            )
        })?;
    let device = attached_device(entities).ok_or_else(|| {
        cleanup_early_attachment(
            MacosPackError::new("DMG attachment identity", "no valid observed device"),
            dmg,
        )
    })?;
    let mut mounted_image = MountedImage {
        device,
        attached: true,
    };
    let checks = (|| {
        let status = verify_readonly_mount(entities, &mountpoint, &mounted_image.device)?;
        let (ditto, signature) =
            verify_mounted_app(staging, &mountpoint, app_name, expected, bun_digest)?;
        Ok(BTreeMap::from([
            ("dmg-attach", attachment),
            ("dmg-mount-facts", status),
            ("dmg-ditto", ditto),
            ("copied-app-signature", signature),
        ]))
    })();
    let detached = mounted_image.detach();
    match (checks, detached) {
        (Ok(mut outputs), Ok(detach)) => {
            outputs.insert("dmg-detach", detach);
            Ok(outputs)
        }
        (Err(check), Ok(_)) => Err(check),
        (Ok(_), Err(detach)) => Err(detach),
        (Err(check), Err(detach)) => Err(MacosPackError::new(
            "DMG cleanup",
            format!("{check}; observed-device detach also failed: {detach}"),
        )),
    }
}

fn verify_readonly_mount(
    entities: &[serde_json::Value],
    mountpoint: &Path,
    device: &str,
) -> Result<ToolOutput, MacosPackError> {
    let mounts: Vec<_> = entities
        .iter()
        .filter_map(|entry| entry.get("mount-point").and_then(serde_json::Value::as_str))
        .collect();
    if mounts.as_slice()
        != [mountpoint.to_str().ok_or_else(|| {
            MacosPackError::new("DMG attachment identity", "non-UTF-8 mountpoint")
        })?]
    {
        return Err(MacosPackError::new(
            "DMG attachment identity",
            "expected exactly one mount at this transaction's mountpoint",
        ));
    }
    let status = run(
        command("/usr/sbin/diskutil")
            .args(["info", "-plist"])
            .arg(device),
        "read-only mount facts",
    )?;
    let facts = plist_json(&status.stdout, "read-only mount facts")?;
    if facts.get("Writable").and_then(serde_json::Value::as_bool) != Some(false) {
        return Err(MacosPackError::new(
            "read-only mount facts",
            "attached filesystem is not confirmed read-only",
        ));
    }
    Ok(status)
}

/// Compare a copied application with the finalized signed source through an
/// independent filesystem inventory and an Apple signature check.
fn verify_mounted_app(
    staging: &Path,
    mountpoint: &Path,
    app_name: &str,
    expected: &[FileReceipt],
    bun_digest: [u8; 32],
) -> Result<(ToolOutput, ToolOutput), MacosPackError> {
    let mut names = fs::read_dir(mountpoint)
        .map_err(|error| MacosPackError::new("DMG contents", error))?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| MacosPackError::new("DMG contents", error))?;
    names.sort();
    // HFS metadata leaves are native container bookkeeping, not payload inputs.
    names.retain(|name| {
        !matches!(
            name.to_str(),
            Some(".fseventsd" | ".Trashes" | ".Spotlight-V100")
        )
    });
    let mut expected_names = vec![
        OsStr::new(app_name).to_owned(),
        OsStr::new("Applications").to_owned(),
    ];
    expected_names.sort();
    if names != expected_names
        || fs::read_link(mountpoint.join("Applications"))
            .map_err(|error| MacosPackError::new("Applications shortcut", error))?
            != Path::new("/Applications")
    {
        return Err(MacosPackError::new(
            "DMG contents",
            "image payload or Applications shortcut differs from the finalized app set",
        ));
    }
    let copied = staging.join("copied-app").join(app_name);
    fs::create_dir(
        copied
            .parent()
            .ok_or_else(|| MacosPackError::new("DMG copy", "copy parent missing"))?,
    )
    .map_err(|error| MacosPackError::new("DMG copy", error))?;
    let ditto = run(
        command("/usr/bin/ditto")
            .arg(mountpoint.join(app_name))
            .arg(&copied),
        "mounted application copy",
    )?;
    let signature = verify_app(&copied)?;
    executable_mode(&copied.join("Contents/MacOS").join(MACOS_HOST_EXECUTABLE))?;
    executable_mode(&copied.join(MACOS_BUN_RELATIVE_PATH))?;
    if digest_file(&copied.join(MACOS_BUN_RELATIVE_PATH))? != bun_digest
        || inventory(&copied)? != expected
    {
        return Err(MacosPackError::new(
            "DMG copy inventory",
            "copied application bytes or modes differ from the finalized signed app",
        ));
    }
    Ok((ditto, signature))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{ToolOutput, attached_device, valid_disk_device, verify_signature_facts};

    #[test]
    fn attached_image_detach_identity_is_observed_and_does_not_accept_commands() {
        for valid in ["/dev/disk0", "/dev/disk123", "/dev/disk123s5"] {
            assert!(valid_disk_device(valid), "real device syntax: {valid}");
        }
        for hostile in [
            "/Volumes/Fixture",
            "/dev/disk",
            "/dev/disk0s",
            "/dev/disk0s2/extra",
            "/dev/disk0;echo x",
            "/dev/disk0\n",
            "/dev/rdisk3",
        ] {
            assert!(!valid_disk_device(hostile), "invalid device: {hostile}");
        }
        let entities = vec![
            json!({"dev-entry": "/dev/disk123s1", "mount-point": "/private/tmp/owned"}),
            json!({"dev-entry": "/dev/disk123", "content-hint": "GUID_partition_scheme"}),
        ];
        assert_eq!(attached_device(&entities).as_deref(), Some("/dev/disk123"));
    }

    #[test]
    fn outer_signing_facts_require_exact_adhoc_identifier() {
        let facts = ToolOutput {
            stdout: String::new(),
            stderr: "Identifier=org.example.test\nSignature=adhoc\n".to_owned(),
        };
        assert!(verify_signature_facts(&facts, "org.example.test").is_ok());
        assert!(verify_signature_facts(&facts, "org.example.other").is_err());
        let developer_id = ToolOutput {
            stderr: facts
                .stderr
                .replace("Signature=adhoc", "Authority=Developer ID"),
            ..facts
        };
        assert!(verify_signature_facts(&developer_id, "org.example.test").is_err());
    }
}
