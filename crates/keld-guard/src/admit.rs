//! KEL-78 T1 strict-profile admission state machine.
//!
//! [`admit`] may return [`ProfileState::Strict`] only when every spec §4
//! `admit` check passes, including a complete §7 OS-containment catalog.
//! T1 does **not** observe App Sandbox, LPAC, or Linux namespaces:
//! [`HostFacts::observe_uncontained`] reports every required primitive
//! missing, so a production Strict request fails closed. A unit-test
//! fixture that returns Strict is a state-machine oracle, not an OS proof.

use std::fmt;

/// SHA-256 of the launched binary. Compared to the archived proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ArtifactDigest(pub [u8; 32]);

/// SHA-256 of the token/profile this spawn will apply.
///
/// Not interchangeable with [`ArtifactDigest`]: the same binary under a
/// different entitlement / LPAC / capability set is a different value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProfileDigest(pub [u8; 32]);

/// Identifier of a hostile-probe archive.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ArchiveId(pub String);

/// OS the proof and host must agree on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HostOs {
    /// macOS App Sandbox candidate (T2).
    Macos,
    /// Windows zero-capability LPAC candidate (T3).
    Windows,
    /// Linux namespace + mount-table deny candidate (T4).
    Linux,
}

impl HostOs {
    /// The compiled target OS. Other targets do not exist in this workspace.
    #[must_use]
    pub const fn current() -> Self {
        #[cfg(target_os = "macos")]
        {
            Self::Macos
        }
        #[cfg(target_os = "windows")]
        {
            Self::Windows
        }
        #[cfg(target_os = "linux")]
        {
            Self::Linux
        }
    }

    /// Stable report token (`macos` / `windows` / `linux`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Macos => "macos",
            Self::Windows => "windows",
            Self::Linux => "linux",
        }
    }
}

impl fmt::Display for HostOs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Three profile states. Not a confidence slider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProfileState {
    /// Default until a complete per-OS OS-containment archive exists.
    Unverified,
    /// Explicit Keld legacy declaration; zero-authority claim forfeited.
    Legacy,
    /// Admission plus a complete OS-containment archive for that OS/artifact/profile.
    Strict,
}

impl ProfileState {
    /// Printed state string for doctor/build/release (`unverified` / `legacy` / `strict`).
    #[must_use]
    pub const fn as_report_str(self) -> &'static str {
        match self {
            Self::Unverified => "unverified",
            Self::Legacy => "legacy",
            Self::Strict => "strict",
        }
    }
}

impl fmt::Display for ProfileState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_report_str())
    }
}

/// Host-minted role generation (KEL-75 owns the fuller `RoleInstance` later).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RoleInstance {
    /// Generation that a KEL-70 restart must keep when Strict admission failed.
    pub generation: u32,
}

/// Identity stored in an archived proof.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProofIdentity {
    /// OS the archive was taken on.
    pub os: HostOs,
    /// Launched binary digest.
    pub artifact_digest: ArtifactDigest,
    /// Token/profile digest this spawn will apply.
    pub profile_digest: ProfileDigest,
    /// Archive locator.
    pub archive_id: ArchiveId,
}

/// Request to admit a profile for one spawn.
///
/// `requested` is never inferred from a package name or Electron
/// `sandbox` / `appSandbox`. Leaving Strict is this function returning
/// `Err`: callers MUST NOT start an unsandboxed replacement, and a
/// KEL-70 restart MUST keep [`AdmissionRequest::requested`] as `Strict`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmissionRequest {
    /// Role generation for this spawn.
    pub role: RoleInstance,
    /// Caller-declared profile. Never inferred.
    pub requested: ProfileState,
    /// Digest of the binary that would be launched.
    pub artifact_digest: ArtifactDigest,
    /// Digest of the token/profile that would be applied.
    pub profile_digest: ProfileDigest,
    /// Identity of the archive, if any. `None` is [`AdmissionError::ProofMissing`].
    pub proof: Option<ProofIdentity>,
    /// Leftover Electron `sandbox` / `appSandbox: "off"` while requesting Strict.
    pub electron_sandbox_off: bool,
    /// Helper would inherit the host sandbox (`com.apple.security.inherit`).
    pub inherit_from_host: bool,
}

/// Layer named in spec §7. Each catalog row is exactly one layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProbeLayer {
    /// Direct syscall/API deny by the OS profile.
    OsContainment,
    /// Supervisor/job/PID reap — not OS containment.
    SupervisorCleanup,
    /// Authenticated app-link / typed host deny — not OS containment.
    HostProtocol,
    /// Worker resource limits — not OS containment.
    ResourceLimits,
}

