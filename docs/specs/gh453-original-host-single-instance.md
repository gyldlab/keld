# Spec: cooperative original-Host no-argument single-instance first proof
Status: approved under the user’s explicit technical-owner delegation; implementation and verification in progress
Linear: GH-453 (#517; existing issue, no new tracker) · Owner: @0monish · Updated: 2026-11-01

## 1. Goal & non-goals

Before the original Bun entry evaluates, an explicitly admitted macOS Legacy development Host makes one cooperative nonblocking kernel-lock attempt for a literal application coordination id. That original durable Host retains an acquired lease through its session and unresolved fatal native custody. The permanent Electron facade reads the captured data outcome synchronously: acquired is true and contended is false, including before Ready. A losing Host still starts its ordinary Bun session.

Non-goals: full GH453 qualification; `hasSingleInstanceLock`; release/reacquire from the app; additionalData; `second-instance` delivery; new grants/principals; strict profiles; other OS/accounts/installed launch; hostile same-user replacement exclusion; automatic migration emission; original loser help/version/export; changes to original dirty Quit/cancel/save behavior; performance claims.

## 2. Spec refs

Architecture 02 process ownership, 03 security default-deny and 04 compatibility ownership govern. This explicitly extends KEL96's config/closed compiled descriptor, GH452's private launch-fact record and KEL75's existing capture seam. The user explicitly approved the remaining in-scope technical decisions through the root task continuation on 2026-11-01. That approval covers this bounded contract; execution and independent review remain gates. The same change amends `kel96-no-flag-host-boot.md`, `kel75-principalized-bun-child-roles.md`, `gh452-app-session-facts.md`, and the relevant GH497/GH499 declaration and qualification notes. Original GH452 §§11–13 schema consumers remain readable. The code is integrated only by the root writer; this specification is recorded before further implementation in this continuation.

## 3. Acceptance criteria (binary, each becomes a test)

1. Captured `keld.config.ts` has no field or exactly one literal `singleInstance: { id }`. The id is 1..128 ASCII bytes of nonempty dot-separated lowercase letters/digits/internal-hyphen components. Dynamic/computed/accessor/spread/duplicate/extra/escaping forms are typed `KELD-CORE-035` before Host launch. Display name, project root, nonce stage and current directory do not choose the id.
2. The actual selected closed compiled schema 1/2/3/4/5 retains the same optional declaration without reselecting profile, renderer or graph. Null, sequence, duplicates, unknown fields and invalid ids refuse. A present declaration on an unqualified platform refuses rather than disappears.
3. No file acquisition occurs without existing explicit Legacy admission and its existing disclosure. Exactly one `File::try_lock` occurs on the retained nofollow private persistent lease. Only `WouldBlock` means contended; all other errors remain typed failure. No unlink, truncate, PID-liveness heuristic or retry queue selects a verdict.
4. Real distinct nonce-stage shipping Hosts with the same id produce true/false before and after Ready while both Bun roles run. A fresh successor produces true after normal first-Host Quit. Different ids do not contend. Native census proves the exact dev/inode is held by the original Host and not Guardian/Bun/other descendants. Host-only death releases the lease while relevant descendants remain observable; an intentionally inherited duplicate defeats that successor-acquisition observer. The intentional-leak control must fail without killing all children to conceal it.
5. The original Host retains its lease across Bun recovery, canceled/dirty Quit and unresolved fatal native custody. Normal ordered teardown closes it. Actual original draw.io startup and dirty Quit/cancel/save/cleanup remain separately required acceptance, with no inference from a synthetic fixture.
6. The one existing private carrier advances to schema 4. It contains only optional `singleInstance: { verdict: "acquired" | "contended" }`, never id/path/descriptor/grant. Its 16 KiB limit, closed own data objects and duplicate rejection remain. Schemas 1/2/3 stay readable, reject this new field and cannot supply it; absence in schema4 remains absence.
7. The canonical pre-entry transport captures a frozen null-prototype copy of the outer record and nested verdict. Own getters, proxies, malformed/inherited-only verdicts and unknown members refuse with `KELD-IPC-029` without invoking user getters. Later source or prototype mutation cannot change or forge it.
8. Only `requestSingleInstanceLock()` and explicit single `undefined` read the captured verdict. No record or an older schema throws the shared `AppSessionFactError` with `KELD-COMPAT-040`, fact `singleInstance`, reason `unavailable`; schema4 without a declaration throws reason `undeclared`. Both name GH453 and a truthful declaration/compiler/Host fix. Non-undefined additionalData or excess arguments throw existing `KELD-COMPAT-013` before reading the fact. No constant/default verdict, method stubs or feature qualification is inferred.
9. The existing generator regenerates the canonical transport stamp from final bytes, and its freshness check passes. Required format, warning-denied workspace Clippy, workspace tests, generated-doc checks and exact final-diff review pass before adoption/qualification. The corpus request row remains expected fail and GH453-owned until actual native/product qualification.

## 4. Design

Atoms and their independence are recorded in `ATOMS.md`. This adds one original-Host file owner and a data-only boot/facts contract; Guard admission, principal minting and OS authority owners stay existing. Reuse: the exported-root config scanner/tokenizer, serde closed object visitors, CLI's captured source and selected descriptor producer, native Application Support data locator, safe existing nix filesystem primitives, std `File::try_lock`, current original-Host lifecycle/fatal-custody owner, existing Core/Guardian private record, canonical pre-entry transport and shared `AppSessionFactError`/unsupported error. No dependency, new production unsafe, async runtime, second environment carrier, KIPC channel or mutable setter is added.

The selected object is `Application Support/Keld/InstanceLocks/v1/<id>/lease` under the native user directory. Directory descriptors and file are retained, private mode/owner/kind/link-count/dev-inode checks run before and after the attempt, and the persistent inode is never deleted by Keld. The native directory owner's domain defines cooperative exclusion; no authenticated release identity or adversarial same-user guarantee is claimed. Visible path replacement is refusal, while a later same-user replacement remains outside scope.

Shared-parent correction under the delegated original-MVP owner decision:
`Keld` is an existing application namespace, not an exclusively private lock
directory. It must remain same-user-owned, nofollow, identity/kind stable and not
group/other writable; an existing owned0755 namespace is valid and is never
chmodded or deleted. Only `InstanceLocks`, `v1` and the application id require
exact0700. The lease still requires exact0600, one link and stable retained
identity before/after the single kernel attempt. Keep every ancestor descriptor
and the same-user replacement limitation. This adds no grant or weaker private
leaf; real filesystem/kernel regressions and final review remain required.

Rejected alternatives: role-side lock acquisition (wrong durable owner); constant boolean (no resource proof); stage/display-name key (distinct stages of the same application fail to contend); deleting a stale-looking path (can split live owners across inodes); additional environment carrier (duplicates KEL75); duplicating the transport parser or error class (violates current owner). Published unsupported/absent behaviors retain typed fail-closed paths; no silent compatibility fallback is chosen.

Runtime seam: validated descriptor and existing Legacy admission precede original-Host acquisition; only serialized outcome crosses Guardian and Bun. Recovery keeps the original file. A retained fatal `HostRunFailure` receives the lease with unproved native custody and the shipping Host retains the whole failure through process exit. File inheritance and all shutdown cases require native evidence; source placement is not that proof. Memory/I/O is cold bounded config/facts parsing and one filesystem attempt, with no queue/copy/performance claim.

Migration unit: captured config, selected descriptor, Core private record, canonical transport, facade, error registry and explicit corpus failure disposition move together. The permanent facade remains; no temporary adapter is introduced. Automatic GH497 migration emission waits for its own owning change and tests.

## 5. Boundaries

Implement only the paths enumerated in `candidate-source-manifest.json`; the root integrator owns any approved spec amendments and generator outputs. Existing Host graph harness WIP is independently owned and must survive integration. Must not touch Guard authority, native profile admission, workspace dependencies, original app source or wider GH453 APIs.

## 6. Tasks

- [x] T1 Record the bounded technical scope/spec decision under the user’s explicit delegation; coordinated KEL96/GH452/KEL75 amendments accompany this source packet.
- [ ] T2 Root integrates tests first, observes RED, applies implementation, runs owner generation/typecheck/build and focused contract tests.
- [ ] T3 Root executes independent native descriptor/lifetime/negative controls and actual original app criteria, then required full gates and exact final-diff review. Corpus qualification is updated only for the proven owned slice.

## 7. Test plan

Criteria 1/2: Core config and boot tests plus real shipping compiler descriptor assertion. Criterion 6: Core private record roundtrip/malformed tests. Criteria 7/8: `single-instance-facts.test.ts`, whose separate children isolate module capture and prototype faults. Criterion 4: existing proposed shipping no-flag pair plus root-owned descriptor/Host-death/intentional-leak observers still to prepare or run. Criterion 5: existing lifecycle fault controls and actual original app; source tests cannot replace these. Criterion 9: owner codegen/check and required full gates. Negative mutations must make the corresponding test fail: force acquired/remove lock for the real pair; leak one duplicate for Host-only death; accept old schema/malformed nested verdict for record tests; return true or false for facade; use raw nested source instead of frozen copy for mutation test. Tests use independent exact data/errors/OS outcomes, isolated temporary resources and observable handshakes; deadlines are kill switches. The isolated Bun contract tests pass (6/6), existing fact/schema consumers pass (41/41), and strict Electron package typecheck passes. A previous direct-narrowing type error was corrected at the facade owner. Constant-true and raw-nested-input negative mutations both fail their intended test. These local results do not prove the native lease or shipping launch; root owns Rust/native checks.

## 8. Review gates triggered

public API: yes; wire/private boot and facts protocol: yes; permission model/profile boundary: independent review required to verify existing Legacy-only admission and no grant broadening; unsafe: none newly selected; dependency addition: none. Existing safe native APIs do not remove independent lifecycle/security evidence obligations. Exact final-diff review is root-owned and not performed by this preparation worker.

## 9. Perf impact

No performance claim. One cold filesystem selection and nonblocking attempt plus a bounded private data field; any measured startup claim must independently decompose census/work/queue-copy/clock/statistic/artifact under architecture 01 §5.

## 10. Open questions

None for this bounded scope. The user explicitly delegated all remaining in-scope technical owner approvals and instructed implementation to continue. The root relayed that authority in the startup task continuation on 2026-11-01; `approved-decision.json` retains its exact scoped wording. Approval is separate from unrun native/build/review predicates and does not qualify broader GH453 or invent implicit Legacy admission.
