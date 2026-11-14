//! X10 real selected Refused source while another actual Packet occupies VALUE.
use super::{bridge, fresh_healthy, marker, observation, selected};
use crate::PRODUCT_DEADLINE;
use crate::support::original_bridge_run::{OriginalRun, QueuedCloseTrace};
use std::time::Instant;
#[path = "support/original_selected_diagnostic_observation.rs"]
mod diagnostic;

#[test]
#[ignore = "Ramani native lease, qualified instrumented pair and exact selected-diagnostic/healthy projects required"]
#[expect(
    clippy::too_many_lines,
    reason = "Keep physical Packet, selected source, actual Close/role loss, retained failure and fresh healthy barriers visible."
)]
fn windows_original_selected_diagnostic_stays_unadmitted_through_physical_value_and_role_loss() {
    let cli = selected("KELD_ORIGINAL_NATIVE_CLI");
    let project = selected("KELD_ORIGINAL_NATIVE_SELECTED_DIAGNOSTIC_PROJECT");
    let healthy = selected("KELD_ORIGINAL_NATIVE_ALLOWED_PROJECT");
    let barrier = tempfile::Builder::new()
        .prefix("keld-x10-selected-")
        .tempdir()
        .expect("D-only owned control directory");
    let mut run = OriginalRun::start_selected_diagnostic(&cli, &project, barrier.path());
    run.send("OPEN");
    let created = run.record("CREATED ");
    let window = created["id"].as_u64().expect("actual window");
    let incarnation = created["generation"]
        .as_str()
        .expect("captured Host incarnation")
        .parse::<u64>()
        .expect("actual incarnation width");
    let association = run.window_view(window);
    assert_eq!(association["incarnation"].as_u64(), Some(incarnation));
    let view = association["view"].as_u64().expect("actual native view");
    let physical_owners = run.retain_selected_diagnostic_owners();
    let mut trace = QueuedCloseTrace::default();

    // Capture/census is complete before the original Packet deadline starts.
    marker(barrier.path(), "arm");
    run.send("LOAD");
    assert_eq!(run.record("LOAD_REQUESTED ")["id"], window);
    let held = run.queued_close_record(&mut trace, "physical", |value| {
        value["phase"] == "packet-held"
    });
    diagnostic::physical_held(&held);
    let page = run.page_when(view, |page| {
        page["registered"] == true && page["routeReady"] == true && page["issued"] == true
    });
    bridge::assert_first_author_script(&page, &run.hello);
    assert_eq!(page["entered"], 0);
    run.send("COUNTERS");
    assert_eq!(
        run.record("COUNTERS ")["effects"],
        0,
        "held physical packet has not reached real Bun listener"
    );

    run.send("SEND");
    assert_eq!(run.record("SEND_RETURNED ")["id"], window);
    let admitted = run.queued_close_record(&mut trace, "scope", |value| {
        value["phase"] == "event-admitted"
            && value["window"] == window
            && observation::target_in(value, view, 0).is_some()
    });
    let key = diagnostic::event_key(&admitted, &association);
    let post = run.queued_close_record(&mut trace, "owner", |value| {
        value["phase"] == "post-return" && observation::same_event(value, &key)
    });
    assert_eq!(post["native_ok"], true);
    let entered = run.page_when(view, |page| {
        page["entered"] == 1 && page["targetDelivery"] == key.message
    });
    assert_eq!(entered["rawChallenge"], page["rawChallenge"]);
    let full = run.queued_close_record(&mut trace, "selected", |value| {
        value["phase"] == "value-full-restored"
            && value["main_to_renderer"]["message"] == key.message
    });
    let source = diagnostic::selected_full(&full, &key, &held);

    // Canonical async Destroy uses the pre-LOAD Host identity, without its reply.
    // Submission alone is not Close; the actual same-controller fact follows below.
    run.send("DESTROY");
    let requested = run.record("DESTROY_REQUESTED ");
    assert_eq!(requested["id"], window);
    assert_eq!(requested["generation"], created["generation"]);
    let closed = run.queued_close_record(&mut trace, "owner", |value| {
        value["phase"] == "controller-closed" && value["view"] == view
    });
    assert_eq!(closed["actual_closed"], true);
    assert_eq!(closed["native_storage_quiescent"], false);
    let retired = run.queued_close_record(&mut trace, "owner", |value| {
        value["phase"] == "retirement-handed-off" && observation::same_event(value, &key)
    });
    assert_eq!(retired["retirement_sent"], true);
    assert_eq!(retired["retirement"], "outcome-unknown");
    let restored = run.queued_close_record(&mut trace, "selected", |value| {
        value["phase"] == "value-full-restored"
            && value["main_to_renderer"]["message"] == key.message
            && value["main_to_renderer"]["native_retirement"] == "outcome-unknown"
    });
    diagnostic::retained_source(&restored, &key, &source);

    // The fixture's actual child exits itself; the held packet still owns its Box.
    run.send("CRASH");
    let crash = run.record("CRASH_ACK ");
    assert_eq!(crash["bunPid"], run.hello["bunPid"]);
    assert_eq!(crash["effects"], 0);
    run.selected_diagnostic_child_exited();
    let revoked_deadline = Instant::now() + PRODUCT_DEADLINE;
    let revoked = run.queued_close_record_by(
        &mut trace,
        "selected",
        |value| {
            value["phase"] == "role-revoked-before-drain"
                && value["main_to_renderer"]["message"] == key.message
        },
        revoked_deadline,
    );
    if let Some(attempt) = diagnostic::restoration_attempt(&revoked) {
        // Same original absolute read budget; restoration does not wait on drain.
        run.queued_close_record_by(
            &mut trace,
            "selected",
            |value| {
                value["phase"] == "source-restored-after-denied-target"
                    && value["source_attempt"] == attempt
                    && value["selected"] == source
            },
            revoked_deadline,
        );
    }
    let pre_drain_restored = diagnostic::role_revoked(&trace, &revoked, &key, &held, &source);
    // The actual revocation is visible; completion still waits for this physical owner.
    marker(barrier.path(), "release");

    run.finish_selected_diagnostic_failure(physical_owners, |native, control| {
        let (retirement, restored) = diagnostic::complete(native, &key, &held, &source);
        assert!(control.contains("CRASH_ACK "));
        assert!(control.contains("DESTROY_REQUESTED "));
        assert!(!control.contains("DESTROY_RETURNED_ERROR "));
        println!(
            "KELD_ORIGINAL_X10_SELECTED_DIAGNOSTIC {}",
            serde_json::json!({
                "window":key.window,"incarnation":key.incarnation,"view":key.view,
                "navigation":key.navigation,"message":key.message,"selected":source,
                "physicalPacket":held["packet"],"actualClose":closed,"roleRevokedBeforeDrain":revoked,
                "roleRetirementReturn":retirement,
                "preDrainSourceRestoration":pre_drain_restored, "sourceRestoration":restored,
                "normalNativeRelease":false,"expectedUnqualified":true,
            })
        );
    });
    barrier
        .close()
        .expect("fixture control disposal follows physical owner retirement");
    fresh_healthy(&cli, &healthy);
    println!("KELD_ORIGINAL_X10_FRESH_HEALTHY_AFTER_DRAIN case=selected-diagnostic");
}