impl ProbeLayer {
    /// Stable id used in errors and probe JSON.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OsContainment => "os_containment",
            Self::SupervisorCleanup => "supervisor_cleanup",
            Self::HostProtocol => "host_protocol",
            Self::ResourceLimits => "resource_limits",
        }
    }
}

/// Independent oracle that produced a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProbeOracle {
    /// Direct syscall/API returned a sandbox-class deny (`PermissionDenied`).
    OsDeny,
    /// The attempt used the host OS (success, `ConnectionRefused`, timeout, …).
    NotOsDeny,
    /// JS / `keld-guard` deny, not the OS profile.
    JsShim,
    /// `KELD-IPC-007` or other host-protocol deny.
    HostProtocol,
    /// Supervisor/job/PID reap.
    SupervisorReap,
    /// Resource-limit hit.
    ResourceLimit,
}

impl ProbeOracle {
    /// Stable id used in errors and probe JSON.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OsDeny => "os_deny",
            Self::NotOsDeny => "not_os_deny",
            Self::JsShim => "js_shim",
            Self::HostProtocol => "host_protocol",
            Self::SupervisorReap => "supervisor_reap",
            Self::ResourceLimit => "resource_limit",
        }
    }

    /// Spec check 9: an OS-containment pass must use this oracle.
    #[must_use]
    pub const fn admits_os_containment_pass(self) -> bool {
        matches!(self, Self::OsDeny)
    }
}

/// Whether a catalog row recorded a pass for its layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProbeVerdict {
    /// Row passed its independent oracle.
    Pass,
    /// Row ran and did not prove the required observation.
    Fail,
}

/// One hostile-catalog row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeRecord {
    /// Catalog probe id (`direct_network`, …).
    pub probe: &'static str,
    /// Spec §7 layer for this probe.
    pub expected_layer: ProbeLayer,
    /// Layer the archive attributed.
    pub recorded_layer: ProbeLayer,
    /// Pass or fail.
    pub verdict: ProbeVerdict,
    /// Oracle that produced the verdict.
    pub oracle: ProbeOracle,
    /// Non-secret diagnostic (errno text, skipped reason).
    pub detail: String,
}

/// Archived probe results bound to a [`ProofIdentity`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofArchive {
    /// Identity this archive claims to prove.
    pub identity: ProofIdentity,
    /// Hostile-catalog rows. May include non-OS layers; those never admit Strict.
    pub rows: Vec<ProbeRecord>,
    /// §4 policy generation this archive was taken under. Older than
    /// [`HostFacts::policy_generation`] is stale.
    pub policy_generation: u64,
}

/// Observed host facts for this spawn. T1's public constructor never claims
/// a sandbox primitive is present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostFacts {
    os: HostOs,
    missing_primitives: Vec<&'static str>,
    policy_generation: u64,
    handle_leak: Option<String>,
}

impl HostFacts {
    /// Live T1 observation: no §4 primitive is applied on this process.
    ///
    /// macOS, Windows, and Linux remain **unverified**. This MUST NOT be
    /// treated as OS containment.
    #[must_use]
    pub fn observe_uncontained() -> Self {
        Self {
            os: HostOs::current(),
            missing_primitives: required_primitives(HostOs::current()).to_vec(),
            policy_generation: CURRENT_POLICY_GENERATION,
            handle_leak: None,
        }
    }

    /// Host OS used for `proof.os` comparison.
    #[must_use]
    pub const fn os(&self) -> HostOs {
        self.os
    }

    /// Policy generation the archive must meet.
    #[must_use]
    pub const fn policy_generation(&self) -> u64 {
        self.policy_generation
    }
}

/// §4 policy generation for this spec text. Bump when T2–T4 change required primitives.
pub const CURRENT_POLICY_GENERATION: u64 = 1;

/// Spec §7 OS-containment probe ids. Every id must have a recorded OS-deny pass
/// before [`admit`] can return [`ProfileState::Strict`].
pub const OS_CONTAINMENT_PROBES: &[&str] = &[
    "direct_filesystem",
    "direct_network",
    "direct_spawn",
    "inherited_handles",
    "scm_rights",
    "descendants",
    "broker_bypass",
    "token_theft",
    "addon_escape",
    "updater_boundary",
];

/// Expected layer for a §7 probe id. Direct net is always OS containment
/// in the contract, including when a `keld-guard` net grant exists.
#[must_use]
pub fn expected_layer_for(probe: &str) -> Option<ProbeLayer> {
    match probe {
        "crash_cleanup" => Some(ProbeLayer::SupervisorCleanup),
        "protocol_confusion" => Some(ProbeLayer::HostProtocol),
        "resource" => Some(ProbeLayer::ResourceLimits),
        id if OS_CONTAINMENT_PROBES.contains(&id) => Some(ProbeLayer::OsContainment),
        _ => None,
    }
}

