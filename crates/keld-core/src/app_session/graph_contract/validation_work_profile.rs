//! Ignored data-only timing of the actual Host graph/source preflight owners.
//! No Host/Guardian/Bun/window launch, original evaluation, lock or user-data mutation.
use std::cell::Cell;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

thread_local! {
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn start() -> Option<Instant> {
    ACTIVE.with(|active| active.get().then(Instant::now))
}

pub(super) fn finish(
    phase: &str,
    started: Option<Instant>,
    files: Option<usize>,
    bytes: Option<u64>,
    directories: Option<usize>,
) {
    if let Some(started) = started {
        eprintln!(
            "KELD_GRAPH_WORK {}",
            serde_json::json!({
                "phase": phase,
                "elapsed_ns": started.elapsed().as_nanos().to_string(),
                "files": files,
                "bytes": bytes,
                "directories": directories,
                "status": "completed",
            })
        );
    }
}

struct ActiveProfile;
impl Drop for ActiveProfile {
    fn drop(&mut self) {
        ACTIVE.with(|active| active.set(false));
    }
}

fn resources() -> [u32; 3] {
    use super::super::{CHILD_ATTEMPTS, LISTENER_ATTEMPTS, WINDOW_ATTEMPTS};
    [
        LISTENER_ATTEMPTS.load(Ordering::Acquire),
        CHILD_ATTEMPTS.load(Ordering::Acquire),
        WINDOW_ATTEMPTS.load(Ordering::Acquire),
    ]
}

#[test]
#[ignore = "Root-selected retained stage data-only profile; never launches original or Host"]
fn retained_stage_host_graph_and_original_source_work_profile() {
    let root = PathBuf::from(
        std::env::var_os("KELD_TEST_GRAPH_PROFILE_STAGE")
            .expect("Root-selected retained stage, no application selector"),
    );
    let before = resources();
    ACTIVE.with(|active| active.set(true));
    let _active = ActiveProfile;
    let boot_started = start();
    // This is the actual Host's no-flag validated selection, including its
    // descriptor-selected graph digest and retained root, not a second validator.
    let mut selected =
        super::super::validate_from_root(&root).expect("actual retained Host boot prevalidation");
    finish("host.validate_from_root", boot_started, None, None, None);
    let graph = Arc::new(
        selected
            .app
            .graph
            .take()
            .expect("schema5 actual admitted graph"),
    );
    let files = graph.graph.files.len();
    let bytes = graph
        .graph
        .files
        .iter()
        .try_fold(0_u64, |sum, row| sum.checked_add(row.size))
        .expect("actual profile byte count is representable");
    eprintln!(
        "KELD_GRAPH_WORK {}",
        serde_json::json!({
            "phase":"artifact", "root":root, "entry":graph.graph.entry,
            "graph_sha256":super::sha_text(&graph.graph_sha256()),
            "files":files,"bytes":bytes,"debug_assertions":cfg!(debug_assertions),
            "profile":"diagnostic same-process API timing; no cache flush or percentile claim",
        })
    );
    let verified = keld_guard::verified_manifest::load_verified_manifest(
        selected
            .permissions_file
            .try_clone()
            .expect("same captured permissions handle"),
        selected.app.root.join(super::super::PERMISSIONS_FILE),
        selected.permissions_digest,
    )
    .expect("same actual compiled permissions digest");
    assert_eq!(
        verified.manifest().declared_profile(),
        Some(keld_guard::DeclaredProfile::Legacy)
    );
    let policy = verified
        .resolve_unique_window_csp(keld_guard::WindowCspProfile::OriginalEditorOffline)
        .expect("actual selected original policy");
    let started = start();
    // Same original-source owner called after Legacy/lock and before Guardian.
    // Its current full graph recheck and native source metadata construction
    // run unchanged; this test never acquires the real user's single-instance lease.
    let prepared = super::super::original_webapp::PreparedOriginalSource::prepare(
        Arc::clone(&graph),
        keld_guard::ProfileState::Legacy,
        policy,
        keld_wv::wkwebview::AppPreloadBudget::default(),
    )
    .expect("actual original-source preparation as data only");
    finish(
        "host.PreparedOriginalSource.prepare",
        started,
        Some(files),
        Some(bytes),
        None,
    );
    assert_eq!(prepared.graph_sha256(), graph.graph_sha256());
    let after = resources();
    assert_eq!(
        after, before,
        "data-only profile must not create listener/child/window resources"
    );
    eprintln!(
        "KELD_GRAPH_WORK {}",
        serde_json::json!({"phase":"resources","before":before,"after":after,"single_instance_attempts":"not invoked","stage_cleanup":"not owned; retained unchanged"})
    );
}
