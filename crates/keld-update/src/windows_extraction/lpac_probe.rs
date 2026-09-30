//! Shared real-LPAC fixture, denial probe and independent parent observations.

#![allow(unsafe_code)] // Test-only Win32 attribute/DACL denial probes; local proofs below.
#![deny(unsafe_op_in_unsafe_fn)]

use std::env;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read as _, Seek as _, SeekFrom};
use std::os::windows::ffi::OsStrExt as _;
use std::os::windows::fs::{MetadataExt as _, OpenOptionsExt as _, symlink_file};
use std::os::windows::io::AsHandle as _;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use keld_runtime::windows_lpac::{WindowsLpacPathAccess, WindowsLpacProfile, WindowsLpacStdio};
use windows_sys::Win32::Security::Authorization::{SE_FILE_OBJECT, SetNamedSecurityInfoW};
use windows_sys::Win32::Security::DACL_SECURITY_INFORMATION;
use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_READONLY, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, SetFileAttributesW, WRITE_DAC,
    WRITE_OWNER,
};

const LPAC_HELPER_ENV: &str = "KELD_265_EXTRACTION_LPAC_HELPER";
const LPAC_HELPER_TEST: &str = "windows_extraction::tests::lpac_stage_mutation_child";

pub(super) fn mutation_child() {
    if env::var(LPAC_HELPER_ENV).as_deref() != Ok("probe") {
        return;
    }
    let stage = PathBuf::from(env::var_os("KELD_265_STAGE").expect("host-provided stage path"));
    let private = PathBuf::from(env::var_os("KELD_265_PRIVATE").expect("role-private path"));
    let private_file = private.join("allowed.txt");
    fs::write(&private_file, b"role-owned").expect("granted role-private write");
    let mut private_permissions = fs::metadata(&private_file)
        .expect("role-private metadata")
        .permissions();
    private_permissions.set_readonly(true);
    fs::set_permissions(&private_file, private_permissions)
        .expect("granted role-private attribute write");

    let create_denied = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(stage.join("tree/new.txt"))
        .is_err();
    let rename_denied = fs::rename(&private_file, stage.join("tree/renamed.txt")).is_err();
    let reparse_denied = symlink_file(&private_file, stage.join("tree/reparse.txt")).is_err();
    let victim = stage.join("tree/nest/one");
    let wide: Vec<u16> = victim
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: `wide` is a live NUL-terminated UTF-16 path and the attribute
    // constant is a Win32 value. The host checks the original attribute afterward.
    let attribute_denied = unsafe { SetFileAttributesW(wide.as_ptr(), FILE_ATTRIBUTE_READONLY) }
        == 0
        && std::io::Error::last_os_error().raw_os_error() == Some(5);
    // SAFETY: `wide` is a live NUL-terminated UTF-16 path; this hostile call passes
    // no pointers to caller-owned security objects. Success would be caught below
    // and the entire fixture is inside a disposable private temporary tree.
    // This denial proves the protected stage DACL did not change. RolePrivate
    // does not grant WRITE_DAC either, so it is not an operation-matched proof
    // that LPAC could edit some other ACL. The host rechecks the exact stage ACL.
    let acl_status = unsafe {
        SetNamedSecurityInfoW(
            wide.as_ptr().cast_mut(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    let acl_denied = acl_status == 5;
    println!(
        "KELD_265_LPAC private_create=true private_attribute=true create_denied={create_denied} \
         rename_denied={rename_denied} reparse_denied={reparse_denied} \
         attribute_denied={attribute_denied} acl_denied={acl_denied}"
    );
    assert!(create_denied && rename_denied && reparse_denied);
    assert!(attribute_denied && acl_denied);
    if let Some(install) = env::var_os("KELD_266_INSTALL_ROOT") {
        probe_committed_records(Path::new(&install));
    }
}

fn probe_committed_records(install: &Path) {
    for relative in [
        "install-provenance",
        "updates/activation.lock",
        "updates/version-floor",
        "updates/current",
        "updates/last-known-good",
        "updates/versions/1.0.0/.complete",
        "updates/versions/1.0.0/content.tar",
        "updates/versions/1.0.0/tree/nest/one",
    ] {
        let target = install.join(relative);
        let write = OpenOptions::new().write(true).open(&target);
        assert_eq!(
            write
                .expect_err("LPAC cannot open committed data for writing")
                .raw_os_error(),
            Some(5),
            "{relative}"
        );
        for right in [DELETE, WRITE_DAC, WRITE_OWNER] {
            let access = OpenOptions::new()
                .access_mode(right)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .open(&target);
            assert_eq!(
                access
                    .expect_err("LPAC cannot acquire committed-state mutation rights")
                    .raw_os_error(),
                Some(5),
                "{relative}: {right:#x}"
            );
        }
    }
    for parent in [install.to_path_buf(), install.join("updates")] {
        let create = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(parent.join("lpac-new"));
        assert_eq!(
            create
                .expect_err("LPAC cannot add committed-state entries")
                .raw_os_error(),
            Some(5)
        );
    }
    // RolePrivate positively proves data/attribute mutation; it does not grant
    // DELETE or WRITE_DAC. Those requested rights are denial-only observations.
    println!(
        "KELD_266_LPAC_BASELINE protected_files=8 write_denied=true delete_access_denied=true dac_denied=true owner_denied=true create_denied=true"
    );
}

pub(crate) fn run_lpac_probe(temp: &Path, stage_path: &Path, install: Option<&Path>) -> String {
    let runtime = temp.join("lpac-runtime");
    let private = temp.join("lpac-private");
    fs::create_dir(&runtime).expect("runtime ACL fixture");
    fs::create_dir(&private).expect("role-private ACL fixture");
    let program = runtime.join("lpac-extraction-probe.exe");
    fs::copy(env::current_exe().expect("test executable"), &program)
        .expect("copy real subprocess fixture");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let profile_name = format!("keld-265-{}-{nonce}", std::process::id());
    let profile = WindowsLpacProfile::create(OsStr::new(&profile_name))
        .expect("fresh zero-capability LPAC profile");
    profile
        .grant_path(temp, WindowsLpacPathAccess::Traverse)
        .expect("fixture ancestor traversal only");
    profile
        .grant_path(&runtime, WindowsLpacPathAccess::ReadExecute)
        .expect("runtime executable ACL");
    profile
        .grant_path(&private, WindowsLpacPathAccess::RolePrivate)
        .expect("role-private control ACL");

    let mut output = tempfile::tempfile_in(&private).expect("captured LPAC output");
    let input = File::open("NUL").expect("null LPAC stdin");
    let mut environment = vec![
        (OsString::from(LPAC_HELPER_ENV), OsString::from("probe")),
        (
            OsString::from("KELD_265_STAGE"),
            stage_path.as_os_str().to_owned(),
        ),
        (
            OsString::from("KELD_265_PRIVATE"),
            private.clone().into_os_string(),
        ),
        (OsString::from("TEMP"), private.clone().into_os_string()),
        (OsString::from("TMP"), private.clone().into_os_string()),
    ];
    if let Some(install) = install {
        environment.push((
            OsString::from("KELD_266_INSTALL_ROOT"),
            install.as_os_str().to_owned(),
        ));
    }
    for key in ["SystemRoot", "WINDIR", "USERPROFILE", "LOCALAPPDATA"] {
        if let Some(value) = env::var_os(key) {
            environment.push((OsString::from(key), value));
        }
    }
    let args: Vec<OsString> = ["--exact", LPAC_HELPER_TEST, "--ignored", "--nocapture"]
        .into_iter()
        .map(OsString::from)
        .collect();
    let mut child = profile
        .spawn_suspended(
            &program,
            &args,
            &environment,
            Some(&private),
            Some(WindowsLpacStdio {
                stdin: input.as_handle(),
                stdout: output.as_handle(),
                stderr: output.as_handle(),
            }),
            &[],
        )
        .expect("suspended LPAC fixture");
    let token = child.observe_token().expect("real LPAC token");
    assert!(token.is_app_container);
    assert!(token.all_application_packages_opt_out_configured);
    assert_eq!(token.capability_count, 0);
    child.resume().expect("resume inspected LPAC fixture");
    let exit = child.wait(10_000).expect("bounded LPAC fixture exit");
    output.seek(SeekFrom::Start(0)).expect("rewind LPAC output");
    let mut observed = String::new();
    output
        .read_to_string(&mut observed)
        .expect("read LPAC output");
    assert_eq!(exit, 0, "LPAC child failed: {observed}");
    assert!(
        observed.contains(
            "KELD_265_LPAC private_create=true private_attribute=true \
                           create_denied=true rename_denied=true reparse_denied=true \
                           attribute_denied=true acl_denied=true"
        ),
        "unexpected LPAC observation: {observed}"
    );
    observe_role_private_control(&private);
    observed
}

fn observe_role_private_control(private: &Path) {
    let control = private.join("allowed.txt");
    assert_eq!(
        fs::read(&control).expect("parent observes role-private write"),
        b"role-owned"
    );
    let metadata = fs::metadata(&control).expect("parent observes role-private attribute");
    assert!(
        metadata.permissions().readonly(),
        "role attribute operation actually occurred"
    );
    let remaining = metadata.file_attributes() & !FILE_ATTRIBUTE_READONLY;
    let attributes = if remaining == 0 {
        FILE_ATTRIBUTE_NORMAL
    } else {
        remaining
    };
    let name: Vec<u16> = control.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: this checked fixture path is owned by the parent; its NUL-terminated
    // UTF-16 buffer stays live for the synchronous call. Restore only the observed
    // disposable control's readonly bit after asserting the child's actual effect.
    assert_ne!(
        unsafe { SetFileAttributesW(name.as_ptr(), attributes) },
        0,
        "restore disposable control for cleanup: {}",
        io::Error::last_os_error()
    );
}
