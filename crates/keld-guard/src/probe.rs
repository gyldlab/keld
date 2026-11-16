//! Synthetic in-process hostile probes for KEL-78 T1.
//!
//! These syscalls run in **this** process, which is not App Sandboxed, LPAC,
//! or namespaced. Direct net therefore must **not** be recorded as an OS-deny
//! pass when `connect`/`bind` reach the host stack (`ConnectionRefused` is
//! not containment). The report is evidence for [`crate::admit`], not a claim
//! that macOS/Windows/Linux are contained.

use std::fs;
use std::io::{self, ErrorKind, Write as _};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::admit::{
    HostOs, OS_CONTAINMENT_PROBES, ProbeLayer, ProbeOracle, ProbeRecord, ProbeVerdict,
    expected_layer_for,
};

/// Result of running the T1 synthetic catalog in this process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeReport {
    /// Compiled target OS. Unverified until T2–T4.
    pub os: HostOs,
    /// One row per catalog probe that this binary attempted or explicitly declined.
    pub rows: Vec<ProbeRecord>,
}

impl ProbeReport {
    /// Encode the report as JSON for the probe binary stdout.
    #[must_use]
    pub fn to_json(&self) -> String {
        let rows: Vec<serde_json::Value> = self
            .rows
            .iter()
            .map(|row| {
                serde_json::json!({
                    "probe": row.probe,
                    "expected_layer": row.expected_layer.as_str(),
                    "recorded_layer": row.recorded_layer.as_str(),
                    "verdict": match row.verdict {
                        ProbeVerdict::Pass => "pass",
                        ProbeVerdict::Fail => "fail",
                    },
                    "oracle": row.oracle.as_str(),
                    "detail": row.detail,
                })
            })
            .collect();
        serde_json::json!({
            "os": self.os.as_str(),
            "contained": false,
            "rows": rows,
        })
        .to_string()
    }

    /// Direct-net row, if present.
    #[must_use]
    pub fn direct_network(&self) -> Option<&ProbeRecord> {
        self.rows.iter().find(|row| row.probe == "direct_network")
    }
}

/// Run the T1 synthetic catalog. Never labels a successful host-stack net
/// attempt as [`ProbeOracle::OsDeny`].
#[must_use]
pub fn run_synthetic_probes() -> ProbeReport {
    let mut rows = Vec::new();
    rows.push(probe_direct_filesystem());
    rows.push(probe_direct_network());
    rows.push(probe_direct_spawn());
    for probe in OS_CONTAINMENT_PROBES {
        if rows.iter().any(|row| row.probe == *probe) {
            continue;
        }
        rows.push(not_claimed(probe));
    }
    rows.push(other_layer(
        "crash_cleanup",
        ProbeLayer::SupervisorCleanup,
        ProbeOracle::SupervisorReap,
        "T1 does not reap descendants; supervisor-cleanup is T5",
    ));
    rows.push(other_layer(
        "protocol_confusion",
        ProbeLayer::HostProtocol,
        ProbeOracle::HostProtocol,
        "T1 does not exercise kipc HELLO; host-protocol is not OS containment",
    ));
    rows.push(other_layer(
        "resource",
        ProbeLayer::ResourceLimits,
        ProbeOracle::ResourceLimit,
        "T1 does not apply worker limits",
    ));
    ProbeReport {
        os: HostOs::current(),
        rows,
    }
}

fn os_row(
    probe: &'static str,
    verdict: ProbeVerdict,
    oracle: ProbeOracle,
    detail: String,
) -> ProbeRecord {
    let layer = expected_layer_for(probe).unwrap_or(ProbeLayer::OsContainment);
    ProbeRecord {
        probe,
        expected_layer: layer,
        recorded_layer: layer,
        verdict,
        oracle,
        detail,
    }
}

fn not_claimed(probe: &'static str) -> ProbeRecord {
    os_row(
        probe,
        ProbeVerdict::Fail,
        ProbeOracle::NotOsDeny,
        "T1 synthetic probe does not mint an OS-deny pass for this row".into(),
    )
}

fn other_layer(
    probe: &'static str,
    layer: ProbeLayer,
    oracle: ProbeOracle,
    detail: &'static str,
) -> ProbeRecord {
    ProbeRecord {
        probe,
        expected_layer: layer,
        recorded_layer: layer,
        verdict: ProbeVerdict::Fail,
        oracle,
        detail: detail.into(),
    }
}

