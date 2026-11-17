//! Source-bound stage/native-fixture custody shared by ordinary and main-thread proofs.
use super::GUARDIAN_OWNER_REPLY_DEADLINE;
#[cfg(keld_core_native_renderer_main)]
use super::native_observer_sensor as sensor;
use super::native_observer_setup::{NativeSignals, OBSERVATION_BOUND, native_setup};
#[cfg(not(keld_core_native_renderer_main))]
use super::tests::worker_link_e2e::native_observer_e2e::sensor;
use super::{
    FsWorkerTestGate, GuardSnapshot, GuardianOwnerCommand, GuardianOwnerHandle, RecoveryArm,
    load_verified_manifest,
};
#[cfg(not(keld_core_native_renderer_main))]
use super::{PrimaryEchoCall, PrimaryEchoReply, PrimaryRouterHandle};
#[cfg(not(keld_core_native_renderer_main))]
use keld_ipc::codec::{decode, encode};
#[cfg(not(keld_core_native_renderer_main))]
use keld_ipc::echo::{EchoRequest, EchoResponse};
use keld_runtime::macos_guardian::{
    GuardedPrimary, GuardedPrimaryUpdate, GuardianBootstrap, SUPERVISED_GUARDIAN_ARG,
};
use keld_runtime::primary::{
    BoundPrimaryGeneration as BoundRoleGeneration, PrimaryRoleEvent as RoleEvent,
};
use nix::unistd::{Pid, getpgid, getpgrp};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::fs::{self, File};
use std::os::unix::fs::MetadataExt as _;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

pub(super) const SENTINEL: &[u8] = b"unchanged";

/// The Host fixture owns staging, HTTP and project lifetime. Root supplies its receipt.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Inputs {
    pub(super) schema: String,
    pub(super) application: Value,
    pub(super) staged_host: PathBuf,
    pub(super) stage_root: PathBuf,
    pub(super) entry_relative: PathBuf,
    pub(super) allowed_root: PathBuf,
    pub(super) target: PathBuf,
    pub(super) expected_content: Vec<u8>,
    pub(super) outside: PathBuf,
    pub(super) outside_content: Vec<u8>,
    pub(super) outside_identity: ObjectIdentity,
    pub(super) permissions_sha256: String,
    pub(super) staged_host_sha256: String,
    pub(super) entry_sha256: String,
    pub(super) permissions_identity: ObjectIdentity,
    pub(super) staged_host_identity: ObjectIdentity,
    pub(super) entry_identity: ObjectIdentity,
    pub(super) allowed_root_identity: ObjectIdentity,
    pub(super) target_identity: ObjectIdentity,
    pub(super) stage_identity: ObjectIdentity,
    pub(super) transport_identity: ObjectIdentity,
}

#[derive(Deserialize)]
pub(super) struct ObjectIdentity {
    pub(super) path: PathBuf,
    pub(super) dev: u64,
    pub(super) ino: u64,
    pub(super) kind: String,
    pub(super) sha256: Option<String>,
}

impl ObjectIdentity {
    pub(super) fn verify(&self, configured: &Path) {
        assert_eq!(
            fs::canonicalize(configured).expect("canonical supplied object"),
            fs::canonicalize(&self.path).expect("canonical captured object")
        );
        let metadata = fs::symlink_metadata(&self.path).expect("actual supplied object metadata");
        assert!(
            !metadata.file_type().is_symlink(),
            "receipt leaf became a symlink"
        );
        assert_eq!(
            (metadata.dev(), metadata.ino()),
            (self.dev, self.ino),
            "fixture object changed"
        );
        assert!(
            matches!(self.kind.as_str(), "file" | "directory"),
            "unknown receipt object kind"
        );
        assert_eq!(metadata.is_dir(), self.kind == "directory");
        assert_eq!(metadata.is_file(), self.kind == "file");
        if let Some(expected) = &self.sha256 {
            assert_eq!(
                file_digest(&self.path),
                *expected,
                "captured fixture bytes changed"
            );
        } else {
            assert!(metadata.is_dir(), "file receipt lacks captured bytes");
        }
    }
}

pub(super) fn file_digest(path: &Path) -> String {
    format!(
        "{:x}",
        Sha256::digest(fs::read(path).expect("actual supplied object bytes"))
    )
}

pub(super) fn captured_digest(text: &str) -> [u8; 32] {
    assert!(
        text.len() == 64 && text.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "invalid captured digest"
    );
    let mut digest = [0; 32];
    for (index, byte) in digest.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16)
            .expect("captured hexadecimal digest");
    }
    digest
}

