# keld-native invariants

Owner/load: native/always. Trigger: crates/keld-native. Extends root AGENTS.md.

- Production MUST NOT use unsafe.
- Test-only unsafe: `tests/support/windows_handle_census.rs` MAY call
  GetCurrentProcess/GetProcessHandleCount with the current-process pseudo
  handle and local count; never close that handle.
- `src/fs.rs` cfg(test) MAY call WaitForSingleObject only on a retained Child.
  Keep the bounded wait. Retire these allowances if the probes move.
