//! macOS project-policy capture, distinct from host parsing/authorization.

use std::fs;
use std::io::{self, Read};
use std::os::unix::fs::{FileTypeExt as _, MetadataExt as _, PermissionsExt as _, symlink};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use keld_guard::{ManifestError, read_manifest_bytes};
use sha2::{Digest as _, Sha256};

use super::tests::fixture;
use super::{
    BootCompileError, PERMISSIONS_FILE, ProjectOwnershipError, ProjectPermissionsSource,
    ensure_unix_metadata_owned_by, stage_dev_boot,
};

fn assert_failed_stage_is_empty(project: &Path) {
    let root = project.join(".keld/dev");
    assert_eq!(
        fs::read_dir(root)
            .expect("stage parent survives failed launch")
            .count(),
        0,
        "policy failure leaked the partial launch directory"
    );
}

fn manifest_ceiling() -> usize {
    // A finite existing frame-cap witness also terminates if the manifest
    // bound is accidentally removed; the ceiling itself stays guard-owned.
    let witness =
        io::repeat(0).take(u64::try_from(keld_ipc::MAX_FRAME_LEN).expect("frame cap fits u64"));
    match read_manifest_bytes(witness, Path::new(PERMISSIONS_FILE))
        .expect_err("frame-sized reader must hit the guard-owned bound")
    {
        ManifestError::TooLarge { max_bytes, .. } => max_bytes,
        other => panic!("expected guard byte ceiling, got {other}"),
    }
}

#[test]
fn project_policy_bytes_and_digest_are_staged_verbatim() {
    let (_temp, project, host) = fixture();
    let bytes =
        b"/* explicit project policy */\n{\"app\":{\"fs\":{\"read\":[\"/tmp/keld-policy/**\"]}}}\n";
    fs::write(project.join(PERMISSIONS_FILE), bytes).expect("project policy");
    let stage = stage_dev_boot(&project, &host).expect("stage explicit policy");
    let staged_path = stage.root().join(PERMISSIONS_FILE);
    assert_eq!(fs::read(&staged_path).expect("staged policy"), bytes);
    assert_eq!(
        fs::metadata(&staged_path)
            .expect("staged metadata")
            .permissions()
            .mode()
            & 0o777,
        0o400
    );
    let descriptor: serde_json::Value = serde_json::from_slice(
        &fs::read(stage.root().join("keld.boot.json")).expect("descriptor bytes"),
    )
    .expect("descriptor JSON");
    assert_eq!(descriptor["permissions"]["file"], PERMISSIONS_FILE);
    assert_eq!(
        descriptor["permissions"]["content_sha256"],
        format!("sha256:{:x}", Sha256::digest(bytes))
    );
    fs::write(project.join(PERMISSIONS_FILE), b"{}\n").expect("later project change");
    assert_eq!(
        fs::read(staged_path).expect("immutable staged capture"),
        bytes
    );
}

#[test]
fn empty_invalid_and_exact_ceiling_policy_are_captured_without_a_cli_parser() {
    for bytes in [
        Vec::new(),
        vec![0xff, 0x00, b'{'],
        vec![0xff; manifest_ceiling()],
    ] {
        let (_temp, project, host) = fixture();
        fs::write(project.join(PERMISSIONS_FILE), &bytes).expect("unparsed project input");
        let stage = stage_dev_boot(&project, &host).expect("capture precedes host parsing");
        assert_eq!(
            fs::read(stage.root().join(PERMISSIONS_FILE)).expect("capture"),
            bytes
        );
        let descriptor: serde_json::Value = serde_json::from_slice(
            &fs::read(stage.root().join("keld.boot.json")).expect("descriptor"),
        )
        .expect("descriptor JSON");
        assert_eq!(
            descriptor["permissions"]["content_sha256"],
            format!("sha256:{:x}", Sha256::digest(&bytes))
        );
    }
}

