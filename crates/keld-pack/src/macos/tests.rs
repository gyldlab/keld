//! Native builder controls. The small compiled binaries exercise packaging only;
//! installed Host/Guardian behavior has independent acceptance owners.
#![allow(clippy::expect_used)] // Test fixtures and OS tool oracles must fail loudly on setup faults.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::{MetadataExt as _, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

use super::capture::{Capture, ReceiptKind, copy_executable, create_0755_directory, digest_file};
use super::{
    MacosExecutableInput, MacosGeneratedFile, MacosPackageInput, ProducedMacosLocal,
    produce_macos_local, validate_macos_executable_metadata,
};
use crate::MacosBundleMetadata;

#[path = "graph_tests.rs"]
mod graph;

fn fixture(root: &Path) -> MacosPackageInput {
    fs::create_dir_all(root.join("assets")).expect("fixture assets");
    fs::write(root.join("assets/main.js"), b"console.log('fixture');\n").expect("fixture entry");
    MacosPackageInput {
        project_root: fs::canonicalize(root).expect("canonical fixture root"),
        metadata: MacosBundleMetadata {
            app_id: "org.example.keld.localfixture".to_owned(),
            name: "Keld Fixture".to_owned(),
            version: "1.2.3".to_owned(),
            bundle_version: "4".to_owned(),
            minimum_system_version: "13.0".to_owned(),
            icon: false,
            bun_sha256: [0; 32],
        },
        host: MacosExecutableInput {
            path: PathBuf::from("host"),
            sha256: [0; 32],
            version: "test-host-1".to_owned(),
        },
        bun: MacosExecutableInput {
            path: PathBuf::from("bun"),
            sha256: [0; 32],
            version: "test-bun-1".to_owned(),
        },
        entry: "assets/main.js".to_owned(),
        renderer: None,
        resources: vec!["assets".to_owned()],
        icon: None,
        boot: br#"{"schema":1}"#.to_vec(),
        permissions: br"{}".to_vec(),
        generated: vec![],
        out_dir: PathBuf::from("output"),
    }
}

fn must_fail_before_publication(input: &MacosPackageInput) {
    let output = input.project_root.join("output");
    let error = produce_macos_local(input).expect_err("invalid build must refuse");
    assert_eq!(error.code(), "KELD-PACK-014", "{error}");
    assert!(!output.exists(), "failed build must not publish: {error}");
}

#[test]
fn rejects_project_symlinks_escaping_paths_and_generated_collisions() {
    let dir = TempDir::new().expect("temporary project");
    let mut request = fixture(dir.path());
    symlink("../outside", request.project_root.join("assets/escape"))
        .expect("malicious resource link");
    assert!(
        Capture::plan(&request).is_err(),
        "symlink traversal must refuse"
    );
    must_fail_before_publication(&request);

    let dir = TempDir::new().expect("temporary project");
    request = fixture(dir.path());
    request.generated = vec![
        MacosGeneratedFile {
            path: "generated/a".into(),
            bytes: vec![1],
        },
        MacosGeneratedFile {
            path: "generated/a/b".into(),
            bytes: vec![2],
        },
    ];
    assert!(
        Capture::plan(&request).is_err(),
        "a generated file cannot also be a parent directory"
    );
    must_fail_before_publication(&request);

    let dir = TempDir::new().expect("temporary project");
    request = fixture(dir.path());
    request.generated.push(MacosGeneratedFile {
        path: "keld.permissions.jsonc".into(),
        bytes: vec![1],
    });
    must_fail_before_publication(&request);

    let dir = TempDir::new().expect("temporary project");
    request = fixture(dir.path());
    request.resources.push("../outside".to_owned());
    must_fail_before_publication(&request);
}

