# keld-runtime invariants

Extends root `AGENTS.md`; this file owns only runtime process-boundary rules.

## Windows containment ABI

- Production `unsafe` is limited to `windows_job.rs` and `windows_lpac.rs`.
  Each operation MUST use the narrow `windows-sys` projection, deny
  `unsafe_op_in_unsafe_fn`, and carry a local `// SAFETY:` handle, pointer,
  allocation, or lifetime proof. Another runtime module requires human review
  plus an update to this owner; tests MAY use unsafe only for independent OS
  observation and fixture cleanup.
- The host Job is supervisor cleanup, never authority containment. It MUST be
  unnamed, non-inheritable, configured without breakaway, installed before any
  child, and proved by host-only termination of a real descendant tree.
- LPAC is the Windows authority boundary governed by
  `docs/specs/kel78-strict-profile-sandbox.md`. A strict spawn MUST use zero
  capabilities, the All Application Packages opt-out, reviewed path ACLs, an
  explicit environment, and private inheritable duplicates listed in
  `PROC_THREAD_ATTRIBUTE_HANDLE_LIST`. Caller-owned handle flags MUST NOT be
  toggled. The child remains suspended until token and raw handle-table
  admission evidence passes.
- Windows containment changes MUST run on real Windows with hostile direct-API
  probes and a temporary negative control for the changed atom. Job cleanup,
  LPAC denial, guarded host protocol, and resource limits remain separate
  verdict layers; one MUST NOT substitute for another.
