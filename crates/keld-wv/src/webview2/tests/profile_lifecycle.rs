//! Native Windows persistent/ephemeral profile lifecycle, isolation and recovery.
//! Includes the exclusively owned private `LocalAppData` test-root fixture helper.

use super::{
    EphemeralProfile, ExclusiveUdfRelease, PROFILE_LAUNCH_DEADLINE, PROFILE_LEASE,
    PROFILE_LIFECYCLE, PROFILE_MARKER, ProfileIdentity, ProfileLifecyclePhase,
    ProfileLifecycleRecord, ProfileProcessIdentity, ProfilePurgePhase, ProfilePurgeRecord,
    WebProfileSelection, dacl_has_untrusted_access, initialize_com_sta, prepare_windows_profile_at,
    prove_exclusive_udf_released, purge_persistent_profile_at, scavenge_ephemeral_profiles,
    try_scavenge_ephemeral_profile, windows_profile_plan,
};

/// KEL-63: the profile must be per-user, not beside the executable.
///
/// The loader default is `<exe>.WebView2\` next to the binary, which a
/// standard user cannot create under `C:\Program Files\`. This asserts we
/// are not on that default and that the directory is user-scoped.
#[test]
fn profile_dir_is_user_scoped_not_next_to_the_exe() {
    let root = std::path::Path::new(r"C:\Users\fixture\AppData\Local");
    let identity = ProfileIdentity::from_host_verified_parts([7; 32], "dev.keld.fixture")
        .expect("verified fixture identity");
    let plan = windows_profile_plan(root, WebProfileSelection::Persistent(identity))
        .expect("persistent profile plan");
    assert!(plan.user_data_dir.starts_with(root));
    assert_eq!(
        plan.user_data_dir,
        root.join("Keld")
            .join("profiles")
            .join("v1")
            .join(identity.namespace_segment())
            .join("webview2")
    );
}

#[test]
fn dev_profile_is_not_the_global_legacy_namespace() {
    let root = std::path::Path::new(r"C:\Users\fixture\AppData\Local");
    let first = EphemeralProfile::from_host_random([1; 32]).expect("first dev launch");
    let second = EphemeralProfile::from_host_random([2; 32]).expect("second dev launch");
    let first =
        windows_profile_plan(root, WebProfileSelection::ephemeral_dev(first)).expect("first plan");
    let second = windows_profile_plan(root, WebProfileSelection::ephemeral_dev(second))
        .expect("second plan");
    assert_ne!(first.control_dir, second.control_dir);
    assert!(
        first
            .control_dir
            .starts_with(root.join("Keld").join("ephemeral").join("v1"))
    );
    assert_eq!(first.user_data_dir, first.control_dir.join("webview2"));
    assert!(!first.control_dir.ends_with("dev.keld"));
}

#[test]
fn native_profile_lease_and_handles_prevent_alias_and_reuse() {
    let root = create_private_test_root("lease");
    let identity = ProfileIdentity::from_host_verified_parts([9; 32], "dev.keld.native-lease")
        .expect("identity");
    let selection = WebProfileSelection::Persistent(identity);
    let first = prepare_windows_profile_at(&root, selection).expect("first owner");
    let second = prepare_windows_profile_at(&root, selection)
        .expect_err("same profile must reject a second native lease");
    assert!(second.to_string().contains("already in use"));
    assert!(
        std::fs::rename(
            &first.plan.control_dir,
            first.plan.control_dir.with_extension("moved")
        )
        .is_err(),
        "retained handles must deny rename/delete substitution"
    );
    drop(first);
    prepare_windows_profile_at(&root, selection)
        .expect_err("dropping a host lease without durable idle must not admit reuse");
    std::fs::remove_dir_all(&root).expect("remove test root");
}

#[test]
fn aliased_control_file_is_rejected_before_lock_or_record_use() {
    let root = create_private_test_root("control-alias");
    let identity =
        ProfileIdentity::from_host_verified_parts([11; 32], "dev.keld.alias").expect("identity");
    let plan =
        windows_profile_plan(&root, WebProfileSelection::Persistent(identity)).expect("plan");
    std::fs::create_dir_all(&plan.control_dir).expect("create control tree");
    std::fs::write(plan.control_dir.join(PROFILE_MARKER), &plan.marker)
        .expect("write marker fixture");
    let outside = root.join("outside.lock");
    std::fs::write(&outside, []).expect("write outside lease target");
    std::fs::hard_link(&outside, plan.control_dir.join(PROFILE_LEASE))
        .expect("create native hard-link alias");

    let error = prepare_windows_profile_at(&root, WebProfileSelection::Persistent(identity))
        .expect_err("aliased lease file must fail");
    assert!(error.to_string().contains("ownership marker"));
    std::fs::remove_dir_all(&root).expect("remove test root");
}

