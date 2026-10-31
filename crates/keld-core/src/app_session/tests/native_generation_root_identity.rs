//! Literal AC4 G2 retained-object and supported anchor-symlink scenarios.
//! Existing direct real-worker/Runtime fixture; no actor/window/document claim.
use super::anchor_failure::AnchorAbsence;
use super::{
    Fixture, FsRecoveryAttempt, OBSERVATION_BOUND, SENTINEL, public_call, public_outcome,
    public_outcome_count, roundtrip, sensor, support,
};
use serde_json::json;
use std::fs;
use std::io;
use std::os::unix::fs::{MetadataExt as _, symlink};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const SELECTED_BEFORE: &[u8] = b"g2-selected-before";
const AMBIENT_DECOY: &[u8] = b"g2-ambient-decoy";
type Identity = (u64, u64);

fn identity(path: &Path) -> Identity {
    let metadata = fs::metadata(path).expect("actual filesystem object identity");
    (metadata.dev(), metadata.ino())
}

/// Owns only the existing fixture's namespace entries; it retains no descriptor.
struct RootNamespace {
    anchor: PathBuf,
    relative: PathBuf,
    ambient: Option<Identity>,
    selected: Option<TempDir>,
    old: Option<AnchorAbsence>,
}

impl RootNamespace {
    fn after_old_drop(fixture: &Fixture) -> Self {
        let anchor = fixture.inputs.allowed_root.clone();
        let relative = fixture
            .inputs
            .target
            .strip_prefix(&anchor)
            .expect("receipt target beneath unchanged literal anchor")
            .to_owned();
        let old = AnchorAbsence::remove(&anchor, &fixture.inputs.target);
        assert_eq!(sensor::directory_handle_count(old.renamed_root()), 0);
        assert_eq!(sensor::object_handle_count(old.renamed_target()), 0);
        let selected = tempfile::Builder::new()
            .prefix("kel143-g2-root-")
            .tempdir_in(anchor.parent().expect("producer-owned namespace parent"))
            .expect("owned distinct G2 object path");
        Self {
            anchor,
            relative,
            ambient: None,
            selected: Some(selected),
            old: Some(old),
        }
    }

    fn selected_root(&self) -> &Path {
        self.selected
            .as_ref()
            .expect("owned selected namespace")
            .path()
    }

    fn selected_target(&self) -> PathBuf {
        self.selected_root().join(&self.relative)
    }
    fn old_root(&self) -> &Path {
        self.old.as_ref().expect("owned old anchor").renamed_root()
    }
    fn old_target(&self) -> &Path {
        self.old
            .as_ref()
            .expect("owned old anchor")
            .renamed_target()
    }

    fn capture_ambient(&mut self) {
        let metadata = fs::symlink_metadata(&self.anchor).expect("test-created ambient entry");
        assert!(metadata.is_dir() || metadata.file_type().is_symlink());
        self.ambient = Some((metadata.dev(), metadata.ino()));
    }

    fn install_directory(&mut self) {
        fs::create_dir(&self.anchor).expect("distinct replacement at unchanged literal anchor");
        self.capture_ambient();
        fs::write(self.anchor.join(&self.relative), SELECTED_BEFORE)
            .expect("seed new G2 file object");
    }

    fn swap_prepared_directory(&mut self) {
        fs::remove_dir(self.selected_root()).expect("vacant owned rename destination");
        fs::rename(&self.anchor, self.selected_root()).expect("rename already prepared G2 root");
        self.ambient = None;
        fs::create_dir(&self.anchor).expect("new ambient decoy at original spelling");
        self.capture_ambient();
        fs::write(self.anchor.join(&self.relative), AMBIENT_DECOY)
            .expect("seed independent ambient decoy file");
    }

    fn install_anchor_alias(&mut self) {
        fs::write(self.selected_target(), SELECTED_BEFORE).expect("seed selected alias-root file");
        symlink(self.selected_root(), &self.anchor).expect("supported alias at the grant anchor");
        self.capture_ambient();
        assert_eq!(
            fs::read_link(&self.anchor).expect("actual anchor alias target"),
            self.selected_root()
        );
        assert!(
            fs::symlink_metadata(&self.anchor)
                .expect("anchor lstat")
                .file_type()
                .is_symlink()
        );
        assert_eq!(identity(&self.anchor), identity(self.selected_root()));
    }

    fn clear_ambient(&mut self) -> io::Result<()> {
        let Some(expected) = self.ambient else {
            return Ok(());
        };
        let metadata = fs::symlink_metadata(&self.anchor)?;
        if (metadata.dev(), metadata.ino()) != expected {
            return Err(io::Error::other(
                "owned ambient entry was replaced before cleanup",
            ));
        }
        if metadata.file_type().is_symlink() {
            fs::remove_file(&self.anchor)?;
        } else {
            fs::remove_dir_all(&self.anchor)?;
        }
        self.ambient = None;
        Ok(())
    }

