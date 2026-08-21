# Spec: no-flag host-as-app boot (Unique #1 lifecycle half)

Status: draft
Linear: KEL-96 · Owner: GYLDLAB · Updated: 2026-08-21

## 1. Goal & non-goals

Make the shipping `keld-host` **process** the application lifecycle root when
invoked with no `--hello` flag: it loads an approved compiled-config boot
artifact, owns the UI event loop / window, owns the authenticated hello
app-link listener, and supervises the Bun child for that window's lifetime.
`keld dev` may launch or orchestrate that host but must not remain a second
owner of window, listener, token, or restart loop.

This is the **boot/lifecycle** half of Unique #1. The **artifact** half
(prebuilt signed host that app developers do not compile) remains research
`82-prebuilt-host-t1.md` / KEL-103 and is out of this spec's first
implementation slice unless a human expands scope.

Non-goals:

- Inventing a fifth unique.
- Wiring `RoleRegistry` / principal-before-dispatch (KEL-97 after KEL-75).
- Implementing keld-auth / KEL-89.
- Filling `keld-pack`, DMG/MSI/deb, update feed, or cross-compile farms.
- Claiming Windows named-pipe/DACL as done (KEL-101); v0 may keep documented
  interim loopback on Windows if the approved design says so.
- OS strict-profile sandbox proofs (KEL-78 T2–T4).
- Mass-merging unrelated open PRs.

## 2. Spec refs

- `docs/architecture/01-overview.md` §§1, 2, 4 (destination host authority;
  PARTIAL LIVE host-owned hello echo today)
- `docs/architecture/02-ipc.md` §1 (v0 host-owned `EchoServer` /
  `HostOwnedHelloSession`; destination role-bound link)
- `docs/architecture/06-runtime-and-tooling.md` §§1–2
- Research (private): `83-host-vs-cli-ownership.md` @ Keld evidence SHA
  `184b308` (refresh); `121-kel30-host-owned-assess.md`;
  `122-kel30-shipped-slice-acceptance.md`; `82-prebuilt-host-t1.md`

Architecture already names destination ownership. This spec freezes the
smallest executable boot contract that turns no-flag `keld-host` into that
process without pretending KEL-30 already shipped Unique #1.

## 3. Acceptance criteria (binary, each becomes a test)

1. Given a valid compiled-config boot fixture for the live platform, when
   `keld-host` is launched with **no** `--hello` flag, then a real native
   window shows the fixture title/renderer marker and the process does **not**
   print the pre-alpha banner or early-return path that exists on `main` today.
2. Given missing, malformed, or untrusted compiled config, when no-flag
   `keld-host` starts, then it exits with a typed error that states the fix,
   starts no Bun child, and opens no window.
3. Given shipping `keld dev` against that fixture, when the session is live,
   then the native window PID, app-link listener, and Bun supervisor are owned
   by a `keld-host` process (process-tree / handle ownership). Falling back to
   the current in-process CLI owner must fail the test.
4. Given a live no-flag host session, when Bun completes HELLO + ≥1 echo
   CALL/REPLY on the host-minted `KELD_APP_LINK`, then a second CALL remains
   possible while the same host window owner is still in its event loop
   (concurrent coexistence under the **host** process).
5. Given a visible host window and a real Bun child, when the child crashes
   once under the restart policy, then the same host window lifetime continues,
   a replacement child starts, and the old link generation fails closed if the
   approved design requires fresh generation (otherwise document reuse of the
   KEL-30 single-link session explicitly as a temporary AC with a follow-up
   ticket).
6. Given approved host shutdown or last-window-close per the design section,
   when teardown runs, then the listener/token become unusable before or as
   Bun is reaped, and no orphan Bun remains after orderly exit.
7. Given retained diagnostics, when `keld-host --hello`, `keld hello`, or
   `keld ipc-client echo` run, then they keep diagnostic/client roles and
   cannot select a principal or become a second application owner.

## 4. Design

### First-principles and reuse decision

- **Ownership facts today (`184b308`):** `HostOwnedHelloSession` /
  `EchoServer` live in `keld-core` (crate ownership). Shipping `keld dev`
  still runs that session **inside the `keld` CLI process**. No-flag
  `keld-host` prints a banner. Unique #1 requires moving the lifecycle root
  into the `keld-host` **process**.
- **Reuse:** Reuse `keld_core::HostOwnedHelloSession`, `EchoServer`,
  `keld_runtime::Supervisor`, Unix `BootstrapListener`, and existing hello
  window entry points. Do not add a second restart loop or parallel token
  policy.
- **Named unmet requirement:** no approved boot artifact + no-flag entry that
  makes `keld-host` the process owner. Crate move in KEL-30 is insufficient.