#[test]
fn dead_persistent_owner_is_durably_quarantined_before_recovery() {
    let root = create_private_test_root("lifecycle");
    let identity = ProfileIdentity::from_host_verified_parts([12; 32], "dev.keld.lifecycle")
        .expect("identity");
    let selection = WebProfileSelection::Persistent(identity);
    let initial = prepare_windows_profile_at(&root, selection).expect("initial owner");
    let lifecycle_path = initial.plan.control_dir.join(PROFILE_LIFECYCLE);
    drop(initial);

    let prior = ProfileLifecycleRecord::windows_idle()
        .begin_startup(
            ProfileProcessIdentity::from_host_observation(u32::MAX, u64::MAX)
                .expect("dead fixture owner"),
        )
        .and_then(|record| record.advance(ProfileLifecyclePhase::Running))
        .expect("prior running record");
    std::fs::write(
        &lifecycle_path,
        prior.to_record_bytes().expect("encode prior state"),
    )
    .expect("replace prior state fixture");
    let recovered = prepare_windows_profile_at(&root, selection)
        .expect("dead owner reaches quarantined recovery preparation");
    assert!(recovered.recovery_required);
    assert_eq!(
        recovered.lifecycle.map(|record| record.phase()),
        Some(ProfileLifecyclePhase::Quarantined)
    );
    let durable = ProfileLifecycleRecord::from_windows_record_bytes(
        &std::fs::read(lifecycle_path).expect("read durable quarantine"),
    )
    .expect("decode durable quarantine");
    assert_eq!(durable.phase(), ProfileLifecyclePhase::Quarantined);
    drop(recovered);
    std::fs::remove_dir_all(&root).expect("remove test root");
}

#[test]
fn persistent_purge_is_resumable_and_preserves_control_state() {
    let root = create_private_test_root("purge");
    let identity =
        ProfileIdentity::from_host_verified_parts([16; 32], "dev.keld.purge").expect("identity");
    let selection = WebProfileSelection::Persistent(identity);
    let initial = prepare_windows_profile_at(&root, selection).expect("initial profile");
    let plan = initial.plan.clone();
    drop(initial);
    std::fs::write(
        plan.control_dir.join(PROFILE_LIFECYCLE),
        ProfileLifecycleRecord::windows_idle()
            .to_record_bytes()
            .expect("idle bytes"),
    )
    .expect("write idle fixture");
    std::fs::write(plan.user_data_dir.join("state.bin"), b"owned state")
        .expect("write data fixture");

    purge_persistent_profile_at(&root, identity, true).expect("first purge");
    assert!(!plan.user_data_dir.exists());
    assert!(plan.control_dir.exists());
    assert!(plan.control_dir.join(PROFILE_MARKER).exists());
    assert!(plan.control_dir.join(PROFILE_LIFECYCLE).exists());
    assert!(!plan.control_dir.join(super::super::PROFILE_PURGE).exists());

    purge_persistent_profile_at(&root, identity, true).expect("idempotent purge");
    assert!(!plan.user_data_dir.exists());
    assert!(!plan.control_dir.join(super::super::PROFILE_PURGE).exists());
    std::fs::remove_dir_all(&root).expect("remove test root");
}

#[test]
fn purge_intent_precedes_recovery_and_blocks_normal_startup() {
    let root = create_private_test_root("purge-recovery-order");
    let identity = ProfileIdentity::from_host_verified_parts([20; 32], "dev.keld.purge-recovery")
        .expect("identity");
    let selection = WebProfileSelection::Persistent(identity);
    let initial = prepare_windows_profile_at(&root, selection).expect("initial profile");
    let plan = initial.plan.clone();
    drop(initial);
    let dead = ProfileLifecycleRecord::windows_idle()
        .begin_startup(
            ProfileProcessIdentity::from_host_observation(u32::MAX, u64::MAX).expect("dead owner"),
        )
        .and_then(|record| record.advance(ProfileLifecyclePhase::Running))
        .expect("dead running");
    std::fs::write(
        plan.control_dir.join(PROFILE_LIFECYCLE),
        dead.to_record_bytes().expect("dead bytes"),
    )
    .expect("write dead lifecycle");

    purge_persistent_profile_at(&root, identity, false)
        .expect_err("recovery needs a real event loop");
    let intent = ProfilePurgeRecord::from_record_bytes(
        &std::fs::read(plan.control_dir.join(super::super::PROFILE_PURGE))
            .expect("read committed purge intent"),
    )
    .expect("decode committed purge intent");
    assert_eq!(intent.phase(), ProfilePurgePhase::Prepared);
    let blocked = prepare_windows_profile_at(&root, selection)
        .expect_err("normal startup cannot bypass purge recovery");
    assert!(blocked.to_string().contains("durable profile intent"));
    std::fs::remove_dir_all(&root).expect("remove test root");
}