impl Inputs {
    pub(super) fn supplied() -> Self {
        let path = std::env::var_os("KELD_KEL143_FIXTURE_RECEIPT")
            .expect("Root must supply the retained Host public-FS fixture receipt");
        let inputs: Self = serde_json::from_slice(&fs::read(path).expect("fixture receipt bytes"))
            .expect("existing Host fixture receipt schema");
        assert_eq!(inputs.schema, "keld.kel143.staged-public-fs-fixture/v1");
        assert_eq!(
            inputs.application,
            json!({"automaticComponent":false,"recoveryInputs":null,"handlerOnlyHold":false})
        );
        assert!(inputs.staged_host.is_absolute() && inputs.stage_root.is_absolute());
        assert!(
            inputs.allowed_root.is_absolute() && inputs.target.starts_with(&inputs.allowed_root)
        );
        assert!(!inputs.entry_relative.is_absolute());
        assert!(!inputs.expected_content.is_empty() && inputs.expected_content != SENTINEL);
        inputs.verify_fixture();
        inputs
    }

    pub(super) fn verify_fixture(&self) {
        let permissions = self.stage_root.join("keld.permissions.jsonc");
        let entry = self.stage_root.join(&self.entry_relative);
        let transport = self.stage_root.join("src/kipc-transport.ts");
        for (identity, path) in [
            (&self.permissions_identity, permissions.as_path()),
            (&self.staged_host_identity, self.staged_host.as_path()),
            (&self.entry_identity, entry.as_path()),
            (&self.allowed_root_identity, self.allowed_root.as_path()),
            (&self.target_identity, self.target.as_path()),
            (&self.stage_identity, self.stage_root.as_path()),
            (&self.transport_identity, transport.as_path()),
            (&self.outside_identity, self.outside.as_path()),
        ] {
            identity.verify(path);
        }
        assert_eq!(
            self.permissions_identity.sha256.as_deref(),
            Some(self.permissions_sha256.as_str())
        );
        assert_eq!(
            self.staged_host_identity.sha256.as_deref(),
            Some(self.staged_host_sha256.as_str())
        );
        assert_eq!(
            self.entry_identity.sha256.as_deref(),
            Some(self.entry_sha256.as_str())
        );
    }

    pub(super) fn prepare_policy(&self) -> GuardSnapshot {
        sensor::calibrate_directory_sensor(&self.allowed_root);
        sensor::calibrate_object_sensor(&self.target);
        let path = self.stage_root.join("keld.permissions.jsonc");
        let digest = captured_digest(&self.permissions_sha256);
        let verified =
            load_verified_manifest(File::open(&path).expect("staged policy"), path, digest)
                .expect("existing verified manifest owner");
        let snapshot =
            GuardSnapshot::prepare(verified).expect("existing native initial preparation");
        assert!(
            sensor::directory_handle_count(&self.allowed_root) > 0,
            "prepared roots absent"
        );
        snapshot
    }

    pub(super) fn spawn_guardian(&self) -> GuardedPrimary {
        let entry = fs::metadata(self.stage_root.join(&self.entry_relative)).expect("staged entry");
        let mut command = Command::new(&self.staged_host);
        command
            .arg(SUPERVISED_GUARDIAN_ARG)
            .arg(&self.stage_root)
            .arg(&self.entry_relative)
            .arg(entry.dev().to_string())
            .arg(entry.ino().to_string())
            .arg("0") // existing staged public-FS fixture is renderer-declared
            .env_remove("KELD_DEV_LEASE")
            .stderr(Stdio::inherit())
            .process_group(0);
        // spawn_supervised owns the dedicated ACK stdout; the test never reads it.
        GuardianBootstrap::spawn_supervised(command)
            .expect("retained real guardian Child")
            .register_guarded_primary_until(Instant::now() + OBSERVATION_BOUND)
            .expect("actual supervised guardian registration")
    }
}

/// Reuses one held Native scope and the existing post-take gate in both owner cases.
pub(super) struct NativeWorkerSetup {
    pub(super) signals: NativeSignals,
    pub(super) gate: Arc<FsWorkerTestGate>,
    pub(super) gate_release: Sender<()>,
    pub(super) gate_taken: Receiver<()>,
}

pub(super) fn native_worker_setup() -> NativeWorkerSetup {
    let (setup, signals) = native_setup(true, false);
    let (taken, gate_taken) = mpsc::sync_channel(8);
    let (gate_release, release) = mpsc::channel();
    let mut gate = FsWorkerTestGate {
        taken,
        release: Mutex::new(release),
        before_entry: None,
        initializer: Mutex::new(None),
    };
    setup(&mut gate);
    // One G1 write, G2 write/read, and G2 denial. Releases never establish order.
    for _ in 0..4 {
        gate_release
            .send(())
            .expect("existing post-take gate release");
    }
    NativeWorkerSetup {
        signals,
        gate: Arc::new(gate),
        gate_release,
        gate_taken,
    }
}