    fn restore_after_cleanup(mut self) {
        self.clear_ambient()
            .expect("remove only test-created ambient entry after product cleanup");
        self.selected
            .take()
            .expect("one selected namespace owner")
            .close()
            .expect("remove selected object after handle census zero");
        self.old
            .take()
            .expect("one original namespace owner")
            .restore();
    }
}

impl Drop for RootNamespace {
    fn drop(&mut self) {
        if let Err(error) = self.clear_ambient() {
            eprintln!("KELD_KEL143_AC4_ROOT_NAMESPACE_CLEANUP_FAILED {error}");
        }
        // Fields drop selected storage, then existing original-anchor custody.
        // This is unwind cleanup only; success requires census and finish first.
    }
}

fn at_fresh_prepare_cut(fixture: &mut Fixture, nonce: &str) -> FsRecoveryAttempt {
    fixture.ready();
    let _old = roundtrip(&fixture.handle(), nonce);
    fixture
        .gate_taken
        .recv_timeout(OBSERVATION_BOUND)
        .expect("actual G1 work taken");
    let held = fixture
        .signals
        .held
        .recv_timeout(OBSERVATION_BOUND)
        .expect("actual Native after-walk hold");
    assert!(!held.fixture_fault);
    assert_eq!((held.request_entries, held.request_returns), (1, 0));
    assert!(held.walk_entries > 0 && held.target_opens > 0);
    fixture.assert_native_held();
    let mut recovery = fixture.retire_while_held();
    fixture.return_and_observe_old_drop(&mut recovery);
    recovery
}

fn allowed_roundtrip(fixture: &Fixture, nonce: &str) {
    let outcome = public_outcome(&roundtrip(&fixture.handle(), nonce));
    assert_eq!(
        outcome,
        json!({"ok":true,"bytes":fixture.inputs.expected_content})
    );
}

fn denied_no_effect(fixture: &Fixture, nonce: &str) {
    let before_identity = identity(&fixture.inputs.outside);
    let denied = public_call(&fixture.handle(), nonce, "deny", 2);
    let outcome = public_outcome_count(&denied, 2);
    assert_eq!(outcome["ok"], false);
    assert_eq!(outcome["code"], "KELD-GUARD002");
    assert_eq!(
        fs::read(&fixture.inputs.outside).expect("independent denied sentinel"),
        fixture.inputs.outside_content
    );
    assert_eq!(identity(&fixture.inputs.outside), before_identity);
}

fn assert_selected_effect(
    fixture: &Fixture,
    namespace: &RootNamespace,
    root: Identity,
    file: Identity,
) {
    assert_eq!(
        fs::read(namespace.selected_target()).expect("independent selected G2 bytes"),
        fixture.inputs.expected_content
    );
    assert_eq!(identity(namespace.selected_root()), root);
    assert_eq!(identity(&namespace.selected_target()), file);
    assert!(sensor::directory_handle_count(namespace.selected_root()) > 0);
    assert_eq!(sensor::object_handle_count(&namespace.selected_target()), 0);
}

fn assert_old_untouched(namespace: &RootNamespace, root: Identity, file: Identity) {
    assert_eq!(
        fs::read(namespace.old_target()).expect("old G1 bytes"),
        SENTINEL
    );
    assert_eq!(identity(namespace.old_root()), root);
    assert_eq!(identity(namespace.old_target()), file);
    assert_eq!(sensor::directory_handle_count(namespace.old_root()), 0);
    assert_eq!(sensor::object_handle_count(namespace.old_target()), 0);
}