#[test]
fn scavenger_deletes_only_marker_validated_inactive_leaf() {
    let root = create_private_test_root("scavenge");
    let profile = EphemeralProfile::from_host_random([17; 32]).expect("ephemeral");
    let plan =
        windows_profile_plan(&root, WebProfileSelection::ephemeral_dev(profile)).expect("plan");
    let (control_handles, created) =
        super::super::retain_directory_chain(&root, &plan.control_dir, true, false)
            .expect("create old control through owning helper");
    assert!(created);
    drop(control_handles);
    std::fs::write(plan.control_dir.join(PROFILE_MARKER), &plan.marker).expect("write old marker");
    std::fs::write(plan.control_dir.join(PROFILE_LEASE), []).expect("write old lease");
    assert!(
        try_scavenge_ephemeral_profile(&root, profile, PROFILE_LAUNCH_DEADLINE)
            .expect("scavenge old leaf")
    );
    assert!(!plan.control_dir.exists());

    let corrupt = EphemeralProfile::from_host_random([18; 32]).expect("ephemeral");
    let corrupt_plan = windows_profile_plan(&root, WebProfileSelection::ephemeral_dev(corrupt))
        .expect("corrupt plan");
    let (corrupt_handles, created) =
        super::super::retain_directory_chain(&root, &corrupt_plan.control_dir, true, false)
            .expect("create corrupt control through owning helper");
    assert!(created);
    drop(corrupt_handles);
    std::fs::write(corrupt_plan.control_dir.join(PROFILE_MARKER), b"foreign")
        .expect("write foreign marker");
    std::fs::write(corrupt_plan.control_dir.join(PROFILE_LEASE), []).expect("write corrupt lease");
    assert!(try_scavenge_ephemeral_profile(&root, corrupt, PROFILE_LAUNCH_DEADLINE).is_err());
    assert!(corrupt_plan.control_dir.exists());
    std::fs::remove_dir_all(&root).expect("remove test root");
}

/// A predecessor probe that cannot prove release retains the leaf and
/// never fails the current host's own release, for every probe step and
/// not only the exit wait (architecture 05; KEL-135 review). The injected
/// step is the launch: a zero launch budget expires environment creation
/// at its first pump, before any browser launches. Oracle: the probe
/// outcome is `Unproven` naming the launch deadline, the scavenge pass
/// returns `Ok(0)`, and both leaf directories still exist.
#[test]
fn expired_probe_launch_retains_the_leaf_without_failing_the_scavenge() {
    let _com = initialize_com_sta().expect("STA for the probe");
    let root = create_private_test_root("scavenge-expired-probe-launch");
    let profile = EphemeralProfile::from_host_random([19; 32]).expect("ephemeral");
    let selection = WebProfileSelection::ephemeral_dev(profile);
    let predecessor = prepare_windows_profile_at(&root, selection).expect("predecessor leaf");
    let plan = predecessor.plan.clone();
    match prove_exclusive_udf_released(&predecessor, std::time::Duration::ZERO) {
        ExclusiveUdfRelease::Unproven(error) => assert!(
            error.to_string().contains("launch deadline expired"),
            "the retention must name the launch deadline: {error}"
        ),
        other => panic!("expected an unproven probe, got {other:?}"),
    }
    drop(predecessor);
    assert!(plan.user_data_dir.exists(), "the leaf outlives its owner");

    let removed = scavenge_ephemeral_profiles(&root, std::time::Duration::ZERO)
        .expect("an unproven probe never fails the scavenge pass");
    assert_eq!(removed, 0, "nothing may be deleted on an unproven probe");
    assert!(
        plan.control_dir.exists() && plan.user_data_dir.exists(),
        "the leaf must be retained for a later pass"
    );
    std::fs::remove_dir_all(&root).expect("remove test root");
}