fn probe_direct_filesystem() -> ProbeRecord {
    probe_direct_filesystem_in(&std::env::temp_dir())
}

fn probe_direct_filesystem_in(directory: &Path) -> ProbeRecord {
    probe_direct_filesystem_with(directory, |path| fs::remove_file(path))
}

/// `remove` cleans up the probe file this call created; tests inject its failure.
fn probe_direct_filesystem_with(
    directory: &Path,
    remove: impl FnOnce(&Path) -> io::Result<()>,
) -> ProbeRecord {
    let path = directory.join(format!(
        "keld-kel78-t1-{}-{}",
        std::process::id(),
        "host.txt"
    ));
    // The PID-based name is predictable. Create exclusively so an occupied
    // pathname (including a symlink) is neither followed nor replaced.
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = match options.open(&path) {
        Ok(file) => file,
        // A failed open owns nothing, so it never unlinks the pathname.
        Err(err) => return filesystem_create_refused(&path, &err),
    };
    // The exclusive create succeeded, so the host path is not contained whatever
    // the write or cleanup report; neither can turn this row into an OS deny.
    let write_result = file.write_all(b"kel78-t1");
    drop(file);
    let cleanup = remove(&path);
    let write_note = match write_result {
        Ok(()) => String::new(),
        Err(err) => format!("; write to the created probe file failed: {err}"),
    };
    let cleanup_note = match cleanup {
        Ok(()) => String::new(),
        Err(err) => format!("; cleanup of the created probe file failed: {err}"),
    };
    os_row(
        "direct_filesystem",
        ProbeVerdict::Fail,
        ProbeOracle::NotOsDeny,
        format!("host temp create succeeded; not an OS deny{write_note}{cleanup_note}"),
    )
}

/// Classify a refused exclusive create. `PermissionDenied` alone is not an OS
/// deny: on Windows `CreateFileW(CREATE_NEW)` also reports `ERROR_ACCESS_DENIED`
/// for an occupant it cannot replace, such as a delete-pending file. The deny
/// counts only when the probe name is proven unoccupied afterwards; an occupant,
/// or a name whose occupancy cannot be read, is not an OS-deny pass. An occupant
/// that disappears between the create and this read is not distinguished.
///
/// This fails closed: a containment that denies the create and also denies the
/// lookup (for example a token without traverse or list rights on the parent, if
/// the lookup reports `ACCESS_DENIED` rather than `NotFound`) yields a `Fail` row
/// marked "occupancy unreadable", never a `Pass`. Such a containment needs a probe
/// directory whose names it can stat before this row can prove its deny.
fn filesystem_create_refused(path: &Path, err: &io::Error) -> ProbeRecord {
    if !is_sandbox_deny(err) {
        return os_row(
            "direct_filesystem",
            ProbeVerdict::Fail,
            ProbeOracle::NotOsDeny,
            format!("create failed without sandbox deny: {err}"),
        );
    }
    match fs::symlink_metadata(path) {
        Err(absent) if absent.kind() == ErrorKind::NotFound => os_row(
            "direct_filesystem",
            ProbeVerdict::Pass,
            ProbeOracle::OsDeny,
            format!("OS deny: {err}"),
        ),
        Ok(_) => os_row(
            "direct_filesystem",
            ProbeVerdict::Fail,
            ProbeOracle::NotOsDeny,
            format!("create denied by an entry already at the probe name; not an OS deny: {err}"),
        ),
        Err(unknown) => os_row(
            "direct_filesystem",
            ProbeVerdict::Fail,
            ProbeOracle::NotOsDeny,
            format!(
                "create denied but the probe name's occupancy is unreadable ({unknown}); not an OS deny: {err}"
            ),
        ),
    }
}