#[test]
fn qualified_stage_graph_leaf_is_generated_only() {
    let dir = TempDir::new().expect("temporary project");
    let mut request = fixture(dir.path());
    fs::write(request.project_root.join("keld.stage-graph.json"), br"{}")
        .expect("unqualified graph-looking resource");
    request.resources = vec!["keld.stage-graph.json".to_owned()];
    assert!(
        Capture::plan(&request).is_err(),
        "a selected authoring resource must not own the generated graph destination"
    );
    must_fail_before_publication(&request);

    request.resources = vec!["assets".to_owned()];
    request.generated.push(MacosGeneratedFile {
        path: "keld.stage-graph.json".to_owned(),
        bytes: br"{}".to_vec(),
    });
    Capture::plan(&request)
        .expect("the qualified graph producer may supply the reserved destination");
}

#[test]
fn resource_directory_modes_survive_umask_subprocess() {
    const CHILD_ROOT: &str = "KELD_PACK_UMASK_CHILD_ROOT";
    if let Some(project) = std::env::var_os(CHILD_ROOT) {
        let project = PathBuf::from(project);
        let request = fixture(&project);
        let nested = request.project_root.join("assets/nested/deep");
        fs::create_dir_all(&nested).expect("owned fixture nested assets");
        fs::write(nested.join("data.json"), br"{}").expect("ordinary nested asset");
        let capture = Capture::plan(&request).expect("captured fixture");
        let destination = project.join("materialized");
        fs::create_dir(&destination).expect("owned resource root");
        fs::set_permissions(&destination, fs::Permissions::from_mode(0o755))
            .expect("resource root caller contract");
        let receipt = capture
            .materialize(&destination)
            .expect("real directory capture");
        for path in ["assets", "assets/nested", "assets/nested/deep"] {
            assert_eq!(
                fs::symlink_metadata(destination.join(path))
                    .expect("materialized directory")
                    .mode()
                    & 0o7777,
                0o755,
                "captured directory mode depends on ambient umask: {path}"
            );
            let entry = receipt
                .iter()
                .find(|entry| entry.path == path)
                .expect("directory row");
            assert_eq!(entry.kind, ReceiptKind::Directory);
            assert_eq!(entry.mode, 0o755);
            assert!(entry.sha256.is_none(), "directories have no file digest");
            assert!(entry.size.is_none(), "directories have no file length");
        }
        return;
    }
    for mask in ["077", "022"] {
        let dir = TempDir::new().expect("owned umask subprocess project");
        let root = fs::canonicalize(dir.path()).expect("canonical subprocess root");
        let mut child = Command::new("/bin/sh");
        child
            .args(["-c", "umask \"$1\"; shift; exec \"$@\"", "sh", mask])
            .arg(std::env::current_exe().expect("current test executable"))
            .args([
                "--exact",
                "macos::tests::resource_directory_modes_survive_umask_subprocess",
                "--nocapture",
            ])
            .env(CHILD_ROOT, &root);
        let output = child.output().expect("native umask subprocess");
        assert!(
            output.status.success(),
            "umask {mask} child failed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        for path in ["assets", "assets/nested", "assets/nested/deep"] {
            assert_eq!(
                fs::symlink_metadata(root.join("materialized").join(path))
                    .expect("subprocess directory")
                    .mode()
                    & 0o7777,
                0o755,
                "umask {mask} leaked into published resource mode"
            );
        }
    }
}

#[test]
fn generated_resources_reject_native_magic_and_addons_but_keep_safe_javascript() {
    for (path, bytes) in [
        ("src/addon.node", b"javascript".as_slice()),
        ("src/macho.bin", b"\xcf\xfa\xed\xfe\x00\x00".as_slice()),
        ("src/elf.bin", b"\x7fELF\x02\x01".as_slice()),
        ("src/program.exe", b"MZ".as_slice()),
    ] {
        let dir = TempDir::new().expect("generated native code project");
        let mut request = fixture(dir.path());
        request.generated.push(MacosGeneratedFile {
            path: path.to_owned(),
            bytes: bytes.to_vec(),
        });
        assert!(
            Capture::plan(&request).is_err(),
            "generated native content bypassed resource classification: {path}"
        );
    }
    let dir = TempDir::new().expect("generated JS project");
    let mut request = fixture(dir.path());
    let javascript = b"export const data = 42;\n".to_vec();
    request.generated.push(MacosGeneratedFile {
        path: "src/generated.js".to_owned(),
        bytes: javascript.clone(),
    });
    let captured = Capture::plan(&request).expect("harmless generated JS");
    let destination = request.project_root.join("captured");
    fs::create_dir(&destination).expect("owned capture root");
    fs::set_permissions(&destination, fs::Permissions::from_mode(0o755))
        .expect("owned capture root mode");
    captured
        .materialize(&destination)
        .expect("JS materialization");
    assert_eq!(
        fs::read(destination.join("src/generated.js")).expect("generated JS bytes"),
        javascript
    );
}