fn required_primitives(os: HostOs) -> &'static [&'static str] {
    match os {
        HostOs::Macos => &[
            "macos.app-sandbox",
            "macos.no-inherit-from-host",
            "macos.no-network-entitlement",
        ],
        HostOs::Windows => &[
            "windows.zero-capability-lpac",
            "windows.all-application-packages-opt-out",
            "windows.handle-allowlist",
        ],
        HostOs::Linux => &[
            "linux.user-mount-pid-net-namespaces",
            "linux.host-path-deny-mount-table",
            "linux.no-new-privs",
            "linux.empty-capabilities",
            "linux.seccomp-no-new-userns",
        ],
    }
}

/// Which identity field failed [`AdmissionError::ProofMismatch`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MismatchField {
    /// `artifact_digest`.
    ArtifactDigest,
    /// `profile_digest`.
    ProfileDigest,
    /// `os`.
    Os,
    /// `archive_id`.
    ArchiveId,
}

impl MismatchField {
    /// Stable field name in the error text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ArtifactDigest => "artifact_digest",
            Self::ProfileDigest => "profile_digest",
            Self::Os => "os",
            Self::ArchiveId => "archive_id",
        }
    }
}

/// Typed admission failure. Leaving Strict is this error: no replacement child.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmissionError {
    /// A required §4 primitive is missing on this host.
    PrimitiveUnavailable {
        /// OS that lacks the primitive.
        os: HostOs,
        /// Primitive id.
        primitive: &'static str,
        /// How to proceed.
        fix: &'static str,
    },
    /// Inherit-from-host, leftover Electron `appSandbox: "off"`, or extra entitlement/SID.
    UnexpectedGrant {
        /// OS of this spawn.
        os: HostOs,
        /// Grant or facade that is forbidden for Strict.
        grant: &'static str,
        /// How to proceed.
        fix: &'static str,
    },
    /// A host handle would leak into the child.
    HandleLeak {
        /// Non-secret description of the leak.
        description: String,
        /// How to proceed.
        fix: &'static str,
    },
    /// Strict requested; proof absent or archive empty.
    ProofMissing {
        /// OS of this spawn.
        os: HostOs,
        /// How to proceed.
        fix: &'static str,
    },
    /// Archive is not fresh for this artifact/profile/policy generation.
    ProofStale {
        /// OS of this spawn.
        os: HostOs,
        /// Archive that was presented.
        archive_id: ArchiveId,
        /// Why it is stale.
        reason: &'static str,
        /// How to proceed.
        fix: &'static str,
    },
    /// Artifact digest, profile digest, OS, or archive id disagrees.
    ProofMismatch {
        /// OS of this spawn.
        os: HostOs,
        /// Which field disagreed.
        field: MismatchField,
        /// Requested/host value.
        expected: String,
        /// Archived value.
        found: String,
        /// How to proceed.
        fix: &'static str,
    },
    /// One or more §7 OS-containment rows lack a recorded OS-deny pass.
    ProofIncomplete {
        /// OS of this spawn.
        os: HostOs,
        /// Missing probe ids, comma-separated.
        missing: String,
        /// How to proceed.
        fix: &'static str,
    },
    /// An OS-containment slot used another layer's oracle, or the wrong layer.
    ProofLayerMismatch {
        /// OS of this spawn.
        os: HostOs,
        /// Probe id.
        probe: &'static str,
        /// Spec layer.
        expected_layer: ProbeLayer,
        /// What the archive recorded.
        recorded_layer: ProbeLayer,
        /// Independent oracle the archive recorded (not the layer id).
        oracle: ProbeOracle,
        /// How to proceed.
        fix: &'static str,
    },
}

impl AdmissionError {
    /// Stable `KELD-GUARD*` code. Spec sketch said `KELD-RUNTIME-0xx`; T1
    /// emits from `keld-guard`, so codes follow the emitting crate.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::PrimitiveUnavailable { .. } => "KELD-GUARD008",
            Self::UnexpectedGrant { .. } => "KELD-GUARD009",
            Self::HandleLeak { .. } => "KELD-GUARD010",
            Self::ProofMissing { .. } => "KELD-GUARD011",
            Self::ProofStale { .. } => "KELD-GUARD012",
            Self::ProofMismatch { .. } => "KELD-GUARD013",
            Self::ProofIncomplete { .. } => "KELD-GUARD014",
            Self::ProofLayerMismatch { .. } => "KELD-GUARD015",
        }
    }
}