#[test]
fn untrusted_profile_access_is_rejected_without_replacing_engine_aces() {
    use windows_permissions::{LocalBox, SecurityDescriptor};

    let owner_and_engine_access = concat!(
        "D:(A;;FA;;;S-1-5-21-1-2-3-1001)",
        "(A;;FW;;;AC)(A;;FW;;;S-1-15-3-1)(A;;FA;;;SY)(A;;FA;;;BA)"
    )
    .parse::<LocalBox<SecurityDescriptor>>()
    .expect("safe descriptor");
    assert!(!dacl_has_untrusted_access(
        owner_and_engine_access.dacl().expect("safe DACL"),
        "S-1-5-21-1-2-3-1001"
    ));

    for broad_sid in [
        "WD",
        "AU",
        "BU",
        "S-1-5-21-3459511679-531530595-653566084-1006",
    ] {
        for rights in ["FR", "FW", "FX", "FA"] {
            let descriptor = format!("D:(A;;FA;;;S-1-5-21-1-2-3-1001)(A;;{rights};;;{broad_sid})")
                .parse::<LocalBox<SecurityDescriptor>>()
                .expect("broad-access descriptor");
            assert!(
                dacl_has_untrusted_access(
                    descriptor.dacl().expect("broad-access DACL"),
                    "S-1-5-21-1-2-3-1001"
                ),
                "{broad_sid} {rights} access must fail"
            );
        }
    }
}

#[test]
fn reparse_ancestor_is_rejected_before_profile_creation() {
    let root = create_private_test_root("reparse");
    let outside = create_private_test_root("reparse-outside");
    let junction = std::process::Command::new("cmd.exe")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(root.join("Keld"))
        .arg(&outside)
        .output()
        .expect("run junction fixture command");
    assert!(
        junction.status.success(),
        "create directory junction: {}",
        String::from_utf8_lossy(&junction.stderr)
    );
    let identity =
        ProfileIdentity::from_host_verified_parts([10; 32], "dev.keld.reparse").expect("identity");
    let error = prepare_windows_profile_at(&root, WebProfileSelection::Persistent(identity))
        .expect_err("reparse ancestor must fail");
    assert!(error.to_string().contains("ownership marker"));
    std::fs::remove_dir(root.join("Keld")).expect("remove reparse point");
    std::fs::remove_dir(root).expect("remove test LocalAppData");
    std::fs::remove_dir(outside).expect("remove reparse target");
}

#[test]
fn dev_nonce_leaf_is_create_once_even_after_release() {
    let root = create_private_test_root("ephemeral");
    let first = EphemeralProfile::from_host_random([3; 32]).expect("first nonce");
    let selected = prepare_windows_profile_at(&root, WebProfileSelection::ephemeral_dev(first))
        .expect("first dev profile");
    drop(selected);
    prepare_windows_profile_at(&root, WebProfileSelection::ephemeral_dev(first))
        .expect_err("a later launch must not reuse an old dev leaf");
    let second = EphemeralProfile::from_host_random([4; 32]).expect("second nonce");
    prepare_windows_profile_at(&root, WebProfileSelection::ephemeral_dev(second))
        .expect("fresh nonce gets a fresh leaf");
    std::fs::remove_dir_all(&root).expect("remove test root");
}

fn create_private_test_root(label: &str) -> std::path::PathBuf {
    use std::fmt::Write as _;

    let mut nonce = [0_u8; 16];
    getrandom::fill(&mut nonce).expect("test randomness");
    let mut suffix = String::with_capacity(32);
    for byte in nonce {
        write!(&mut suffix, "{byte:02x}").expect("write test suffix");
    }
    let root = super::super::known_local_app_data()
        .expect("resolve test LocalAppData")
        .join("Keld")
        .join("test-fixtures")
        .join(format!("keld-profile-{label}-{suffix}"));
    std::fs::create_dir_all(&root).expect("create test LocalAppData");
    let current_sid = super::super::current_process_sid()
        .expect("resolve test process SID")
        .to_string();
    let grant = format!("*{current_sid}:(OI)(CI)F");
    let output = std::process::Command::new("icacls.exe")
        .arg(&root)
        .args(["/inheritance:r", "/grant:r"])
        .arg(grant)
        .output()
        .expect("run owner-private DACL fixture command");
    assert!(
        output.status.success(),
        "set owner-private fixture DACL: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    root
}