#[test]
fn selected_leaf_survives_path_replacement_with_an_outside_symlink() {
    let (temp, project, _host) = fixture();
    let selected = project.join(PERMISSIONS_FILE);
    let outside = temp.path().join("outside-policy");
    let original = b"original selected bytes\n";
    fs::write(&selected, original).expect("selected leaf");
    fs::write(&outside, b"replacement outside bytes\n").expect("outside leaf");
    let source = ProjectPermissionsSource::open(&project).expect("retain selected source");
    fs::rename(&selected, project.join("retired-policy")).expect("retire pathname");
    symlink(&outside, &selected).expect("replace pathname with outside symlink");
    assert_eq!(source.capture().expect("same selected object"), original);
    assert_eq!(
        fs::read(&outside).expect("outside sentinel"),
        b"replacement outside bytes\n"
    );
}

#[test]
fn selected_root_and_leaf_stay_owned_after_root_path_replacement() {
    let (_temp, project, _host) = fixture();
    fs::write(project.join(PERMISSIONS_FILE), b"selected root bytes\n").expect("source");
    let source = ProjectPermissionsSource::open(&project).expect("retain root and leaf");
    let original = source.root.metadata().expect("retained root metadata");
    let retired = project.with_file_name("retired-project");
    fs::rename(&project, &retired).expect("retire root pathname");
    fs::create_dir(&project).expect("replacement root");
    fs::write(project.join(PERMISSIONS_FILE), b"different root bytes\n").expect("replacement");
    let held = source.root.metadata().expect("still-owned root");
    assert_eq!((held.dev(), held.ino()), (original.dev(), original.ino()));
    let replaced = fs::metadata(&project).expect("replacement metadata");
    assert_ne!((held.dev(), held.ino()), (replaced.dev(), replaced.ino()));
    assert_eq!(
        source.capture().expect("retained capture"),
        b"selected root bytes\n"
    );
}

#[test]
fn policy_source_descriptors_are_close_on_exec_and_leaf_open_is_nonblocking() {
    use rustix::fs::{OFlags, fcntl_getfl};
    use rustix::io::{FdFlags, fcntl_getfd};

    let (_temp, project, _host) = fixture();
    fs::write(project.join(PERMISSIONS_FILE), b"{}\n").expect("policy source");
    let source = ProjectPermissionsSource::open(&project).expect("selected descriptors");
    let file = source.policy.as_ref().expect("present policy descriptor");
    assert!(
        fcntl_getfd(&source.root)
            .expect("root flags")
            .contains(FdFlags::CLOEXEC)
    );
    assert!(
        fcntl_getfd(file)
            .expect("leaf flags")
            .contains(FdFlags::CLOEXEC)
    );
    assert!(
        fcntl_getfl(file)
            .expect("leaf open flags")
            .contains(OFlags::NONBLOCK)
    );
}

#[test]
fn policy_source_root_symlink_is_not_followed() {
    let (temp, project, _host) = fixture();
    let root_alias = temp.path().join("root-alias");
    symlink(&project, &root_alias).expect("root alias fixture");
    let error =
        ProjectPermissionsSource::open(&root_alias).expect_err("selected root may not be a link");
    assert!(
        error.to_string().contains("permissions source root"),
        "{error}"
    );
    assert!(error.to_string().contains("KELD-CLI-047"), "{error}");
}

#[test]
fn present_symlink_directory_and_unreadable_sources_fail_and_clean_stage() {
    assert_ne!(
        rustix::process::geteuid().as_raw(),
        0,
        "permission oracle requires non-root uid"
    );
    for kind in ["symlink", "dangling-symlink", "directory", "unreadable"] {
        let (temp, project, host) = fixture();
        let path = project.join(PERMISSIONS_FILE);
        match kind {
            "symlink" => {
                let outside = temp.path().join("outside-policy");
                fs::write(&outside, b"{}\n").expect("outside source");
                symlink(outside, &path).expect("unsafe source link");
            }
            "dangling-symlink" => {
                symlink(temp.path().join("absent"), &path).expect("dangling link");
            }
            "directory" => fs::create_dir(&path).expect("directory source"),
            "unreadable" => {
                fs::write(&path, b"{}\n").expect("unreadable source");
                fs::set_permissions(&path, fs::Permissions::from_mode(0o000))
                    .expect("unreadable mode");
            }
            _ => panic!("unknown fixture kind"),
        }
        let error =
            stage_dev_boot(&project, &host).expect_err("unsafe present input is not fallback");
        assert!(
            error.to_string().contains("KELD-CLI-047"),
            "{kind}: {error}"
        );
        assert!(
            error.to_string().contains("permissions source"),
            "{kind}: {error}"
        );
        assert_failed_stage_is_empty(&project);
    }
}

