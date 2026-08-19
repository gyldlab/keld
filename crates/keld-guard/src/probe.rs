//! Synthetic in-process hostile probes for KEL-78 T1.
//!
//! These syscalls run in **this** process, which is not App Sandboxed, LPAC,
//! or namespaced. Direct net therefore must **not** be recorded as an OS-deny
//! pass when `connect`/`bind` reach the host stack (`ConnectionRefused` is
//! not containment). The report is evidence for [`crate::admit`], not a claim
//! that macOS/Windows/Linux are contained.

use std::fs;
use std::io::{self, ErrorKind};
use std::net::TcpListener;
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
    let path = std::env::temp_dir().join(format!(
        "keld-kel78-t1-{}-{}",
        std::process::id(),
        "host.txt"
    ));
    let result = fs::write(&path, b"kel78-t1");
    if path.exists() {
        let _ = fs::remove_file(&path);
    }
    match result {
        Ok(()) => os_row(
            "direct_filesystem",
            ProbeVerdict::Fail,
            ProbeOracle::NotOsDeny,
            "host temp create succeeded; not an OS deny".into(),
        ),
        Err(err) if is_sandbox_deny(&err) => os_row(
            "direct_filesystem",
            ProbeVerdict::Pass,
            ProbeOracle::OsDeny,
            format!("OS deny: {err}"),
        ),
        Err(err) => os_row(
            "direct_filesystem",
            ProbeVerdict::Fail,
            ProbeOracle::NotOsDeny,
            format!("create failed without sandbox deny: {err}"),
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