impl fmt::Display for AdmissionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PrimitiveUnavailable { os, primitive, fix } => write!(
                f,
                "KELD-GUARD008: required strict primitive `{primitive}` is unavailable on {os}. {fix}"
            ),
            Self::UnexpectedGrant { os, grant, fix } => write!(
                f,
                "KELD-GUARD009: unexpected grant `{grant}` on {os} while requesting Strict. {fix}"
            ),
            Self::HandleLeak {
                description, fix, ..
            } => write!(
                f,
                "KELD-GUARD010: host handle would leak into the child ({description}). {fix}"
            ),
            Self::ProofMissing { os, fix } => write!(
                f,
                "KELD-GUARD011: Strict requested on {os} but the OS-containment proof is missing. {fix}"
            ),
            Self::ProofStale {
                os,
                archive_id,
                reason,
                fix,
            } => write!(
                f,
                "KELD-GUARD012: Strict proof `{id}` on {os} is stale ({reason}). {fix}",
                id = archive_id.0
            ),
            Self::ProofMismatch {
                os,
                field,
                expected,
                found,
                fix,
            } => write!(
                f,
                "KELD-GUARD013: Strict proof {field} mismatch on {os}: expected `{expected}`, found `{found}`. {fix}",
                field = field.as_str()
            ),
            Self::ProofIncomplete { os, missing, fix } => write!(
                f,
                "KELD-GUARD014: Strict OS-containment catalog on {os} is incomplete (missing {missing}). {fix}"
            ),
            Self::ProofLayerMismatch {
                os,
                probe,
                expected_layer,
                recorded_layer,
                oracle,
                fix,
            } => write!(
                f,
                "KELD-GUARD015: Strict archive row `{probe}` on {os} recorded layer `{recorded}` and oracle `{oracle}` instead of layer `{expected}` and oracle `{needed}`. {fix}",
                expected = expected_layer.as_str(),
                recorded = recorded_layer.as_str(),
                oracle = oracle.as_str(),
                needed = expected_oracle_for(*expected_layer).as_str(),
            ),
        }
    }
}

impl std::error::Error for AdmissionError {}

const FIX_NO_CHILD: &str = "Do not start a child and do not restart unsandboxed; keep requested=Strict or declare explicit Keld legacy.";

/// Admit a profile for one spawn.
///
/// `Unverified` and `Legacy` do not require an archive. `Strict` requires
/// every check in spec §4. A live T1 [`HostFacts::observe_uncontained`]
/// fails [`AdmissionError::PrimitiveUnavailable`] before any catalog can
/// admit Strict — that is intentional: T1 has no OS sandbox.
///
/// # Errors
///
/// Returns a typed [`AdmissionError`] when Strict cannot be admitted.
/// Callers MUST NOT start a child (including an unsandboxed KEL-70 restart)
/// on `Err`.
pub fn admit(
    req: &AdmissionRequest,
    facts: &HostFacts,
    archive: Option<&ProofArchive>,
) -> Result<ProfileState, AdmissionError> {
    match req.requested {
        ProfileState::Unverified => Ok(ProfileState::Unverified),
        ProfileState::Legacy => Ok(ProfileState::Legacy),
        ProfileState::Strict => admit_strict(req, facts, archive),
    }
}

fn admit_strict(
    req: &AdmissionRequest,
    facts: &HostFacts,
    archive: Option<&ProofArchive>,
) -> Result<ProfileState, AdmissionError> {
    reject_unexpected(req, facts)?;
    require_primitives(facts)?;
    let proof = req.proof.as_ref().ok_or_else(|| proof_missing(facts.os))?;
    let archive = require_nonempty_archive(facts.os, archive)?;
    match_identity(req, facts, proof, archive)?;
    if archive.policy_generation < facts.policy_generation {
        return Err(AdmissionError::ProofStale {
            os: facts.os,
            archive_id: proof.archive_id.clone(),
            reason: "archive predates the current §4 policy generation",
            fix: FIX_NO_CHILD,
        });
    }
    if let Some(err) = layer_mismatch(facts.os, &archive.rows) {
        return Err(err);
    }
    if let Some(missing) = incomplete_os_rows(&archive.rows) {
        return Err(AdmissionError::ProofIncomplete {
            os: facts.os,
            missing,
            fix: FIX_NO_CHILD,
        });
    }
    Ok(ProfileState::Strict)
}

fn proof_missing(os: HostOs) -> AdmissionError {
    AdmissionError::ProofMissing {
        os,
        fix: FIX_NO_CHILD,
    }
}

