# keld-runtime invariants

- Unsafe owners: Windows Job/LPAC, macOS Guardian pre-exec/registration, Linux strict pre-exec and pipe peek require explicit SAFETY and unsafe-op denial. Before changing a platform seam, read its [conditional native contract](../../docs/architecture/06-runtime-and-tooling.md#10-platform-scoped-runtime-unsafe-contracts). Do not widen process/handle rights or ambient authority.
- macOS Guardian: no Bun before authenticated host-retention ACK; pre-exec is fixed-stack and fail-closed. Refuse unproven PID or process group without signaling. SIGSTOP pre-exec and detached descendants are not proven contained; native negative controls required.
- Windows Job: unnamed, noninheritable, nonbreakaway Job precedes child execution; only verified release/census may remove kill-on-close. Keeper has QUERY/TERMINATE, never ASSIGN or limit changes.
- Linux strict: unprivileged Bubblewrap; fail closed, stdio/seccomp FDs only. Pre-exec MAY dup + mark CLOEXEC; no Rust/alloc after fork. Native acceptance: `.agents/testing.md`.
- Windows Job: unnamed/noninherit/nonbreakaway before children; launcher retains outer Job. Keeper QUERY/TERMINATE exact attempt only; no host/Bun/assign changes; the only limit change is the S6b3 strip. EOF: bounded wait then terminate. Delete stage after host exit + `ActiveProcesses == 0`; Jobs only clean up.
- LPAC: zero capabilities, All Packages opt-out, reviewed ACL, exact env/handle list; preserve caller flags; resume after token/raw-handle proof. Same-token candidate: suspended, no attrs/inherit, Job-assigned pre-resume.
- Windows tests: hostile probes; keep Job/LPAC/protocol/limit results separate.