#[test]
fn rejects_conventional_secret_resources_even_beneath_selected_asset_directories() {
    // Direct and recursively discovered paths share the same resource admission
    // owner. These names are conservative guardrails, not a general secrets scan.
    for leaf in [
        ".npmrc",
        ".NETRC",
        ".pypirc",
        "credentials.json",
        "SERVICE-ACCOUNT.JSON",
        "id_rsa",
        "ID_ED25519",
        "server.PEM",
        "client.key",
        "Auth.P8",
        "client.P12",
        "keyStore.PFX",
    ] {
        let dir = TempDir::new().expect("temporary resource project");
        let mut request = fixture(dir.path());
        let nested = request.project_root.join("assets/nested");
        fs::create_dir(&nested).expect("nested assets directory");
        fs::write(nested.join(leaf), b"private credential fixture").expect("credential file");
        assert!(
            Capture::plan(&request).is_err(),
            "directory enumeration admitted conventional credential: {leaf}"
        );
        request.resources = vec![format!("assets/nested/{leaf}")];
        assert!(
            Capture::plan(&request).is_err(),
            "direct selection admitted conventional credential: {leaf}"
        );
    }

    let dir = TempDir::new().expect("positive asset project");
    let request = fixture(dir.path());
    fs::create_dir(request.project_root.join("assets/nested")).expect("nested asset directory");
    fs::write(
        request.project_root.join("assets/nested/data.json"),
        br#"{"type":"asset","value":42}"#,
    )
    .expect("ordinary JSON asset");
    let captured = Capture::plan(&request).expect("ordinary JSON assets must remain supported");
    let dest = request.project_root.join("captured");
    fs::create_dir(&dest).expect("capture destination");
    captured
        .materialize(&dest)
        .expect("ordinary JSON asset materialization");
    assert_eq!(
        fs::read(dest.join("assets/nested/data.json")).expect("materialized JSON"),
        br#"{"type":"asset","value":42}"#
    );
}

#[test]
fn rejects_modified_source_native_resource_and_overlapping_output() {
    let dir = TempDir::new().expect("temporary project");
    let request = fixture(dir.path());
    let capture = Capture::plan(&request).expect("valid capture plan");
    fs::write(
        request.project_root.join("assets/main.js"),
        b"console.log('changed');\n",
    )
    .expect("mutate source after plan");
    let target = request.project_root.join("materialized");
    create_0755_directory(&target).expect("valid destination independent of caller umask");
    assert!(
        capture.materialize(&target).is_err(),
        "capture must not accept post-plan source changes"
    );

    let dir = TempDir::new().expect("temporary project");
    let mut request = fixture(dir.path());
    fs::write(
        request.project_root.join("assets/native.node"),
        b"\xcf\xfa\xed\xfe\x0c\x00\x00\x01",
    )
    .expect("native resource");
    must_fail_before_publication(&request);

    let dir = TempDir::new().expect("temporary project");
    request = fixture(dir.path());
    request.out_dir = PathBuf::from("assets/subdir");
    let capture = Capture::plan(&request).expect("valid capture plan");
    assert!(
        capture
            .reject_output_overlap(&request.project_root.join("assets/subdir"), &request)
            .is_err(),
        "output inside captured resource must refuse"
    );
}

