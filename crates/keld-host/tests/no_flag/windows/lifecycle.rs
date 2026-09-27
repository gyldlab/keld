//! Native ordered lifecycle and healthy relaunch contract.

use crate::support::product::ProductFixture;
use crate::support::product_cycle::run_product_cycle;

#[test]
fn windows_no_flag_host_owns_window_two_calls_ordered_quit_and_relaunch() {
    let fixture = ProductFixture::new();
    let first = run_product_cycle(&fixture, "first");
    let second = run_product_cycle(&fixture, "second");

    assert_ne!(
        first.host_pid, second.host_pid,
        "relaunch must be a new host process"
    );
    assert_ne!(
        first.bun_pid, second.bun_pid,
        "relaunch must be a new Bun process"
    );
    assert_ne!(
        first.app_link, second.app_link,
        "relaunch must mint fresh authority"
    );
}
