# Host acceptance test map

These files exercise the shipping host; they are not its implementation. The
[testing playbook](../../../.agents/testing.md) owns proof and organization rules.
Start at the failed name; read its definition and actual dependencies, not every file.

## Current navigation

| Contract | Source and useful starting symbols |
| --- | --- |
| Diagnostic flags | `host_flags.rs` |
| macOS boot/policy rejection | `no_flag_macos.rs`: `InvalidBoot`, `InvalidPolicy`, `NativeAbsenceWatcher` |
| macOS lifecycle/recovery | `no_flag_macos.rs`: `ShippingDevCycle`, `RecoveryCycle`, `LiveCycle` |
| macOS descriptor/window observation | `no_flag_macos.rs`: `unix_descriptor_census_*`, `unix_descriptors_reported`, `NativeWindowObserver` |
| Signed identity, storage, purge, multi-user | macOS/Windows `no_flag_*` files: `kel135_*`; signing/account prerequisites remain explicit |
| macOS media proof | `no_flag_macos.rs`: `media_restart_proof`, `dev_media_proof` |
| Windows stage/HANDLE ownership | `no_flag_windows.rs`: `windows_stage_*`, `assert_dev_lease_handle_isolation` |
| Windows lifecycle/recovery | `no_flag_windows.rs`: `run_same_window_recovery`, `shipping_windows_*` |
| Windows profile observations | `no_flag_windows.rs`: `SignedProfileStateRun`, `ProfileStateServer` |
| Linux admission/strict-tree/recovery | `no_flag_linux.rs`: `linux_*`, `shipping_keld_dev_*`, `ProcessIdentity`, `StrictGeneration` |
| Shared fixture behavior | `fixtures/t1b_harness.ts`, `fixtures/profile_state.js`, native Swift and signing/account scripts; inspect callers before editing |

Use `rg -n` with the actual failing name, then read that bounded source range and its
referenced helpers. This map describes domains, not a second registry of every case.

## Commands and evidence limits

From the repository root on macOS, list registered cases:

```sh
cargo test -p keld-host --test no_flag_macos -- --list
```

Use the Windows/Linux target on its native OS. Profile discovery also needs
`--features profile-test-hooks` where applicable. Listings prove registration only.
Select the exact full test name from the listing to run one case. Self-invoked helpers
may return normally without their fixture environment; exit zero is not proof that
its helper body or product contract ran.

`just ci` owns the full local gate; `.config/nextest.toml` owns native test groups.
Ignored signing, camera/microphone, second-user and reboot cases need their explicit
prerequisites and operator authorization; default runs do not qualify them.

## Proposed migration, not current layout

[KEL-252](../../../docs/specs/kel252-contract-oriented-test-layout.md) proposes thin
existing roots, contract modules and narrow support. Those directories are not
implemented by this documentation change. Preserve exact helpers/selectors, fixture
paths, native scheduling, assertions and cleanup. Update this map with each approved
move rather than documenting uncreated modules as current.