fn reject_unexpected(req: &AdmissionRequest, facts: &HostFacts) -> Result<(), AdmissionError> {
    if req.electron_sandbox_off {
        return Err(AdmissionError::UnexpectedGrant {
            os: facts.os,
            grant: "electron.appSandbox=off",
            fix: "Declare the Keld profile key explicitly; Electron sandbox/appSandbox is not Strict or Legacy. Do not start.",
        });
    }
    if req.inherit_from_host {
        return Err(AdmissionError::UnexpectedGrant {
            os: facts.os,
            grant: "inherit-from-host",
            fix: "Sign a separate helper with its own App Sandbox; do not inherit keld-host. Do not start.",
        });
    }
    if let Some(description) = facts.handle_leak.as_ref() {
        return Err(AdmissionError::HandleLeak {
            description: description.clone(),
            fix: "Strip inherited handles to app-link + log sinks, then retry admission. Do not start.",
        });
    }
    Ok(())
}

fn require_primitives(facts: &HostFacts) -> Result<(), AdmissionError> {
    match facts.missing_primitives.first().copied() {
        Some(primitive) => Err(AdmissionError::PrimitiveUnavailable {
            os: facts.os,
            primitive,
            fix: FIX_NO_CHILD,
        }),
        None => Ok(()),
    }
}

fn require_nonempty_archive(
    os: HostOs,
    archive: Option<&ProofArchive>,
) -> Result<&ProofArchive, AdmissionError> {
    match archive {
        Some(archive) if !archive.rows.is_empty() => Ok(archive),
        Some(_) | None => Err(proof_missing(os)),
    }
}

fn match_identity(
    req: &AdmissionRequest,
    facts: &HostFacts,
    proof: &ProofIdentity,
    archive: &ProofArchive,
) -> Result<(), AdmissionError> {
    if proof.os != facts.os {
        return Err(mismatch(
            facts.os,
            MismatchField::Os,
            facts.os.as_str(),
            proof.os.as_str(),
        ));
    }
    if proof.artifact_digest != req.artifact_digest {
        return Err(mismatch(
            facts.os,
            MismatchField::ArtifactDigest,
            &hex32(&req.artifact_digest.0),
            &hex32(&proof.artifact_digest.0),
        ));
    }
    if proof.profile_digest != req.profile_digest {
        return Err(mismatch(
            facts.os,
            MismatchField::ProfileDigest,
            &hex32(&req.profile_digest.0),
            &hex32(&proof.profile_digest.0),
        ));
    }
    if proof.archive_id != archive.identity.archive_id {
        return Err(mismatch(
            facts.os,
            MismatchField::ArchiveId,
            &proof.archive_id.0,
            &archive.identity.archive_id.0,
        ));
    }
    if archive.identity == *proof {
        return Ok(());
    }
    if archive.identity.artifact_digest != proof.artifact_digest {
        return Err(mismatch(
            facts.os,
            MismatchField::ArtifactDigest,
            &hex32(&proof.artifact_digest.0),
            &hex32(&archive.identity.artifact_digest.0),
        ));
    }
    if archive.identity.profile_digest != proof.profile_digest {
        return Err(mismatch(
            facts.os,
            MismatchField::ProfileDigest,
            &hex32(&proof.profile_digest.0),
            &hex32(&archive.identity.profile_digest.0),
        ));
    }
    Err(mismatch(
        facts.os,
        MismatchField::Os,
        proof.os.as_str(),
        archive.identity.os.as_str(),
    ))
}

fn mismatch(os: HostOs, field: MismatchField, expected: &str, found: &str) -> AdmissionError {
    AdmissionError::ProofMismatch {
        os,
        field,
        expected: expected.to_owned(),
        found: found.to_owned(),
        fix: FIX_NO_CHILD,
    }
}

const fn expected_oracle_for(layer: ProbeLayer) -> ProbeOracle {
    match layer {
        ProbeLayer::OsContainment => ProbeOracle::OsDeny,
        ProbeLayer::SupervisorCleanup => ProbeOracle::SupervisorReap,
        ProbeLayer::HostProtocol => ProbeOracle::HostProtocol,
        ProbeLayer::ResourceLimits => ProbeOracle::ResourceLimit,
    }
}

fn proof_layer_mismatch(os: HostOs, row: &ProbeRecord, expected: ProbeLayer) -> AdmissionError {
    AdmissionError::ProofLayerMismatch {
        os,
        probe: row.probe,
        expected_layer: expected,
        recorded_layer: row.recorded_layer,
        oracle: row.oracle,
        fix: FIX_NO_CHILD,
    }
}

