use crate::support::MARKER;
use crate::support::PROCESS_DEADLINE;
use crate::support::TITLE;
use crate::support::dev_cycle::ShippingDevCycle;
use crate::support::dev_cycle::prepare_keld_dev_helper;
use crate::support::native_window::native_windows;
use crate::support::process::await_process_gone;
use crate::support::process::signal_process_group;
use crate::support::product::ProductFixture;
use crate::support::product::dev_stage_count;
use std::io::Write;
use std::sync::mpsc;
use std::thread;

#[test]
fn shipping_keld_dev_delegates_to_host_and_cli_death_reaps_the_session() {
    let fixture = ProductFixture::new("t2-cli-delegation");
    let _prepared_project = fixture.stage();
    let helper = prepare_keld_dev_helper(&fixture);
    let baseline_stages = dev_stage_count(&fixture.project);

    let mut killed = ShippingDevCycle::launch(&fixture, &helper, "t2-cli");
    let killed_evidence = killed.evidence();
    killed.kill_cli_and_expect_lease_shutdown();
    assert_eq!(dev_stage_count(&fixture.project), baseline_stages);

    let mut signaled = ShippingDevCycle::launch(&fixture, &helper, "t2-cli-sigint");
    let signaled_evidence = signaled.evidence();
    signaled.signal_cli_group_and_expect_lease_shutdown("INT", 2);
    assert_eq!(dev_stage_count(&fixture.project), baseline_stages);

    let mut hung_up = ShippingDevCycle::launch(&fixture, &helper, "t2-cli-sighup");
    let hung_up_evidence = hung_up.evidence();
    hung_up.signal_cli_group_and_expect_lease_shutdown("HUP", 1);
    assert_eq!(dev_stage_count(&fixture.project), baseline_stages);

    let mut failed = ShippingDevCycle::launch(&fixture, &helper, "t2-cli-host-failure");
    let failed_evidence = failed.evidence();
    failed.self_terminate_and_expect_verbatim_error();
    assert_eq!(dev_stage_count(&fixture.project), baseline_stages);

    let stages_before_orderly = dev_stage_count(&fixture.project);
    let mut orderly = ShippingDevCycle::launch(&fixture, &helper, "t2-cli-relaunch");
    assert_eq!(dev_stage_count(&fixture.project), stages_before_orderly + 1);
    let orderly_evidence = orderly.evidence();
    orderly.quit_and_expect_success();
    assert_eq!(dev_stage_count(&fixture.project), stages_before_orderly);

    eprintln!(
        "KEL96_T2_EVIDENCE killed={killed_evidence} sigint={signaled_evidence} sighup={hung_up_evidence} failed={failed_evidence} relaunch={orderly_evidence} marker={MARKER}"
    );
}

#[test]
fn shipping_keld_dev_lease_loss_reaps_the_recovered_generation() {
    let fixture = ProductFixture::new("t3-cli-lease-after-recovery");
    let _prepared_project = fixture.stage();
    let helper = prepare_keld_dev_helper(&fixture);
    let baseline_stages = dev_stage_count(&fixture.project);
    let mut cycle = ShippingDevCycle::launch(&fixture, &helper, "t3-cli-recovery");
    cycle.crash_and_recover();
    cycle.kill_cli_and_expect_recovered_lease_shutdown();
    assert_eq!(dev_stage_count(&fixture.project), baseline_stages);
}

#[test]
fn dev_lease_bytes_are_non_authority_and_only_eof_stops_the_host() {
    let fixture = ProductFixture::new("dev-lease-data");
    let (mut cycle, mut lease_writer) = fixture.launch_leased_cycle("lease-data");
    cycle.assert_live_product();
    let (written_tx, written_rx) = mpsc::channel();
    let writer_thread = thread::spawn(move || {
        let result = lease_writer.write_all(&vec![b'x'; 1024 * 1024]);
        written_tx
            .send((lease_writer, result))
            .expect("return lease writer");
    });
    let (lease_writer, write_result) = match written_rx.recv_timeout(PROCESS_DEADLINE) {
        Ok(result) => result,
        Err(error) => {
            if let Some(host) = cycle.host.as_mut() {
                let _ = host.kill();
                let _ = host.wait();
            }
            let _ = signal_process_group("-KILL", cycle.bun_pid);
            cycle.group_gone = true;
            writer_thread.join().expect("lease writer joins after kill");
            panic!("host did not drain liveness-only bytes: {error}");
        }
    };
    writer_thread.join().expect("lease writer joins");
    write_result.expect("write liveness-only lease bytes");
    cycle
        .control_writer
        .write_all(b"ECHO3\n")
        .expect("request post-data echo");
    cycle.expect_line("ECHO3");

    drop(lease_writer);
    cycle.expect_line("LINK_EOF");
    let output = cycle.wait_host();
    assert!(output.status.success(), "lease-loss shutdown: {output:?}");
    await_process_gone(cycle.bun_pid);
    await_process_gone(cycle.descendant_pid);
    await_process_gone(cycle.guardian_pid);
    assert!(native_windows(cycle.host_pid, TITLE).is_empty());
    cycle.group_gone = true;
}