#[test]
#[ignore = "explicit physical macOS source-bound G2 post-prepare namespace swap lane"]
fn prepared_successor_stays_bound_after_second_anchor_replacement() {
    const CASE: &str = "guardian-g2-post-prepare-swap";
    const SELECTOR: &str = "app_session::tests::worker_link_e2e::native_observer_e2e::native_generation_recovery::root_identity::prepared_successor_stays_bound_after_second_anchor_replacement";
    if !support::is_native_child(CASE) {
        support::isolated_guardian_case(CASE, SELECTOR);
        return;
    }
    let mut fixture = Fixture::start();
    assert_ne!(fixture.inputs.expected_content, SELECTED_BEFORE);
    assert_ne!(fixture.inputs.expected_content, AMBIENT_DECOY);
    let mut recovery = at_fresh_prepare_cut(&mut fixture, "00000000000000000000000000005143");
    let mut namespace = RootNamespace::after_old_drop(&fixture);
    let old_root = identity(namespace.old_root());
    let old_file = identity(namespace.old_target());
    namespace.install_directory();
    let selected_root = identity(&fixture.inputs.allowed_root);
    let selected_file = identity(&fixture.inputs.target);
    assert_ne!(selected_root, old_root);
    assert_ne!(selected_file, old_file);
    sensor::calibrate_directory_sensor(&fixture.inputs.allowed_root);
    sensor::calibrate_object_sensor(&fixture.inputs.target);
    fixture.prepare_release_and_bind(&mut recovery);
    assert!(fixture.handle().is_current(2));
    assert!(sensor::directory_handle_count(&fixture.inputs.allowed_root) > 0);
    namespace.swap_prepared_directory();
    let decoy_root = identity(&fixture.inputs.allowed_root);
    let decoy_file = identity(&fixture.inputs.target);
    assert_eq!(identity(namespace.selected_root()), selected_root);
    assert_eq!(identity(&namespace.selected_target()), selected_file);
    assert_ne!(decoy_root, selected_root);
    assert_ne!(decoy_file, selected_file);
    assert!(sensor::directory_handle_count(namespace.selected_root()) > 0);
    assert_eq!(
        sensor::directory_handle_count(&fixture.inputs.allowed_root),
        0
    );
    assert_eq!(sensor::directory_handle_count(namespace.old_root()), 0);
    allowed_roundtrip(&fixture, "00000000000000000000000000005243");
    assert_selected_effect(&fixture, &namespace, selected_root, selected_file);
    assert_eq!(
        fs::read(&fixture.inputs.target).expect("independent ambient decoy"),
        AMBIENT_DECOY
    );
    assert_eq!(identity(&fixture.inputs.target), decoy_file);
    assert_old_untouched(&namespace, old_root, old_file);
    denied_no_effect(&fixture, "00000000000000000000000000005244");
    println!(
        "KELD_KEL143_AC4_G2_SWAP old_root={old_root:?} old_file={old_file:?} selected_root={selected_root:?} selected_file={selected_file:?} decoy_root={decoy_root:?} decoy_file={decoy_file:?} actual_g2_effect_only_selected=true"
    );
    fixture.finish();
    assert_eq!(sensor::directory_handle_count(namespace.selected_root()), 0);
    assert_eq!(sensor::object_handle_count(&namespace.selected_target()), 0);
    assert_eq!(sensor::directory_handle_count(namespace.old_root()), 0);
    namespace.restore_after_cleanup();
    println!(
        "KELD_AC7_CASE_DONE case={CASE} retained_g2_root=true roots=0 window_document_claim=false"
    );
}

#[test]
#[ignore = "explicit physical macOS source-bound supported grant-anchor symlink lane"]
fn fresh_successor_selects_complete_object_through_supported_anchor_alias() {
    const CASE: &str = "guardian-g2-anchor-symlink";
    const SELECTOR: &str = "app_session::tests::worker_link_e2e::native_observer_e2e::native_generation_recovery::root_identity::fresh_successor_selects_complete_object_through_supported_anchor_alias";
    if !support::is_native_child(CASE) {
        support::isolated_guardian_case(CASE, SELECTOR);
        return;
    }
    let mut fixture = Fixture::start();
    assert_ne!(fixture.inputs.expected_content, SELECTED_BEFORE);
    let mut recovery = at_fresh_prepare_cut(&mut fixture, "00000000000000000000000000006143");
    let mut namespace = RootNamespace::after_old_drop(&fixture);
    let old_root = identity(namespace.old_root());
    let old_file = identity(namespace.old_target());
    namespace.install_anchor_alias();
    let selected_root = identity(namespace.selected_root());
    let selected_file = identity(&namespace.selected_target());
    let alias_identity = namespace.ambient.expect("actual anchor symlink identity");
    assert_ne!(selected_root, old_root);
    assert_ne!(selected_file, old_file);
    sensor::calibrate_directory_sensor(namespace.selected_root());
    sensor::calibrate_object_sensor(&namespace.selected_target());
    fixture.prepare_release_and_bind(&mut recovery);
    assert!(fixture.handle().is_current(2));
    assert!(sensor::directory_handle_count(namespace.selected_root()) > 0);
    assert_eq!(sensor::directory_handle_count(namespace.old_root()), 0);
    allowed_roundtrip(&fixture, "00000000000000000000000000006243");
    assert_selected_effect(&fixture, &namespace, selected_root, selected_file);
    let alias = fs::symlink_metadata(&fixture.inputs.allowed_root)
        .expect("same anchor alias after G2 effect");
    assert!(alias.file_type().is_symlink());
    assert_eq!((alias.dev(), alias.ino()), alias_identity);
    assert_eq!(
        fs::read_link(&fixture.inputs.allowed_root).expect("same grant-anchor target"),
        namespace.selected_root()
    );
    assert_old_untouched(&namespace, old_root, old_file);
    denied_no_effect(&fixture, "00000000000000000000000000006244");
    println!(
        "KELD_KEL143_AC4_ANCHOR_ALIAS alias={alias_identity:?} old_root={old_root:?} old_file={old_file:?} selected_root={selected_root:?} selected_file={selected_file:?} selected_complete_object=true"
    );
    fixture.finish();
    assert_eq!(sensor::directory_handle_count(namespace.selected_root()), 0);
    assert_eq!(sensor::object_handle_count(&namespace.selected_target()), 0);
    assert_eq!(sensor::directory_handle_count(namespace.old_root()), 0);
    namespace.restore_after_cleanup();
    println!(
        "KELD_AC7_CASE_DONE case={CASE} supported_anchor_alias=true roots=0 window_document_claim=false"
    );
}
