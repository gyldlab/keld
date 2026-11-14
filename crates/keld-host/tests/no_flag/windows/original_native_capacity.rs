//! X11 actual joint DATA/four-CONTROL pressure on the original native route.
use super::{bridge, fresh_healthy, marker, selected};
use crate::facade_window::fixture::assert_retired;
use crate::support::original_bridge_run::{OriginalRun, QueuedCloseTrace};
use serde_json::Value;
use std::fs;
use std::time::{Duration, Instant};
#[path = "support/original_native_capacity_observation.rs"]
mod observation;

fn packet_returned(
    run: &mut OriginalRun,
    trace: &mut QueuedCloseTrace,
    held: &Value,
    deadline: Instant,
) -> Value {
    let returned = run.queued_close_record_by(
        trace,
        "physical",
        |value| value["phase"] == "packet-returned",
        deadline,
    );
    observation::physical_written(&returned, held, false);
    let dropped = run.queued_close_record_by(
        trace,
        "physical",
        |value| value["phase"] == "packet-dropped",
        deadline,
    );
    observation::physical_written(&dropped, held, true);
    dropped
}

#[expect(
    clippy::too_many_lines,
    reason = "Keep the actual two finite disposal schedules and opposite-owner barriers visible together."
)]
fn native_capacity_case(writer_first: bool) {
    let cli = selected("KELD_ORIGINAL_NATIVE_CLI");
    let project = selected("KELD_ORIGINAL_NATIVE_CAPACITY_PROJECT");
    let healthy = selected("KELD_ORIGINAL_NATIVE_ALLOWED_PROJECT");
    let barrier = tempfile::Builder::new()
        .prefix("keld-x11-native-capacity-")
        .tempdir()
        .expect("D-only original scheduling controls");
    let physical = barrier.path().join("physical");
    let native = barrier.path().join("native");
    let consumer = barrier.path().join("consumer");
    for directory in [&physical, &native, &consumer] {
        fs::create_dir(directory).expect("fresh owned condition directory");
    }
    // The existing emergency process owner drops before the fixture directories.
    let mut run = OriginalRun::start_native_capacity(&cli, &project, barrier.path());
    run.send("OPEN");
    let window = run.record("CREATED ")["id"]
        .as_u64()
        .expect("actual original window");
    let association = run.window_view(window);
    let view = association["view"].as_u64().expect("actual native view");
    let window_snapshot = run.retain_native_capacity_window();
    let mut trace = QueuedCloseTrace::default();

    marker(&physical, "arm");
    // This independent five-second observation bound starts before LOAD and the
    // actual Packet. It matches the canonical app-link budget; every later condition
    // uses this one deadline, without extending either physical/native product hold.
    let deadline = Instant::now() + Duration::from_secs(5);
    if writer_first {
        run.send(&format!(
            "ARM_CONSUMER {}",
            serde_json::to_string(consumer.to_str().expect("owned fixture path"))
                .expect("bounded path JSON")
        ));
        let armed = run.record("CONSUMER_ARMED ");
        assert_eq!(armed["bunPid"], run.hello["bunPid"]);
        assert_eq!(
            armed["budgetMs"], 5000,
            "unchanged canonical five-second budget"
        );
    }
    run.send("LOAD");
    assert_eq!(run.record("LOAD_REQUESTED ")["id"], window);
    let held = run.queued_close_record_by(
        &mut trace,
        "physical",
        |value| value["phase"] == "packet-held",
        deadline,
    );
    observation::physical_held(&held);
    let initial = run.page_when(view, |page| {
        page["registered"] == true && page["routeReady"] == true && page["issued"] == true
    });
    bridge::assert_first_author_script(&initial, &run.hello);
    assert_eq!(initial["entered"], 0);
    assert!(initial["targetDelivery"].is_null() && initial["terminal"].is_null());
    run.send("COUNTERS");
    assert_eq!(
        run.record("COUNTERS ")["effects"],
        0,
        "real Bun consumer has not entered"
    );

    marker(&native, "arm");
    let native_held = run.queued_close_record_by(
        &mut trace,
        "hold",
        |value| value["phase"] == "held",
        deadline,
    );
    assert_eq!(native_held["mode"], "before-intake");
    run.send("SEND");
    assert_eq!(run.record("SEND_RETURNED ")["id"], window);
    let full = run.queued_close_record_by(
        &mut trace,
        "pressure",
        |value| value["phase"] == "filled",
        deadline,
    );
    observation::filled(&full, &association, &held);
    let both = run.queued_close_record_by(
        &mut trace,
        "pressure",
        |value| {
            observation::controls_held(value, &full)
                && value["state"]["renderer"]["primary_write"] == true
        },
        deadline,
    );
    observation::both_original_sources(&both, &full);
    assert_eq!(both["state"]["main"]["consumer_done"], false);
    assert_eq!(both["state"]["main"]["native_done"], false);
    assert_eq!(both["state"]["renderer"]["consumer_done"], false);

    let first_cut;
    let packet_drop;
    if writer_first {
        marker(&physical, "release");
        packet_drop = packet_returned(&mut run, &mut trace, &held, deadline);
        let entered = run.native_consumer_condition(&consumer, "entered", deadline);
        assert_eq!(
            entered["effects"], 1,
            "genuine synchronous callback is on-stack"
        );
        first_cut = run.queued_close_record_by(
            &mut trace,
            "pressure",
            |value| {
                observation::controls_held(value, &full)
                    && value["state"]["renderer"]["primary_write"] == false
                    && value["state"]["renderer"]["consumer_done"] == false
            },
            deadline,
        );
        observation::both_original_sources(&first_cut, &full);
        assert_eq!(first_cut["state"]["main"]["consumer_done"], false);
        assert_eq!(first_cut["state"]["main"]["native_done"], false);
        assert!(!native.join("release").exists() && !consumer.join("release").exists());
        marker(&native, "release");
    } else {
        marker(&native, "release");
        first_cut = run.queued_close_record_by(
            &mut trace,
            "pressure",
            |value| {
                observation::controls_disposed(value, &full)
                    && value["state"]["renderer"]["primary_write"] == true
                    && value["state"]["renderer"]["consumer_done"] == false
                    && value["value_position"]["phase"] == "writing"
                    && value["value_position"]["serial"] == held["packet"]["serial"]
            },
            deadline,
        );
        observation::both_original_sources(&first_cut, &full);
        assert_eq!(
            first_cut["value_position"]["target_id"],
            held["packet"]["target_id"]
        );
        assert!(
            !physical.join("release").exists(),
            "same actual Packet is still held"
        );
        marker(&physical, "release");
        packet_drop = packet_returned(&mut run, &mut trace, &held, deadline);
    }
    run.queued_close_record_by(
        &mut trace,
        "hold",
        |value| value["phase"] == "released",
        deadline,
    );
    let native_disposed = run.queued_close_record_by(
        &mut trace,
        "pressure",
        |value| observation::controls_disposed(value, &full),
        deadline,
    );
    let native_page = run.page_when(view, |page| {
        page["entered"] == 1
            && page["readyCount"].as_u64() == initial["readyCount"].as_u64().map(|n| n + 4)
    });
    observation::native_page(&native_page, &initial, &full, false);
    if writer_first {
        // Native token disposal and real page observations precede actual callback
        // return. The provider's ordinary ring release/ReleaseConsumed follows it.
        marker(&consumer, "release");
        run.native_consumer_condition(&consumer, "leaving", deadline);
    }
    let final_page = run.page_when(view, |page| page["terminal"].is_object());
    observation::native_page(&final_page, &initial, &full, true);
    let empty = run.queued_close_record_by(
        &mut trace,
        "pressure",
        |value| observation::empty(value, &full),
        deadline,
    );
    assert!(
        Instant::now() < deadline,
        "one original bounded native schedule"
    );
    run.send("COUNTERS");
    assert_eq!(run.record("COUNTERS ")["effects"], 1);
    run.finish_with(|native_trace, control| {
        observation::complete(native_trace, control, &full, &held);
        assert_retired(&window_snapshot);
        println!(
            "KELD_ORIGINAL_X11_NATIVE_CAPACITY {}",
            serde_json::json!({
                "case":if writer_first {"writer-before-native"} else {"native-before-writer"},
                "full":full,"initialBoth":both,"firstDisposalCut":first_cut,
                "physicalPacketDropped":packet_drop,"nativeControlsDisposed":native_disposed,
                "finalZero":empty,"actualPage":final_page,
                "wrongReceiptNativeClaim":false,"freshHealthyRequired":true,
            })
        );
    });
    barrier
        .close()
        .expect("fixture controls disposed only after actual product cleanup");
    fresh_healthy(&cli, &healthy);
    println!(
        "KELD_ORIGINAL_X11_FRESH_HEALTHY case={}",
        if writer_first {
            "writer-before-native"
        } else {
            "native-before-writer"
        }
    );
}

#[test]
#[ignore = "Ramani native custody, qualified exact instrumented pair and pressure/healthy projects required"]
fn windows_original_full_native_control_pressure_releases_native_before_writer() {
    native_capacity_case(false);
}

#[test]
#[ignore = "Ramani native custody, qualified exact instrumented pair and pressure/healthy projects required"]
fn windows_original_full_native_control_pressure_releases_writer_before_native() {
    native_capacity_case(true);
}