fn probe_direct_network() -> ProbeRecord {
    // Bind success means the host network stack accepted a socket. Spec
    // requires connect/bind/socket to fail with an OS deny. ConnectionRefused
    // on connect is not that deny; bind-on-localhost is the T1 oracle.
    match TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)) {
        Ok(listener) => os_row(
            "direct_network",
            ProbeVerdict::Fail,
            ProbeOracle::NotOsDeny,
            format!(
                "bind {:?} succeeded; not an OS deny (direct net stays OS-deny in the contract)",
                listener.local_addr().ok()
            ),
        ),
        Err(err) if is_sandbox_deny(&err) => os_row(
            "direct_network",
            ProbeVerdict::Pass,
            ProbeOracle::OsDeny,
            format!("bind OS deny: {err}"),
        ),
        Err(err) => os_row(
            "direct_network",
            ProbeVerdict::Fail,
            ProbeOracle::NotOsDeny,
            format!("bind failed without sandbox deny: {err}"),
        ),
    }
}

fn probe_direct_spawn() -> ProbeRecord {
    let mut cmd = spawn_helper();
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    match cmd.status() {
        Ok(status) => os_row(
            "direct_spawn",
            ProbeVerdict::Fail,
            ProbeOracle::NotOsDeny,
            format!("spawned helper exited {status}; not an OS deny of exec"),
        ),
        Err(err) if is_sandbox_deny(&err) => os_row(
            "direct_spawn",
            ProbeVerdict::Pass,
            ProbeOracle::OsDeny,
            format!("spawn OS deny: {err}"),
        ),
        Err(err) => os_row(
            "direct_spawn",
            ProbeVerdict::Fail,
            ProbeOracle::NotOsDeny,
            format!("spawn failed without sandbox deny: {err}"),
        ),
    }
}

fn spawn_helper() -> Command {
    #[cfg(windows)]
    {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", "exit", "0"]);
        cmd
    }
    #[cfg(not(windows))]
    {
        Command::new("/bin/true")
    }
}