#[test]
fn refuses_preexisting_output_with_real_native_pinned_inputs() {
    let dir = TempDir::new().expect("temporary project");
    let mut request = fixture(dir.path());
    let host = compile_fixture(&request.project_root, "host");
    let bun = compile_fixture(&request.project_root, "bun");
    let mut sign_bun = Command::new("/usr/bin/codesign");
    sign_bun
        .args(["--sign", "-", "--force", "--timestamp=none"])
        .arg(&bun);
    run_tool(sign_bun);
    request.host.sha256 = digest_file(&host).expect("qualified test Host hash");
    request.bun.sha256 = digest_file(&bun).expect("qualified signed test Bun hash");
    request.metadata.bun_sha256 = request.bun.sha256;
    let output = request.project_root.join("output");
    fs::create_dir(&output).expect("existing output");
    fs::write(output.join("keep"), b"must survive").expect("existing output contents");
    let error = produce_macos_local(&request).expect_err("no-clobber contract");
    assert!(error.to_string().contains("already exists"), "{error}");
    assert_eq!(
        fs::read(output.join("keep")).expect("preserved"),
        b"must survive"
    );
    assert!(
        fs::read_dir(&request.project_root)
            .expect("project inventory")
            .all(|entry| !entry
                .expect("directory entry")
                .file_name()
                .to_string_lossy()
                .starts_with(".keld-macos-build-")),
        "no transient published candidate may escape the collision refusal"
    );
}

#[test]
fn rejects_symlinked_nonexecutable_and_wrong_digest_host_inputs() {
    let dir = TempDir::new().expect("temporary project");
    let request = fixture(dir.path());
    let host = request.project_root.join("host");
    fs::write(&host, b"native input fixture").expect("host input");
    let mut executable = MacosExecutableInput {
        path: PathBuf::from("host"),
        sha256: digest_file(&host).expect("input digest"),
        version: "test-host-1".into(),
    };
    assert!(
        copy_executable(
            &request.project_root,
            &executable,
            &request.project_root.join("captured-host"),
        )
        .is_err(),
        "a regular nonexecutable input is forbidden"
    );

    fs::set_permissions(&host, fs::Permissions::from_mode(0o755)).expect("test executable mode");
    executable.sha256 = [0xff; 32];
    assert!(
        copy_executable(
            &request.project_root,
            &executable,
            &request.project_root.join("captured-wrong"),
        )
        .is_err(),
        "a changed pin must fail even for executable bytes"
    );

    symlink("host", request.project_root.join("host-link")).expect("malicious symlink");
    executable.path = PathBuf::from("host-link");
    executable.sha256 = digest_file(&host).expect("correct target digest");
    assert!(
        copy_executable(
            &request.project_root,
            &executable,
            &request.project_root.join("captured-link"),
        )
        .is_err(),
        "pin equality must not authorize a symlinked input"
    );
}

