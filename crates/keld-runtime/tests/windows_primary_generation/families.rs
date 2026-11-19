//! Process-isolated real Bun restart with the production family enrollment hook.

use super::{
    EVENT_DEADLINE, EchoWorker, PrimaryFixture, assert_retired_locator_closed, expect_link_bound,
    expect_provisioned, expect_revoked, expect_spawned, primary_config,
};
use keld_runtime::SupervisorOutcome;
use keld_runtime::primary::{PrimaryRoleRevocationCause, PrimaryRoleSupervisor};
use keld_runtime::windows_job::{WindowsPrimaryFamilies, install_host_death_job};
use std::time::{Duration, Instant};

#[test]
#[ignore = "real Windows process-lifetime Host Job; execute in its own native process"]
fn windows_primary_family_release_covers_real_crash_generations() {
    let host = install_host_death_job().expect("actual Host Job before Bun resources");
    let families = WindowsPrimaryFamilies::new();
    let fixture = PrimaryFixture::new();
    let supervisor = PrimaryRoleSupervisor::start_with_bound_generations(
        primary_config(&fixture).windows_primary_families(families.clone()),
    )
    .expect("real Bun enrolled through the supervisor binding hook");

    let first = expect_provisioned(&supervisor, 1);
    expect_spawned(&supervisor, first, 1);
    let mut old_control = fixture.accept_control();
    let old_link = old_control.read_ready();
    old_control.write_line("BIND");
    let bound = supervisor
        .recv_bound_generation(EVENT_DEADLINE)
        .expect("first authenticated generation");
    expect_link_bound(&supervisor, first, 1);
    assert_eq!(old_control.read_line(), "BOUND");
    let old_echo = EchoWorker::start(bound);
    old_control.write_line("CRASH");
    old_echo.finish();
    expect_revoked(
        &supervisor,
        first,
        1,
        PrimaryRoleRevocationCause::ChildExited,
    );
    assert_retired_locator_closed(&old_link);

    let second = expect_provisioned(&supervisor, 2);
    assert_ne!(first, second);
    expect_spawned(&supervisor, second, 2);
    let mut control = fixture.accept_control();
    let new_link = control.read_ready();
    assert_ne!(old_link, new_link);
    control.write_line("BIND");
    let bound = supervisor
        .recv_bound_generation(EVENT_DEADLINE)
        .expect("replacement authenticated generation");
    expect_link_bound(&supervisor, second, 2);
    assert_eq!(control.read_line(), "BOUND");
    let echo = EchoWorker::start(bound);
    families
        .observe_non_family_exit(&host, Instant::now() + Duration::from_secs(2))
        .expect("real replacement Bun is enrolled before release observation");
    control.write_line("ECHO family-still-live 71");
    assert_eq!(
        control.read_line(),
        "ECHO family-still-live 71",
        "release observation must not terminate or revoke the excluded primary"
    );
    echo.stop();
    supervisor.shutdown();
    expect_revoked(&supervisor, second, 2, PrimaryRoleRevocationCause::Shutdown);
    assert!(matches!(
        supervisor.wait_for_outcome(),
        SupervisorOutcome::Stopped
    ));
    control.expect_closed();
    echo.join();
    let late = PrimaryRoleSupervisor::start_with_bound_generations(
        primary_config(&fixture).windows_primary_families(families),
    );
    assert!(
        late.is_err(),
        "the sealed session cannot launch another primary"
    );
    println!(
        "KELD_T2_PRIMARY_FAMILIES first={first:?} successor={second:?} enrolled=true sealed=true live_echo=true"
    );
}