pub(super) fn next_bound(guardian: &mut GuardedPrimary, expected: u32) -> BoundRoleGeneration {
    let deadline = Instant::now() + OBSERVATION_BOUND;
    loop {
        assert!(
            Instant::now() < deadline,
            "actual bound generation {expected} absent"
        );
        guardian
            .poll_fatal()
            .expect("retained guardian/control health");
        match guardian.recv_update(Duration::from_millis(10)) {
            Some(GuardedPrimaryUpdate::Bound(bound)) => {
                assert_eq!(bound.attempt(), expected);
                return bound;
            }
            Some(GuardedPrimaryUpdate::Role(event)) => {
                println!("KELD_KEL143_REAL_EVENT {event:?}");
                assert!(
                    !matches!(event, RoleEvent::Revoked { .. }),
                    "initial role revoked"
                );
            }
            None => {}
        }
    }
}

pub(super) fn assert_owned_groups(guardian: &GuardedPrimary, bun: u32) {
    let guardian_pid =
        Pid::from_raw(i32::try_from(guardian.guardian_pid()).expect("guardian PID fits"));
    let bun_pid = Pid::from_raw(i32::try_from(bun).expect("Bun PID fits"));
    assert_eq!(getpgid(Some(guardian_pid)), Ok(guardian_pid));
    assert_eq!(getpgid(Some(bun_pid)), Ok(bun_pid));
    assert_ne!(
        guardian_pid,
        getpgrp(),
        "guardian cannot enter the hostile host group"
    );
    assert_ne!(bun_pid, guardian_pid);
    assert_parent(guardian.guardian_pid(), std::process::id());
    assert_parent(bun, guardian.guardian_pid());
}

fn assert_parent(pid: u32, expected: u32) {
    // Read-only identity prerequisite, inside the existing exact-self45s watchdog.
    let output = Command::new("/bin/ps")
        .args(["-p", &pid.to_string(), "-o", "ppid="])
        .output()
        .expect("actual retained-family parent census");
    assert!(
        output.status.success() && output.stderr.is_empty(),
        "parent census unavailable: {output:?}"
    );
    let parent: u32 = std::str::from_utf8(&output.stdout)
        .expect("ASCII parent census")
        .trim()
        .parse()
        .expect("one exact parent identity");
    assert_eq!(parent, expected, "actual guardian/Bun parent ownership");
}

/// The baton has at most two arm commands outstanding; the separate failure probe
/// can add one test-issued command. Queue next arm before consuming the prior receipt.
/// The actor's real reply send fences successive normal turns without an idle timeout.
/// Dropping this receiver unblocks an actor reply before fixture fallback joins it.
pub(super) struct ActorTurns {
    pub(super) owner: GuardianOwnerHandle,
    pub(super) receipt: Receiver<Result<(), String>>,
    pub(super) completed: usize,
}

impl ActorTurns {
    fn queue(owner: &GuardianOwnerHandle) -> Receiver<Result<(), String>> {
        let (reply, receipt) = mpsc::sync_channel(0);
        owner
            .command_tx
            .send(GuardianOwnerCommand::ArmRecovery(
                RecoveryArm::Initial(1),
                reply,
            ))
            .expect("queue existing duplicate arm command");
        receipt
    }

    pub(super) fn start(owner: GuardianOwnerHandle) -> Self {
        let receipt = Self::queue(&owner);
        Self {
            owner,
            receipt,
            completed: 0,
        }
    }

    pub(super) fn advance(&mut self) {
        let next = Self::queue(&self.owner);
        let prior = std::mem::replace(&mut self.receipt, next);
        prior
            .recv_timeout(GUARDIAN_OWNER_REPLY_DEADLINE)
            .expect("actual owner command reply while Native held/pending")
            .expect("actual duplicate arm receipt");
        self.completed += 1;
    }
}

#[cfg(not(keld_core_native_renderer_main))]
pub(super) fn roundtrip(handle: &PrimaryRouterHandle, nonce: &str) -> PrimaryEchoCall {
    public_call(handle, nonce, "roundtrip", 1)
}

#[cfg(not(keld_core_native_renderer_main))]
pub(super) fn public_call(
    handle: &PrimaryRouterHandle,
    nonce: &str,
    action: &str,
    count: u32,
) -> PrimaryEchoCall {
    handle
        .begin_echo_call(
            &encode(&EchoRequest {
                message: json!({"action":action, "nonce": nonce}).to_string(),
                count,
            })
            .expect("owning Echo request codec"),
        )
        .expect("actual primary/public API call")
}

#[cfg(not(keld_core_native_renderer_main))]
pub(super) fn public_outcome(call: &PrimaryEchoCall) -> Value {
    public_outcome_count(call, 1)
}

#[cfg(not(keld_core_native_renderer_main))]
pub(super) fn public_outcome_count(call: &PrimaryEchoCall, count: u32) -> Value {
    let reply = call
        .reply
        .recv_timeout(Duration::from_secs(6))
        .expect("actual public outcome")
        .expect("host Echo path remains live");
    let PrimaryEchoReply::Reply(bytes) = reply else {
        panic!("unexpected public handler result {reply:?}");
    };
    let response: EchoResponse = decode(&bytes).expect("owning Echo response codec");
    assert_eq!(response.count, count);
    serde_json::from_str(&response.message).expect("public FS sample JSON")
}
