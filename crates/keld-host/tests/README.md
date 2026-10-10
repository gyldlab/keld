# Host acceptance test map

These files exercise the shipping host; they are not its implementation. The
[testing playbook](../../../.agents/testing.md) owns proof rules and
[test-layout.md](../../../.agents/test-layout.md) owns placement, extraction, discovery,
and resource-locality rules.

Start from the failing test or contract name and read that scenario plus the narrow
support it imports. This map is a navigation aid, not a second registry of every test.

## Current navigation

| Contract | Current source |
| --- | --- |
| Diagnostic flags | `host_flags.rs` |
| Linux staging and boot admission | `no_flag/linux/staging.rs`, `no_flag/linux/support/{project,stage}.rs` |
| Linux shipping lifecycle and process evidence | `no_flag/linux/lifecycle.rs`, `no_flag/linux/support/process.rs` |
| Linux recovery | `no_flag/linux/recovery.rs` |
| Linux `keld dev` / host-death lifecycle | `no_flag/linux/dev_lifecycle.rs` |
| Linux control and renderer fixtures | `no_flag/linux/support/{control,renderer}.rs` |
| macOS boot/startup admission | `no_flag/macos/{boot_admission,startup_rollback}.rs`, `no_flag/macos/support/admission.rs` |
| macOS shipping/dev lifecycle and recovery | `no_flag/macos/{lifecycle,dev_lifecycle,recovery}.rs`, `no_flag/macos/support/{product,dev_cycle,recovery_cycle}.rs` |
| macOS descriptor/window observations | `no_flag/macos/{descriptor_attribution,descriptor_liveness}.rs`, `no_flag/macos/support/{unix_descriptors,lease_descriptors,native_window}.rs` |
| macOS profile identity/storage/purge/multi-user/reboot | `no_flag/macos/profiles/**`, `no_flag/macos/support/{profile_evidence,profile_origin,profile_renderer,cross_user,signed_app}.rs` |
| macOS media acceptance | `no_flag/macos/media/**` |
| macOS child-output/lifetime controls | `no_flag/macos/wait_capture.rs`, `no_flag/macos/support/product.rs` |
| Windows shipping/dev lifecycle and recovery | `no_flag/windows/{lifecycle,dev_lifecycle,recovery}.rs`, `no_flag/windows/support/{product,product_cycle,process}.rs` |
| Windows staging, process/HANDLE and window evidence | `no_flag/windows/staging.rs`, `no_flag/windows/support/{stage,handles,window}.rs` |
| Windows console/control contracts | `no_flag/windows/{console,control_contract}.rs`, `no_flag/windows/support/control.rs` |
| Windows renderer HTTP/publication/deadline/connection contracts | `no_flag/windows/renderer_{http_contract,publication,deadlines,connections}.rs`, `no_flag/windows/support/renderer.rs` |
| Windows profile identity/storage/purge/multi-user | `no_flag/windows/profiles/**`, `no_flag/windows/support/profile_*.rs`, `no_flag/windows/support/{cross_user,signed_identity,signed_process,signed_purge}.rs` |
| Windows media acceptance | `no_flag/windows/media.rs` |
| Shared fixture assets | `fixtures/**`; inspect callers before changing bytes, signing/account prerequisites, or scripts |

The integration roots remain `no_flag_linux.rs`, `no_flag_macos.rs`, and
`no_flag_windows.rs`. They preserve executable identity, module registration, and the
root helper selectors that subprocess callers depend on. Platform-specific scenarios,
oracles, ownership, and cleanup remain platform-local unless equivalence is independently
proved.

## Discovery and evidence limits

List registered cases on the native OS:

```sh
cargo test -p keld-host --test no_flag_linux -- --list --format terse
cargo test -p keld-host --test no_flag_macos -- --list --format terse
cargo test -p keld-host --test no_flag_windows -- --list --format terse
```

Run only the command for the current OS. Profile discovery also needs
`--features profile-test-hooks` where the relevant target supports it. A listing proves
registration, not product behavior.

For an exact case, copy its fully qualified name from the native listing. Raw libtest can
exit successfully when `--exact` selects zero tests, so subprocess helpers must also
prove their expected handshake or observable effect. Preserve the existing helper
selectors and runner/nextest scheduling when moving source.

`just ci` owns the full local repository gate; `.config/nextest.toml` owns native test
groups and concurrency. Ignored signing, media-device, second-user, reboot, or other
manual rows require their stated prerequisites and real-OS evidence; default or hosted
runs do not qualify those conditions.