#[test]
fn oversized_source_preserves_guard_detail_and_fix_in_staging_error() {
    let (_temp, project, host) = fixture();
    let path = project.join(PERMISSIONS_FILE);
    let bytes = vec![b' '; manifest_ceiling() + 1];
    fs::write(&path, &bytes).expect("oversized source");
    let selected_path = path
        .canonicalize()
        .expect("producer diagnostics use canonical project root");
    let expected =
        read_manifest_bytes(bytes.as_slice(), &selected_path).expect_err("owned guard limit");
    let error = stage_dev_boot(&project, &host).expect_err("oversize cannot become default policy");
    assert!(error.to_string().contains("KELD-CLI-047"), "{error}");
    assert!(error.to_string().contains(&expected.to_string()), "{error}");
    assert_failed_stage_is_empty(&project);
}

// CI-state proof of callback wiring; the real foreign-inode case below owns
// native leaf-owner evidence.
#[test]
fn retained_leaf_owner_check_cannot_be_skipped_after_root_check() {
    let (_temp, project, _host) = fixture();
    let path = project.join(PERMISSIONS_FILE);
    fs::write(&path, b"owned fixture\n").expect("source");
    let mut leaf_checked = false;
    let error = ProjectPermissionsSource::open_with_owner_check(&project, |selected, metadata| {
        if selected == path {
            leaf_checked = true;
            assert!(
                metadata.is_file(),
                "owner check receives selected leaf metadata"
            );
            Err(ProjectOwnershipError::foreign(selected.to_owned()))
        } else {
            ensure_unix_metadata_owned_by(selected, metadata, rustix::process::geteuid().as_raw())
        }
    })
    .expect_err("a root owner check is not a leaf owner check");
    assert!(
        leaf_checked,
        "the actual opened leaf must reach ownership validation"
    );
    assert!(error.to_string().contains("KELD-CLI-049"), "{error}");
    assert!(error.to_string().contains(PERMISSIONS_FILE), "{error}");
}

#[test]
fn real_foreign_owned_policy_leaf_is_rejected_and_partial_stage_is_removed() {
    let (_temp, project, host) = fixture();
    // This public protocol table supplies only an existing foreign-owned
    // inode. Never read or change its bytes, owner or mode; fixture cleanup
    // removes only the extra link beneath the owned project directory.
    let public_source = Path::new("/private/etc/protocols");
    let original = fs::metadata(public_source)
        .expect("real foreign-owner prerequisite: public macOS protocol table metadata");
    assert!(
        original.is_file(),
        "foreign-owner prerequisite must be a regular file"
    );
    let invoking_uid = rustix::process::geteuid().as_raw();
    assert_ne!(
        original.uid(),
        invoking_uid,
        "foreign-owner prerequisite must differ from invoking uid"
    );
    let selected = project.join(PERMISSIONS_FILE);
    fs::hard_link(public_source, &selected)
        .expect("real foreign-owner prerequisite: owned same-volume link must be available");
    let linked = fs::symlink_metadata(&selected).expect("owned foreign-inode link metadata");
    assert!(
        linked.is_file(),
        "selected fixture must remain a regular file"
    );
    assert_eq!(
        (linked.dev(), linked.ino()),
        (original.dev(), original.ino())
    );
    assert_eq!(linked.uid(), original.uid());
    assert_ne!(linked.uid(), invoking_uid);

    // Check acquisition before invoking the whole producer, so deleting its
    // leaf-owner check fails this test without ever reading the public inode.
    let source_error = ProjectPermissionsSource::open(&project)
        .expect_err("real foreign-owned leaf must fail before any byte capture");
    assert!(
        matches!(&source_error, BootCompileError::ProjectOwnership(_)),
        "{source_error}"
    );
    assert!(
        source_error.to_string().contains(PERMISSIONS_FILE),
        "{source_error}"
    );

    let error = stage_dev_boot(&project, &host)
        .expect_err("real foreign-owned policy cannot be captured or staged");
    assert!(
        matches!(&error, BootCompileError::ProjectOwnership(_)),
        "{error}"
    );
    assert!(error.to_string().contains("KELD-CLI-049"), "{error}");
    assert!(error.to_string().contains(PERMISSIONS_FILE), "{error}");
    assert_failed_stage_is_empty(&project);
}

