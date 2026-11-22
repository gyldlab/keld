# References and revision history

Part of: [index](index.md)
Owns: References and revision history

## 2. Spec refs

- [KEL-142 API contract](../kel142-macos-renderer-bridge-api/design-api.md) and [bridge, ordering and errors](../kel142-macos-renderer-bridge-api/design-bridge.md) §4.1 (no
  automatic replay),
  §4.2 (terminal outcomes), §6 (ordering, one terminal), §7 (errors). This
  revision replaces §7 and refines §4.2's "missing handler" bullet. Every other
  KEL-142 rule is unchanged.
- `docs/specs/gh527-worker-owned-blocking-call-transport.md` §4.4 (`WorkerLink`
  terminal codes) and §4.6 (`@keld/api` ERR answers). Consumed. Per owner
  decision Q-3 (§10), T-1 corrects only the three passages where it says that a
  call pending at a Quit-close "neither ran" (§4.5).
- `docs/architecture/02-ipc.md`: KIPC frames and the `CallError` payload. Not
  changed.
- `docs/architecture/07-agent-experience.md` §2 and
  `docs/engineering/keld-error-codes.md`: registry rules (a heading lands only
  with its first emitter).
- KEL-143 acceptance: "prove recovery never silently replays an operation whose
  outcome is ambiguous".

The design deviates from no architecture section.

### Historical section map

The original local section numbers remain historical references: sections 1 and 8 are in [index](index.md), 2 in [references](references.md), 3 in [acceptance](acceptance.md),
4.1–4.9 in the nine design parts linked from the index, 5 in [boundaries](boundaries.md), 6 in [tasks](tasks.md), 7 in [tests](tests.md), 9 in [performance](performance.md), and 10
in [decisions](decisions.md). References explicitly naming another document retain that document’s numbering.

### Historical revision declarations



Historical status declaration: **revision r3 draft amendment (GH-1132), pending exact-content owner approval**.
Revision r2 was approved at `60280baa` and merged in PR #1096
(`f19c8847`); the r3 correction below is **not yet approved**.
Historical tracker declaration: GH-1095 (#517; tracker of record; linked from KEL-143) · Revises: KEL-142 §7 ·
Historical owner declaration: repository owner · Historical revision date: 2026-10-24

Historical code citations are at `origin/main` `516644b0`. Evidence for the
current behaviour comes from the never-merge experiment PR #1082 (issue #1081).
The new retry-safety correction changes only the **app guidance and test plan**.
No production change is authorized by this draft.

### Revision r3 (GH-1132; 2026-10-24; requires exact-content approval)

Adversarial review after the r2 merge found that [historical §4.4](design-4.md)(b) made a **negative status
snapshot** an independent reason to resend an Outcome Unknown operation. A
handler can schedule a side effect, reject with `KELD-API-001`, and leave its
work active. The app then sees "no effect yet", retries, and both operations
commit. A status query is safe only if it also gives enforceable proof that the
earlier operation **cannot commit later**. r3 removes status-only retry
authority; the safe paths and their negative control are in [historical §4.4](design-4.md) and AT-RS.
The original error-code classifications, wire format, and r2 navigation/error
shape rules are unchanged.

### Revision r2 (2026-10-23, docs only; approved at `60280baa`)

The implementation review of PR #1105 found two defects in the approved
content. Owner decision 3 authorized drafting this revision.

1. **[historical §4.7](design-7.md)'s premise was unreachable.** A host `navigate()` cannot run while
   the app runs. `run_app_until_quit` holds `&mut self` and moves the views
   and bridges into the event loop (`crates/keld-wv/src/wkwebview/mod.rs:302@516644b0e1c086baa08ff8e8b52667f4bc97144a`, `:391` on #1105),
   and `AppWindowCommand` has no navigate command. So "a bridge reset without a
   commit" cannot happen today. r2 revises criterion 7, [historical §4.7](design-7.md) and AT-S arm (d)
   to match. Exactly-once now applies to every navigation that can actually
   occur, and the uncommitted host-navigation case is deferred to its future
   owner (#459).
2. **Criteria 10 and 13 conflicted.** The host prefixing rule
   (`app_session.rs:5789` on #1105) changes the message bytes when a Bun
   message starts with its code but not with `"<code>: "`. r2 picks one
   precise prefix rule with one owner ([historical §4.9](design-9.md)). It states exactly which messages
   keep their bytes and which gain the prefix.
