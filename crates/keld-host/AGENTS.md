# keld-host invariants

Extends root `AGENTS.md`; this file owns the thin shipping-binary boundary.

- `main.rs` stays a thin dispatcher into validated `keld-core`/`keld-runtime`
  owners. No-flag startup installs the platform host-death owner before any app
  resource; diagnostics and private helper roles return before that path.
- The Windows cleanup-sentinel role may only validate/wait/delete its exact dev
  stage. It MUST NOT enter boot, create a window/app-link/Bun child, mint a
  principal, inherit the host Job, or gain permission authority.
- Production unsafe belongs to the owning library, not this binary. Test-only
  unsafe is limited to independent real-OS handle/process census and fixture
  cleanup with local `// SAFETY:` proofs.
- Product claims require the shipping no-flag executable and native window,
  direct child and descendant process handles, ordered cleanup, stage deletion,
  and relaunch. A helper artifact or aggregate process count is not evidence.