fn layer_mismatch(os: HostOs, rows: &[ProbeRecord]) -> Option<AdmissionError> {
    for row in rows {
        let Some(expected) = expected_layer_for(row.probe) else {
            continue;
        };
        if row.expected_layer != expected || row.recorded_layer != expected {
            return Some(proof_layer_mismatch(os, row, expected));
        }
        if expected == ProbeLayer::OsContainment
            && row.verdict == ProbeVerdict::Pass
            && !row.oracle.admits_os_containment_pass()
        {
            return Some(proof_layer_mismatch(os, row, ProbeLayer::OsContainment));
        }
    }
    None
}

fn incomplete_os_rows(rows: &[ProbeRecord]) -> Option<String> {
    let mut missing = Vec::new();
    for probe in OS_CONTAINMENT_PROBES {
        let passed = rows.iter().any(|row| {
            row.probe == *probe
                && row.verdict == ProbeVerdict::Pass
                && row.recorded_layer == ProbeLayer::OsContainment
                && row.oracle.admits_os_containment_pass()
        });
        if !passed {
            missing.push(*probe);
        }
    }
    if missing.is_empty() {
        None
    } else {
        Some(missing.join(", "))
    }
}

fn hex32(bytes: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for byte in bytes {
        let _ = fmt::Write::write_fmt(&mut out, format_args!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn art(byte: u8) -> ArtifactDigest {
        ArtifactDigest([byte; 32])
    }

    fn prof(byte: u8) -> ProfileDigest {
        ProfileDigest([byte; 32])
    }

    fn id() -> ProofIdentity {
        ProofIdentity {
            os: HostOs::current(),
            artifact_digest: art(1),
            profile_digest: prof(2),
            archive_id: ArchiveId("archive-t1".into()),
        }
    }

    fn strict_req(proof: Option<ProofIdentity>) -> AdmissionRequest {
        AdmissionRequest {
            role: RoleInstance { generation: 7 },
            requested: ProfileState::Strict,
            artifact_digest: art(1),
            profile_digest: prof(2),
            proof,
            electron_sandbox_off: false,
            inherit_from_host: false,
        }
    }

    fn facts_uncontained() -> HostFacts {
        HostFacts::observe_uncontained()
    }

    /// State-machine fixture only. Not an OS sandbox observation.
    fn facts_primitives_present() -> HostFacts {
        HostFacts {
            os: HostOs::current(),
            missing_primitives: Vec::new(),
            policy_generation: CURRENT_POLICY_GENERATION,
            handle_leak: None,
        }
    }

    fn os_pass(probe: &'static str) -> ProbeRecord {
        ProbeRecord {
            probe,
            expected_layer: ProbeLayer::OsContainment,
            recorded_layer: ProbeLayer::OsContainment,
            verdict: ProbeVerdict::Pass,
            oracle: ProbeOracle::OsDeny,
            detail: "fixture".into(),
        }
    }

    fn complete_archive() -> ProofArchive {
        ProofArchive {
            identity: id(),
            rows: OS_CONTAINMENT_PROBES.iter().copied().map(os_pass).collect(),
            policy_generation: CURRENT_POLICY_GENERATION,
        }
    }

    fn assert_code(err: &AdmissionError, code: &str) {
        assert_eq!(err.code(), code);
        let text = err.to_string();
        assert!(text.contains(code), "{text}");
        assert!(
            text.contains("Do not start") || text.contains("do not restart"),
            "fix must forbid unsandboxed start: {text}"
        );
    }

    #[test]
    fn unverified_is_default_and_never_becomes_strict() {
        let req = AdmissionRequest {
            requested: ProfileState::Unverified,
            proof: Some(id()),
            ..strict_req(None)
        };
        let archive = complete_archive();
        assert_eq!(
            admit(&req, &facts_primitives_present(), Some(&archive)),
            Ok(ProfileState::Unverified)
        );
        assert_eq!(ProfileState::Unverified.as_report_str(), "unverified");
    }

    #[test]
    fn legacy_does_not_need_an_archive() {
        let req = AdmissionRequest {
            requested: ProfileState::Legacy,
            proof: None,
            ..strict_req(None)
        };
        assert_eq!(
            admit(&req, &facts_uncontained(), None),
            Ok(ProfileState::Legacy)
        );
        assert_eq!(ProfileState::Legacy.as_report_str(), "legacy");
    }

    #[test]
    fn live_uncontained_facts_never_admit_strict() {
        let req = strict_req(Some(id()));
        let archive = complete_archive();
        match admit(&req, &facts_uncontained(), Some(&archive)) {
            Err(err @ AdmissionError::PrimitiveUnavailable { .. }) => {
                assert_code(&err, "KELD-GUARD008");
                assert_eq!(req.requested, ProfileState::Strict);
            }
            other => panic!("T1 live facts must fail closed, got {other:?}"),
        }
    }

    #[test]
    fn missing_proof_is_guard011() {
        let req = strict_req(None);
        match admit(&req, &facts_primitives_present(), None) {
            Err(err @ AdmissionError::ProofMissing { .. }) => assert_code(&err, "KELD-GUARD011"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn empty_archive_is_guard011() {
        let req = strict_req(Some(id()));
        let archive = ProofArchive {
            identity: id(),
            rows: Vec::new(),
            policy_generation: CURRENT_POLICY_GENERATION,
        };
        match admit(&req, &facts_primitives_present(), Some(&archive)) {
            Err(err @ AdmissionError::ProofMissing { .. }) => assert_code(&err, "KELD-GUARD011"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn profile_digest_mismatch_is_guard013() {
        let mut proof = id();
        proof.profile_digest = prof(9);
        let req = strict_req(Some(proof));
        let archive = complete_archive();
        match admit(&req, &facts_primitives_present(), Some(&archive)) {
            Err(err @ AdmissionError::ProofMismatch { field, .. }) => {
                assert_eq!(field, MismatchField::ProfileDigest);
                assert_code(&err, "KELD-GUARD013");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn artifact_digest_mismatch_is_guard013() {
        let proof = ProofIdentity {
            artifact_digest: art(9),
            ..id()
        };
        let req = AdmissionRequest {
            proof: Some(proof),
            ..strict_req(None)
        };
        let archive = complete_archive();
        match admit(&req, &facts_primitives_present(), Some(&archive)) {
            Err(err @ AdmissionError::ProofMismatch { field, .. }) => {
                assert_eq!(field, MismatchField::ArtifactDigest);
                assert_code(&err, "KELD-GUARD013");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn os_mismatch_is_guard013() {
        let other = match HostOs::current() {
            HostOs::Macos => HostOs::Linux,
            HostOs::Linux | HostOs::Windows => HostOs::Macos,
        };
        let proof = ProofIdentity { os: other, ..id() };
        let req = strict_req(Some(proof));
        let archive = complete_archive();
        match admit(&req, &facts_primitives_present(), Some(&archive)) {
            Err(err @ AdmissionError::ProofMismatch { field, .. }) => {
                assert_eq!(field, MismatchField::Os);
                assert_code(&err, "KELD-GUARD013");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn archive_id_mismatch_is_guard013() {
        let proof = ProofIdentity {
            archive_id: ArchiveId("other-archive".into()),
            ..id()
        };
        let req = strict_req(Some(proof));
        match admit(&req, &facts_primitives_present(), Some(&complete_archive())) {
            Err(err @ AdmissionError::ProofMismatch { field, .. }) => {
                assert_eq!(field, MismatchField::ArchiveId);
                assert_code(&err, "KELD-GUARD013");
                let text = err.to_string();
                assert!(text.contains("archive_id"), "{text}");
                assert!(text.contains("other-archive"), "{text}");
                assert!(text.contains("archive-t1"), "{text}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn handle_leak_is_guard010() {
        let facts = HostFacts {
            os: HostOs::current(),
            missing_primitives: Vec::new(),
            policy_generation: CURRENT_POLICY_GENERATION,
            handle_leak: Some("host stdout".into()),
        };
        match admit(&strict_req(Some(id())), &facts, Some(&complete_archive())) {
            Err(err @ AdmissionError::HandleLeak { .. }) => assert_code(&err, "KELD-GUARD010"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn stale_policy_generation_is_guard012() {
        let req = strict_req(Some(id()));
        let mut archive = complete_archive();
        archive.policy_generation = 0;
        match admit(&req, &facts_primitives_present(), Some(&archive)) {
            Err(err @ AdmissionError::ProofStale { .. }) => assert_code(&err, "KELD-GUARD012"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn protocol_only_archive_is_incomplete_not_strict() {
        let req = strict_req(Some(id()));
        let archive = ProofArchive {
            identity: id(),
            rows: vec![ProbeRecord {
                probe: "protocol_confusion",
                expected_layer: ProbeLayer::HostProtocol,
                recorded_layer: ProbeLayer::HostProtocol,
                verdict: ProbeVerdict::Pass,
                oracle: ProbeOracle::HostProtocol,
                detail: "KELD-IPC-007".into(),
            }],
            policy_generation: CURRENT_POLICY_GENERATION,
        };
        match admit(&req, &facts_primitives_present(), Some(&archive)) {
            Err(err @ AdmissionError::ProofIncomplete { .. }) => {
                let AdmissionError::ProofIncomplete { missing, .. } = &err else {
                    unreachable!("matched ProofIncomplete");
                };
                assert!(missing.contains("direct_network"), "{missing}");
                assert_code(&err, "KELD-GUARD014");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn js_shim_in_os_slot_is_layer_mismatch() {
        let req = strict_req(Some(id()));
        let mut archive = complete_archive();
        archive.rows.retain(|row| row.probe != "direct_network");
        archive.rows.push(ProbeRecord {
            probe: "direct_network",
            expected_layer: ProbeLayer::OsContainment,
            recorded_layer: ProbeLayer::OsContainment,
            verdict: ProbeVerdict::Pass,
            oracle: ProbeOracle::JsShim,
            detail: "guard denied connect".into(),
        });
        match admit(&req, &facts_primitives_present(), Some(&archive)) {
            Err(
                err @ AdmissionError::ProofLayerMismatch {
                    probe,
                    oracle,
                    expected_layer,
                    recorded_layer,
                    ..
                },
            ) => {
                assert_eq!(probe, "direct_network");
                assert_eq!(oracle, ProbeOracle::JsShim);
                assert_eq!(expected_layer, ProbeLayer::OsContainment);
                assert_eq!(recorded_layer, ProbeLayer::OsContainment);
                assert_code(&err, "KELD-GUARD015");
                let text = err.to_string();
                assert!(text.contains("js_shim"), "{text}");
                assert!(text.contains("os_deny"), "{text}");
                assert!(
                    !text.contains("recorded layer `os_containment` instead of `os_containment`"),
                    "Display must name the oracle mismatch, not repeat os_containment: {text}"
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn protocol_oracle_in_os_slot_is_layer_mismatch() {
        let req = strict_req(Some(id()));
        let mut archive = complete_archive();
        for row in &mut archive.rows {
            if row.probe == "direct_network" {
                row.oracle = ProbeOracle::HostProtocol;
            }
        }
        match admit(&req, &facts_primitives_present(), Some(&archive)) {
            Err(
                err @ AdmissionError::ProofLayerMismatch {
                    probe,
                    oracle,
                    expected_layer,
                    recorded_layer,
                    ..
                },
            ) => {
                assert_eq!(probe, "direct_network");
                assert_eq!(oracle, ProbeOracle::HostProtocol);
                assert_eq!(expected_layer, ProbeLayer::OsContainment);
                assert_eq!(recorded_layer, ProbeLayer::OsContainment);
                assert_code(&err, "KELD-GUARD015");
                let text = err.to_string();
                assert!(text.contains("oracle `host_protocol`"), "{text}");
                assert!(text.contains("oracle `os_deny`"), "{text}");
                assert!(
                    !text.contains("recorded layer `os_containment` instead of `os_containment`"),
                    "Display must name the oracle mismatch, not repeat os_containment: {text}"
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn electron_sandbox_off_is_unexpected_grant() {
        let req = AdmissionRequest {
            electron_sandbox_off: true,
            ..strict_req(Some(id()))
        };
        match admit(&req, &facts_primitives_present(), Some(&complete_archive())) {
            Err(err @ AdmissionError::UnexpectedGrant { grant, .. }) => {
                assert_eq!(grant, "electron.appSandbox=off");
                assert_code(&err, "KELD-GUARD009");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn inherit_from_host_is_unexpected_grant() {
        let req = AdmissionRequest {
            inherit_from_host: true,
            ..strict_req(Some(id()))
        };
        match admit(&req, &facts_primitives_present(), Some(&complete_archive())) {
            Err(err @ AdmissionError::UnexpectedGrant { grant, .. }) => {
                assert_eq!(grant, "inherit-from-host");
                assert_code(&err, "KELD-GUARD009");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn complete_fixture_catalog_returns_strict_without_claiming_os() {
        let req = strict_req(Some(id()));
        assert_eq!(
            admit(&req, &facts_primitives_present(), Some(&complete_archive())),
            Ok(ProfileState::Strict)
        );
        assert_eq!(ProfileState::Strict.as_report_str(), "strict");
    }

    #[test]
    fn dropping_one_os_row_fails_incomplete() {
        let req = strict_req(Some(id()));
        let mut archive = complete_archive();
        archive.rows.retain(|row| row.probe != "direct_filesystem");
        match admit(&req, &facts_primitives_present(), Some(&archive)) {
            Err(AdmissionError::ProofIncomplete { missing, .. }) => {
                assert_eq!(missing, "direct_filesystem");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn expected_layer_for_direct_network_is_always_os_containment() {
        assert_eq!(
            expected_layer_for("direct_network"),
            Some(ProbeLayer::OsContainment)
        );
    }
}