- **Rejected alternatives:** inventing Unique #1 by deleting the banner only;
  wiring RoleRegistry into the first slice; treating research 83 as an
  approved implement license without this spec's Status: approved.
- **Compatibility fallback:** keep `--hello` as a diagnostic window path.
- **Performance:** none claimed; cold-to-window / RSS measured only if a later
  PR asserts a change.

### Boot artifact (must be named before Status: approved)

Open question until human chooses one reversible option:

- **Option A (smallest):** host reads a checked-in / generated JSON (or similar)
  next to the fixture / app bundle; schema versioned; typed parse errors.
- **Option B:** host invokes existing source `keld.config.ts` only under
  `keld dev` orchestration and refuses no-flag boot without a compiled
  artifact (forces artifact half sooner).

Default for draft review: **Option A** for the first falsifiable fixture;
Option B remains explicit non-goal for T1 unless chosen.

### Process topology

```text
Today (PARTIAL LIVE):  keld (CLI) --owns--> window + HostOwnedHelloSession + bun
Destination (this spec): keld-host --owns--> window + HostOwnedHelloSession + bun
                         keld (CLI) --launches/logs--> keld-host (dev)
```

### Types / channels (sketch)

- Extend `keld-host` main: no-flag path loads boot config → starts
  `HostOwnedHelloSession` → opens window via existing `run_hello_window_*`
  while session live → shutdown on window return.
- Optional thin `keld-core` helper: `run_host_owned_app(...)` so CLI and host
  share one owner function (CLI only if a human keeps a temporary dual path;
  AC3 forbids dual ownership in the shipping path).

### Capabilities / manifest

none for T1 (no new grants). Do not claim Unique #4 wiring (KEL-102).

### Wire protocol

none — reuse existing HELLO / `KELD_APP_LINK` / echo CALL contracts.

### Platform notes

- macOS: UDS bootstrap already live for host-owned echo.
- Windows: document loopback TCP as interim unless KEL-101 lands first.
- Linux: same concurrent product proof required (KEL-100 overlap); do not mark
  3-OS Unique #1 Done from Darwin alone.

## 5. Boundaries

- Implement in: `crates/keld-host`, `crates/keld-core` (session/boot helper),
  `crates/keld-cli` (`keld dev` launch/orchestration only), tests, architecture
  LIVE/TARGET labels in the same PR that ships ownership.
- Must not touch: `RoleRegistry` product wiring; `keld-pack` installers;
  `keld-auth`; CI router workarounds; unrelated open PRs; inventing signed
  release machinery (KEL-103).

## 6. Tasks (each ≈ one PR; ordered)

- [ ] T0: Human approves this draft (Status → approved) citing refreshed
      research-83.
- [ ] T1: No-flag `keld-host` boots fixture window from compiled config;
      banner gone on success path; typed failure on bad config. Darwin first.
- [ ] T2: `keld dev` launches host process owner; AC3 process-tree proof;
      concurrent HELLO/CALL under host.
- [ ] T3: Window-survives-Bun-crash (or explicit deferred ticket if T1 reuses
      single-link session).
- [ ] T4: Windows + Linux product proofs (coordinate with KEL-100).
- [ ] T5: Arch 01/06 LIVE labels match shipped process ownership.

## 7. Test plan

| AC | Test |
|---|---|
| 1–2 | Host binary integration: no-args success + malformed config |
| 3 | Process-tree / window PID ownership under `keld dev` |
| 4 | Extend concurrent coexistence under host process (no sleep-sync) |
| 5 | Crash + restart with window continuity (or deferred ticket id) |
| 6 | Shutdown/reap + endpoint cleanup |
| 7 | Diagnostic regression (`--hello`, ipc-client) |

Anti-flake: await output markers / conditions; bind port 0; temp dirs; no
sleep-sync.

## 8. Review gates triggered

- unsafe: none expected
- Public API: yes if new shared boot helper is public
- Permission model: none in T1 (no RoleRegistry / guard wiring)
- Dependency addition: none expected
- Wire protocol: none

## 9. Perf impact

none claimed. If cold-to-window or RSS changes are asserted, use architecture
01 §5 fixtures with attribution.

## 10. Open questions

1. Boot artifact format and trust (Option A vs B above).
2. Does T1 require fresh link generation on Bun restart, or is KEL-30
   single-link reuse explicitly deferred to KEL-97/KEL-75?
3. Exact last-window-close vs quit policy for the no-flag host (align with
   KEL-72 lifecycle events once host-owned).
4. Whether `keld hello` remains a CLI in-process diagnostic forever or becomes
   a thin host launcher.