// CI-state proof of the shared uid predicate, not a native foreign-owner run.
#[test]
fn unix_owner_predicate_rejects_a_different_uid_for_retained_metadata() {
    let (_temp, project, _host) = fixture();
    let metadata = fs::metadata(&project).expect("real root metadata");
    let foreign = metadata
        .uid()
        .checked_add(1)
        .unwrap_or_else(|| metadata.uid() - 1);
    let error =
        ensure_unix_metadata_owned_by(&project, &metadata, foreign).expect_err("foreign uid");
    assert!(error.to_string().contains("KELD-CLI-049"), "{error}");
}

const FIFO_CHILD: &str = "boot::permissions_tests::fifo_source_capture_child";
const FIFO_PROJECT: &str = "KELD_POLICY_FIFO_PROJECT";
const FIFO_MARKER: &str = "KELD_POLICY_FIFO_REJECTED";

#[test]
fn fifo_source_is_rejected_without_waiting_for_a_writer() {
    let (_temp, project, _host) = fixture();
    // The pinned rustix mkfifoat binding is unavailable on macOS. Create the
    // same actual FIFO with the system tool, solely at this owned fixture leaf.
    let fifo = Command::new("/usr/bin/mkfifo")
        .args(["-m", "600"])
        .arg(project.join(PERMISSIONS_FILE))
        .output()
        .expect("create real FIFO fixture");
    assert!(
        fifo.status.success(),
        "FIFO fixture creation failed: {}: {}",
        fifo.status,
        String::from_utf8_lossy(&fifo.stderr)
    );
    assert!(
        fs::symlink_metadata(project.join(PERMISSIONS_FILE))
            .expect("FIFO fixture metadata")
            .file_type()
            .is_fifo(),
        "fixture must be an actual FIFO"
    );
    let mut child = Command::new(std::env::current_exe().expect("libtest executable"))
        .args(["--exact", FIFO_CHILD, "--ignored", "--nocapture"])
        .env(FIFO_PROJECT, &project)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("isolated FIFO acquisition child");
    let mut stdout = child.stdout.take().expect("child output");
    let (send, receive) = mpsc::channel();
    let reader = thread::spawn(move || {
        let mut output = String::new();
        let result = stdout.read_to_string(&mut output).map(|_| output);
        let _ = send.send(result);
    });
    let output = match receive.recv_timeout(Duration::from_secs(10)) {
        Ok(result) => result.expect("child output read"),
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            reader
                .join()
                .expect("child output drained after emergency reap");
            panic!("FIFO source acquisition did not finish without a writer: {error}");
        }
    };
    let status = child.wait().expect("FIFO child reaped");
    reader.join().expect("FIFO observer joined");
    assert!(status.success(), "FIFO child failed: {status}: {output}");
    assert!(
        output.contains(FIFO_MARKER),
        "helper was not exercised: {output}"
    );
}

#[test]
#[ignore = "private FIFO child endpoint, selected by the bounded parent"]
fn fifo_source_capture_child() {
    let Some(project) = std::env::var_os(FIFO_PROJECT) else {
        return;
    };
    let error = ProjectPermissionsSource::open(Path::new(&project))
        .expect_err("FIFO is not a regular policy source");
    assert!(error.to_string().contains("KELD-CLI-047"), "{error}");
    assert!(error.to_string().contains("not a regular file"), "{error}");
    println!("{FIFO_MARKER}");
}
