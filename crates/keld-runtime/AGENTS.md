# keld-runtime invariants

Extends root `AGENTS.md`; owns process boundaries.

- Production `unsafe` is limited to `windows_job.rs`, `windows_lpac.rs`,
  `linux_strict.rs` pre-exec FD isolation, and
  `lib.rs::peek_pipe_available_handle`'s read-only `PeekNamedPipe` byte query.
  Each MUST use narrow bindings, deny `unsafe_op_in_unsafe_fn`, and carry
  local `// SAFETY:` proofs. New paths require review + root/nested updates;
  tests MAY use unsafe only for independent OS observation and cleanup.
- Linux strict MUST use unprivileged Bubblewrap, fail closed on setup,
  and expose only stdio/seccomp FDs. Pre-exec MAY duplicate them,
  apply `close_range(..., CLOSE_RANGE_CLOEXEC)`, and return an OS error;
  no allocation/general Rust after fork. Real tests MUST separately prove
  namespace/mount/Landlock denial, caps, no-new-privs, seccomp, FDs, descendants,
  host death, negative controls, and relaunch.
- The host Job is supervisor cleanup, never authority containment. It MUST be
  unnamed, non-inheritable, non-breakaway, installed before children, and proved
  by host-only death of a real descendant tree.
- LPAC is KEL-78's Windows authority boundary. Strict spawn MUST use zero
  capabilities, All Application Packages opt-out, reviewed ACLs, exact env, and
  private duplicates in the handle list. Caller-owned flags MUST NOT change;
  resume follows token/raw-handle evidence.
- Windows changes MUST run on real Windows with hostile probes/negative control.
  Job, LPAC, protocol, and limit verdicts stay separate.