fn is_sandbox_deny(err: &io::Error) -> bool {
    matches!(err.kind(), ErrorKind::PermissionDenied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admit::{
        AdmissionError, AdmissionRequest, HostFacts, ProfileState, RoleInstance, admit,
    };

    struct ProbeFixture {
        directory: std::path::PathBuf,
    }

    impl ProbeFixture {
        fn new(case: &str) -> Self {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("current clock")
                .as_nanos();
            let directory = std::env::temp_dir().join(format!(
                "keld-guard-probe-{case}-{}-{nonce}",
                std::process::id()
            ));
            #[cfg(unix)]
            let mut builder = fs::DirBuilder::new();
            #[cfg(not(unix))]
            let builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt as _;
                builder.mode(0o700);
            }
            builder
                .create(&directory)
                .expect("new private probe fixture");
            Self { directory }
        }

        fn directory(&self) -> &Path {
            &self.directory
        }

        fn predicted_path(&self) -> std::path::PathBuf {
            self.directory
                .join(format!("keld-kel78-t1-{}-host.txt", std::process::id()))
        }
    }

    impl Drop for ProbeFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }

    #[cfg(unix)]
    #[test]
    fn filesystem_probe_preserves_preseeded_symlink_and_its_target() {
        use std::os::unix::fs::symlink;

        let fixture = ProbeFixture::new("symlink");
        let sentinel = fixture.directory().join("sentinel");
        fs::write(&sentinel, b"original sentinel bytes").expect("seed sentinel");
        let predicted = fixture.predicted_path();
        symlink(&sentinel, &predicted).expect("preseed predicted filename as a symlink");

        let row = probe_direct_filesystem_in(fixture.directory());

        assert_eq!(
            fs::read(&sentinel).expect("read sentinel"),
            b"original sentinel bytes",
            "the probe must not write through a pre-existing symlink"
        );
        assert!(
            fs::symlink_metadata(&predicted)
                .expect("existing symlink remains")
                .file_type()
                .is_symlink(),
            "the probe must not remove an entry it did not create"
        );
        assert_eq!(row.verdict, ProbeVerdict::Fail);
        assert_eq!(row.oracle, ProbeOracle::NotOsDeny);
        assert!(
            row.detail
                .starts_with("create failed without sandbox deny:"),
            "{row:?}"
        );
    }

    #[test]
    fn filesystem_probe_preserves_preexisting_regular_file() {
        let fixture = ProbeFixture::new("regular");
        let predicted = fixture.predicted_path();
        fs::write(&predicted, b"pre-existing data").expect("seed occupied filename");

        let row = probe_direct_filesystem_in(fixture.directory());

        assert_eq!(
            fs::read(&predicted).expect("pre-existing file remains"),
            b"pre-existing data",
            "the probe must not overwrite or remove an occupied filename"
        );
        assert_eq!(row.verdict, ProbeVerdict::Fail);
        assert_eq!(row.oracle, ProbeOracle::NotOsDeny);
        assert!(
            row.detail
                .starts_with("create failed without sandbox deny:"),
            "{row:?}"
        );
    }

    #[test]
    fn filesystem_probe_removes_its_own_created_file_and_reports_real_io() {
        let fixture = ProbeFixture::new("created");
        let predicted = fixture.predicted_path();

        let success = probe_direct_filesystem_in(fixture.directory());

        assert_eq!(success.verdict, ProbeVerdict::Fail);
        assert_eq!(success.oracle, ProbeOracle::NotOsDeny);
        assert_eq!(success.detail, "host temp create succeeded; not an OS deny");
        assert!(
            fs::symlink_metadata(&predicted).is_err(),
            "the probe must remove its own temporary file"
        );

        let missing_parent = fixture.directory().join("absent-directory");
        let missing = probe_direct_filesystem_in(&missing_parent);
        assert_eq!(missing.verdict, ProbeVerdict::Fail);
        assert_eq!(missing.oracle, ProbeOracle::NotOsDeny);
        assert!(
            missing
                .detail
                .starts_with("create failed without sandbox deny:"),
            "{missing:?}"
        );
    }

    #[test]
    fn filesystem_probe_does_not_pass_on_a_directory_at_the_probe_name() {
        let fixture = ProbeFixture::new("directory");
        let predicted = fixture.predicted_path();
        fs::create_dir(&predicted).expect("preseed predicted filename as a directory");

        let row = probe_direct_filesystem_in(fixture.directory());

        assert!(
            fs::symlink_metadata(&predicted)
                .expect("pre-existing directory remains")
                .is_dir(),
            "the probe must not remove an entry it did not create"
        );
        assert_eq!(row.verdict, ProbeVerdict::Fail, "{row:?}");
        assert_eq!(row.oracle, ProbeOracle::NotOsDeny, "{row:?}");
    }

    /// `CreateFileW(CREATE_NEW)` reports `ERROR_ACCESS_DENIED` for a name held by a
    /// delete-pending file, which is an occupant, not an OS deny of the create.
    #[cfg(windows)]
    #[test]
    fn filesystem_probe_does_not_pass_on_a_delete_pending_file_at_the_probe_name() {
        use std::os::windows::fs::OpenOptionsExt as _;
        const DELETE: u32 = 0x0001_0000;
        const SHARE_ALL: u32 = 0x1 | 0x2 | 0x4;
        const FILE_FLAG_DELETE_ON_CLOSE: u32 = 0x0400_0000;

        let fixture = ProbeFixture::new("delete-pending");
        let predicted = fixture.predicted_path();
        fs::write(&predicted, b"occupant").expect("seed occupant");
        let holder = fs::OpenOptions::new()
            .read(true)
            .share_mode(SHARE_ALL)
            .open(&predicted)
            .expect("hold the occupant open");
        drop(
            fs::OpenOptions::new()
                .access_mode(DELETE)
                .share_mode(SHARE_ALL)
                .custom_flags(FILE_FLAG_DELETE_ON_CLOSE)
                .open(&predicted)
                .expect("open the occupant delete-on-close"),
        );
        let pending = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&predicted)
            .expect_err("the delete-pending name cannot be created");
        assert_eq!(
            pending.kind(),
            ErrorKind::PermissionDenied,
            "fixture must reproduce the ambiguous Win32 ACCESS_DENIED: {pending}"
        );

        let row = probe_direct_filesystem_in(fixture.directory());
        drop(holder);

        assert_eq!(row.verdict, ProbeVerdict::Fail, "{row:?}");
        assert_eq!(row.oracle, ProbeOracle::NotOsDeny, "{row:?}");
        assert!(row.detail.contains("not an OS deny"), "{row:?}");
    }

    /// Positive control: a real permission deny on an unoccupied name stays an OS deny.
    #[cfg(unix)]
    #[test]
    fn filesystem_probe_passes_a_real_permission_deny_on_an_unoccupied_name() {
        use std::os::unix::fs::PermissionsExt as _;

        let fixture = ProbeFixture::new("denied");
        let denied = fixture.directory().join("read-only");
        fs::create_dir(&denied).expect("create denied parent");
        fs::set_permissions(&denied, fs::Permissions::from_mode(0o500))
            .expect("make denied parent read-only");
        let mode = fs::metadata(&denied)
            .expect("read denied parent mode")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o500, "fixture must apply mode 0o500");
        // Fixture prerequisite: the applied mode must actually deny a create. It
        // does not when the process overrides DAC checks (root, CAP_DAC_OVERRIDE),
        // the filesystem ignores mode bits, or an ACL grants the create. This fails
        // instead of skipping, so no test leg can pass without running this control.
        let sentinel = denied.join("deny-prerequisite-sentinel");
        let created = fs::File::create(&sentinel).is_ok();
        if created {
            fs::remove_file(&sentinel).expect("remove deny-prerequisite sentinel");
            fs::set_permissions(&denied, fs::Permissions::from_mode(0o700))
                .expect("restore denied parent for cleanup");
        }
        assert!(
            !created,
            "fixture prerequisite missing: a create inside the mode-0o500 directory \
             succeeded, so this environment does not enforce directory mode bits for \
             this process and the positive control cannot build a real deny; run it as \
             an unprivileged user on a filesystem that enforces mode bits"
        );

        let row = probe_direct_filesystem_in(&denied);
        fs::set_permissions(&denied, fs::Permissions::from_mode(0o700))
            .expect("restore denied parent for cleanup");

        assert_eq!(row.verdict, ProbeVerdict::Pass, "{row:?}");
        assert_eq!(row.oracle, ProbeOracle::OsDeny, "{row:?}");
        assert!(row.detail.starts_with("OS deny:"), "{row:?}");
    }

    #[test]
    fn filesystem_probe_reports_a_failed_cleanup_of_its_own_file() {
        let fixture = ProbeFixture::new("cleanup");
        let predicted = fixture.predicted_path();

        let row = probe_direct_filesystem_with(fixture.directory(), |_| {
            Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "injected cleanup refusal",
            ))
        });

        assert_eq!(row.verdict, ProbeVerdict::Fail, "{row:?}");
        assert_eq!(row.oracle, ProbeOracle::NotOsDeny, "{row:?}");
        assert!(
            row.detail
                .contains("cleanup of the created probe file failed: injected cleanup refusal"),
            "{row:?}"
        );
        assert!(
            fs::symlink_metadata(&predicted).is_ok(),
            "the injected cleanup left the created file for the fixture to remove"
        );
    }

    #[test]
    fn live_direct_network_is_not_an_os_deny_pass() {
        let report = run_synthetic_probes();
        let net = report.direct_network().expect("direct_network row");
        assert_eq!(net.expected_layer, ProbeLayer::OsContainment);
        assert_ne!(
            (net.verdict, net.oracle),
            (ProbeVerdict::Pass, ProbeOracle::OsDeny),
            "unsandboxed connect/bind must not be recorded as OS containment: {}",
            net.detail
        );
        assert_eq!(net.oracle, ProbeOracle::NotOsDeny);
        assert_eq!(net.verdict, ProbeVerdict::Fail);
        let json = report.to_json();
        assert!(json.contains("\"contained\":false"), "{json}");
        assert!(json.contains("direct_network"), "{json}");
    }

    #[test]
    fn live_report_cannot_admit_strict_even_if_primitives_were_present() {
        let report = run_synthetic_probes();
        // Use the same private constructor path as admit tests via observe + empty missing
        // is not possible from here; admit with uncontained facts is PrimitiveUnavailable.
        let req = AdmissionRequest {
            role: RoleInstance { generation: 1 },
            requested: ProfileState::Strict,
            artifact_digest: crate::admit::ArtifactDigest([1; 32]),
            profile_digest: crate::admit::ProfileDigest([2; 32]),
            proof: None,
            electron_sandbox_off: false,
            inherit_from_host: false,
        };
        match admit(&req, &HostFacts::observe_uncontained(), None) {
            Err(AdmissionError::PrimitiveUnavailable { .. }) => {}
            other => panic!("{other:?}"),
        }
        assert!(
            report
                .rows
                .iter()
                .any(|row| row.probe == "direct_network" && row.verdict == ProbeVerdict::Fail),
            "live net row must fail so a later primitive observer still cannot use it as a pass"
        );
    }
}