fn run_tool(mut command: Command) {
    let output = command.output().expect("native tool available");
    assert!(
        output.status.success(),
        "native command failed: {:?}; stdout: {}; stderr: {}",
        command,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn compile_fixture(root: &Path, name: &str) -> PathBuf {
    let source = root.join("test-main.c");
    fs::write(&source, b"int main(void) { return 0; }\n").expect("native fixture source");
    let destination = root.join(name);
    let mut clang = Command::new("/usr/bin/xcrun");
    clang
        .args(["clang", "-arch", "arm64", "-mmacosx-version-min=13.0", "-o"])
        .arg(&destination)
        .arg(source);
    run_tool(clang);
    destination
}

#[test]
fn native_signed_bundle_checks_every_directory_under_umask_077() {
    // Invoke the same native app/DMG acceptance mechanics with a restrictive
    // process umask. The test binary itself is the child, not a mocked builder.
    let mut child = Command::new("/bin/sh");
    child
        .args(["-c", "umask 077; exec \"$@\"", "sh"])
        .arg(std::env::current_exe().expect("native libtest executable"))
        .args([
            "--exact",
            "macos::tests::native_assembly_verifies_app_image_copy_and_rejects_mutations",
            "--nocapture",
        ]);
    let output = child.output().expect("native umask077 acceptance process");
    assert!(
        output.status.success(),
        "real macOS signing/DMG failed under umask077: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_assembly_verifies_app_image_copy_and_rejects_mutations() {
    let dir = TempDir::new().expect("temporary native project");
    let mut request = fixture(dir.path());
    let host = compile_fixture(&request.project_root, "host");
    let bun = compile_fixture(&request.project_root, "bun");
    let mut sign_bun = Command::new("/usr/bin/codesign");
    sign_bun
        .args(["--sign", "-", "--force", "--timestamp=none"])
        .arg(&bun);
    run_tool(sign_bun);
    request.host.sha256 = digest_file(&host).expect("fixture Host hash");
    request.bun.sha256 = digest_file(&bun).expect("fixture Bun hash");
    request.metadata.bun_sha256 = request.bun.sha256;
    let original_bun = request.bun.sha256;
    let output = produce_macos_local(&request).expect("real native package, DMG and copy");
    assert_signed_package_receipt(&output, &original_bun);
    assert_copied_app_mode_mutation_controls(dir.path(), &output.app);
    assert_corrupted_image_rejected(dir.path(), &output.dmg);
}

fn assert_signed_package_receipt(output: &ProducedMacosLocal, original_bun: &[u8; 32]) {
    assert!(output.app.is_dir());
    assert!(output.dmg.is_file());
    assert!(output.receipt.is_file());
    let final_bun = output.app.join("Contents/Helpers/bun");
    assert_eq!(
        digest_file(&final_bun).expect("signed Bun hash"),
        *original_bun
    );
    let mut verify_bun = Command::new("/usr/bin/codesign");
    verify_bun.args(["--verify", "--strict"]).arg(&final_bun);
    run_tool(verify_bun);
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(&output.receipt).expect("external receipt"))
            .expect("receipt JSON");
    assert_eq!(receipt["artifactClass"], "local-adhoc");
    assert!(receipt["installedWorkflowQualification"].as_str().is_some());
    assert_eq!(receipt["finalBunSha256"], super::capture::hex(original_bun));
    let inventory = receipt["signedApplicationFiles"]
        .as_array()
        .expect("signed inventory");
    assert_generated_file_modes(output, inventory);
    for member in inventory
        .iter()
        .filter(|member| member["kind"] == "directory")
    {
        assert_eq!(
            member["mode"], 0o755,
            "signed bundle directory has umask-sensitive mode: {}",
            member["path"]
        );
        assert!(
            member.get("sha256").is_none(),
            "a directory has no file digest"
        );
        assert!(
            member.get("size").is_none(),
            "a directory has no file length"
        );
    }
    let directory = inventory
        .iter()
        .find(|item| item["path"] == "Contents/Resources/app/assets")
        .expect("owned nested resource directory row");
    assert_eq!(directory["kind"], "directory");
    assert_eq!(directory["mode"], 0o755);
    assert!(
        directory.get("sha256").is_none(),
        "directory digest must be absent"
    );
    assert!(
        directory.get("size").is_none(),
        "directory file size must be absent"
    );
    let host_entry = inventory
        .iter()
        .find(|item| item["path"] == "Contents/MacOS/keld-host")
        .expect("signed Host entry");
    assert_eq!(host_entry["kind"], "file");
    assert!(host_entry["sha256"].is_string());
    assert!(
        receipt["inputHost"]["version"].is_null(),
        "declared label must not imply qualification"
    );
    assert!(receipt["application"]["version"].is_null());
    assert_eq!(receipt["application"]["declaredVersion"], "1.2.3");
    assert_eq!(receipt["inputHost"]["declaredVersion"], "test-host-1");
    assert_eq!(receipt["inputBun"]["declaredVersion"], "test-bun-1");
}

fn assert_generated_file_modes(output: &ProducedMacosLocal, inventory: &[serde_json::Value]) {
    for path in [
        "Contents/Info.plist",
        "Contents/_CodeSignature/CodeResources",
    ] {
        let member = inventory
            .iter()
            .find(|member| member["path"] == path)
            .expect("generated signed metadata inventory row");
        assert_eq!(member["kind"], "file");
        assert_eq!(
            member["mode"], 0o644,
            "generated metadata mode depends on caller umask: {path}"
        );
        assert_eq!(
            fs::symlink_metadata(output.app.join(path))
                .expect("generated signed metadata")
                .mode()
                & 0o7777,
            0o644,
            "signed metadata must be readable after installation: {path}"
        );
    }
    assert_eq!(
        fs::symlink_metadata(&output.receipt)
            .expect("external artifact receipt")
            .mode()
            & 0o7777,
        0o644,
        "artifact receipt mode depends on caller umask"
    );
}

fn assert_copied_app_mode_mutation_controls(directory: &Path, app: &Path) {
    let altered = directory.join("altered.app");
    let mut copy = Command::new("/usr/bin/ditto");
    copy.arg(app).arg(&altered);
    run_tool(copy);
    let copied_host = altered.join("Contents/MacOS/keld-host");
    fs::set_permissions(&copied_host, fs::Permissions::from_mode(0o644))
        .expect("single mode mutation");
    let mut verify_app = Command::new("/usr/bin/codesign");
    verify_app
        .args(["--verify", "--deep", "--strict"])
        .arg(&altered);
    run_tool(verify_app);
    let metadata = File::open(&copied_host)
        .expect("changed executable")
        .metadata()
        .expect("changed metadata");
    assert!(
        validate_macos_executable_metadata(&metadata).is_err(),
        "mode downgrade must refuse independent of a passing code signature"
    );
    fs::set_permissions(&copied_host, fs::Permissions::from_mode(0o755))
        .expect("restore single Host-mode mutation");
    let copied_assets = altered.join("Contents/Resources/app/assets");
    fs::set_permissions(&copied_assets, fs::Permissions::from_mode(0o700))
        .expect("single nested directory-mode mutation");
    let mut signature_still_valid = Command::new("/usr/bin/codesign");
    signature_still_valid
        .args(["--verify", "--deep", "--strict"])
        .arg(&altered);
    run_tool(signature_still_valid);
    assert!(
        super::assemble::inventory(&altered).is_err(),
        "directory chmod must be detected despite a valid copied application signature"
    );
}

fn assert_corrupted_image_rejected(directory: &Path, dmg: &Path) {
    let corrupt_image = directory.join("corrupt.dmg");
    fs::copy(dmg, &corrupt_image).expect("disposable image");
    let mut image = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&corrupt_image)
        .expect("disposable corrupted image");
    let length = image.metadata().expect("size").len();
    assert!(length > 4096, "a real disk image is required");
    // UDZO carries its required 512-byte UDIF trailer at EOF. A midpoint
    // bit flip can hit an unchecked extent; remove known container data instead.
    let trailer_offset = length - 512;
    image
        .seek(SeekFrom::Start(trailer_offset))
        .expect("select UDIF trailer");
    let mut trailer_header = [0_u8; 12];
    image
        .read_exact(&mut trailer_header)
        .expect("UDIF trailer header");
    assert_eq!(
        &trailer_header[..4],
        b"koly",
        "required UDIF trailer signature"
    );
    assert_eq!(&trailer_header[4..8], &4_u32.to_be_bytes(), "UDIF version");
    assert_eq!(
        &trailer_header[8..12],
        &512_u32.to_be_bytes(),
        "UDIF trailer size"
    );
    image
        .set_len(trailer_offset)
        .expect("remove the required UDIF trailer");
    image.sync_all().expect("flush mutation");
    assert_eq!(
        image.metadata().expect("truncated size").len(),
        trailer_offset
    );
    drop(image);
    let output = Command::new("/usr/bin/hdiutil")
        .args(["verify", "-nocache"])
        .arg(&corrupt_image)
        .output()
        .expect("hdiutil verify available");
    assert!(
        !output.status.success(),
        "compressed image without its required UDIF trailer must fail integrity verification: status={}, stdout={}, stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
