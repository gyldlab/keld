# Spec: minimum macOS product spine and clean-machine contract
Status: draft
Linear: KEL-139 · Owner: GYLDLAB · Updated: 2026-09-29

## 1. Goal & non-goals

Freeze the smallest real macOS product contract that composes Keld's landed
host, runtime, kipc, guard and native primitives into one developer-visible
path:

`prebuilt Keld -> host window -> window.keld.invoke -> Bun/@keld/api ->
guarded filesystem -> same-document Bun recovery -> ordered Quit`

The final acceptance runs this path while Rust is genuinely unavailable and
then restores the development Mac exactly. This spec composes existing owners;
it does not move window, process, principal, receiver, guard, retained-handle,
signing, or capture ownership.

Non-goals:

- Windows/Linux inference; broad Electron/BrowserWindow parity; multiple roles.
- Installer/updater activation, notarization, Store distribution, CEF, shared
  memory, native-role delivery, or a new packaging system.
- General `@keld/schema` / `keld gen`, a second renderer protocol, or the full
  destination `window.keld.send/on/stream` surface. This slice needs only
  `window.keld.invoke`.
- Exactly-once replay for an operation whose effect may have committed before
  its reply was lost.
- A macOS strict-profile claim without separate KEL-78 shipped-artifact proof.
- A performance-improvement claim or a new benchmark prerequisite.

## 2. Spec refs

This contract consumes, rather than redefines:

- architecture 01 §§1–4 — host authority, supervised Bun, webview trust, no-Rust
  destination;
- architecture 02 §§2–4 — kipc/session/correlation/cancellation/backpressure;
- architecture 03 — generated default-deny host policy;
- architecture 05 §§2–3 — `window.keld` destination bridge and guarded native
  composition;
- architecture 06 — supervision, recovery and ordered shutdown;
- KEL-96 approved no-flag boot, especially landed macOS T3;
- KEL-75 approved role/lifecycle contract, especially passed T4a;
- KEL-102 approved guard contract, with exact KEL-102/T3 required by KEL-140;
- KEL-130/T1 passed retained-filesystem artifact;
- KEL-133 passed receiver semantics + AC9 supplement;
- KEL-134 passed bounded-output artifact;
- KEL-136 landed canonical TypeScript app-link transport;
- KEL-98 landed bounded echo declaration;
- KEL-80's bounded product-spine bridge slice once its exact artifact passes;
- KEL-103 as a future exact signing/prebuilt predecessor for KEL-141;
- KEL-144 as the physical-Mac Rust-quarantine/restore owner.

**No architecture ownership boundary changes here.** A child that needs to
change handle ownership, crash ownership, or principal minting must stop and
obtain its own architecture/spec approval.

## 3. Acceptance criteria

Each row is independently pass/fail; one row cannot borrow another's oracle.

### AC1 — immutable prebuilt launch, no Rust

Given KEL-141's approved immutable macOS arm64 CLI, host and pinned Bun
artifacts, KEL-144 makes `cargo`, `rustc` and `rustup` genuinely unavailable,
uses no repository checkout/target artifact, and launches one real host-owned
window from a separate project.

Require all:

- artifact hashes/provenance and release-vs-test-signing classification;
- host-controlled CLI -> host -> Bun selection; inherited `PATH`, project input
  and child input cannot substitute Bun;
- process/executable census shows no compiler or checkout-built executable;
- toolchain absence is observed before launch.

A sanitized `PATH`, source-built binary, or unsigned artifact called "release"
does not pass. Machine restoration is AC7, not inferred from AC1. KEL-139 does
not choose KEL-103's certificate/team/secret owner.

### AC2 — one real renderer call uses only the host bridge

Given the real WKWebView installs the trusted preload/user-script before page code
in the engine's isolated world, with only the reviewed `window.keld` facade exposed
to the page and no page access to the preload realm, an OS-visible pointer activates
the sample button. Exactly one nonzero-correlation
typed `CALL` crosses:

`renderer -> native bridge -> host router -> admitted Bun`

and its matching typed `REPLY` updates the same document.

The required public surface is the already-specified
`window.keld.invoke(channel, payload, opts?)`; this slice does not claim the rest
of architecture 05's destination bridge. The AC2 fixture calls `invoke` without
`opts`. This parent does not define or approve option keys; KEL-142 must freeze any
supported option semantics in its own public-API review, and options cannot carry
principal, role, navigation generation or other authority.

Negative controls must fail before app dispatch for:

- raw endpoint/token access or direct renderer -> Bun connection;
- caller-selected principal/role/navigation generation;
- stale navigation, malformed/forged identity and over-budget requests;
- slow-consumer growth beyond KEL-80's declared call/byte bound.

A JS-only `element.click()` is not real-macOS evidence.

### AC3 — `@keld/api` lifecycle/channel reflects host state

One authenticated Bun generation consumes lifecycle over the landed canonical
`@keld/kipc` transport. The minimum `@keld/api` behavior must:

- wait for host `Ready`;
- subscribe/unsubscribe to host `LastWindowClosed`;
- issue correlated `Quit` and surface typed success/failure;
- host the one typed application request/reply handler used by AC2.

No import-time ready promise, stdout readiness, renderer readiness, second
socket, or copied HELLO/frame/deadline/receiver/write-queue logic is allowed.

`@keld/api` is specified-only today. KEL-142 must freeze its exact exported
identifier names in a public-API-reviewed child spec. It must extract/reuse the
current `@keld/electron` lifecycle behavior so `@keld/electron` becomes a
compatibility consumer, not a second lifecycle/transport owner.

### AC4 — one guarded filesystem allow/deny reaches the existing owners

Entry requires the exact landed KEL-102/T3 artifact plus KEL-130/T1; KEL-102/T2
or a generic parent pass is insufficient.

For the admitted Bun principal and immutable session policy:

1. one app request reaches KEL-102's sole `dispatch_privileged` boundary and
   KEL-130's retained broker;
2. in-scope write + read returns exact bytes;
3. denied/out-of-scope mutation returns a machine-readable `KELD-GUARD-*`
   failure and fix;
4. an independent OS sentinel proves no denied outside effect;
5. payload cannot choose principal, policy snapshot, retained root,
   `ScopePermit`, or grant index;
6. prerequisite symlink/race/oversize/deadline cases remain rejected through
   the product route;
7. retired-generation and post-quiesce requests cannot enter the handler.

The renderer reaches this through AC2/application code. This is host-enforced
authorization, not proof that Bun has zero ambient OS authority.

### AC5 — recovery keeps the same window **and document**

Entry requires passed KEL-96/T3 + KEL-75/T4a and working AC2/AC4 routes.

After Ready and a successful app/guarded operation, kill only the admitted Bun
generation. Before successor provisioning, old endpoint/token/link/routes,
grants and affected pending calls are unusable according to their owners. Then a
fresh generation authenticates and reaches `Ready(g2)`.

Continuity passes only when both remain:

- the same native window identity;
- a renderer-created document nonce/state value, proving no reload/navigation.

After `Ready(g2)`, that same document emits a successor-correlated bridge call
and completes another guarded filesystem roundtrip. KEL-96/T3's existing
same-window evidence is a predecessor, not this stronger T4a product oracle.

Every affected pending call gets exactly one terminal caller outcome; none may hang
indefinitely or be silently replayed into the successor generation. The harmless
fixture makes an ambiguous commit and any duplicate effect independently observable.

### AC6 — Quit has one ordered owner and leaves no descendants

After recovery, correlated Quit must preserve KEL-96 ordering:

`accept/attribute -> reply -> quiesce/drain -> revoke/close -> terminate/reap -> UI exit`

Observe the correlated reply, authority/link state, live process handles/tree,
zero Keld descendants, host exit and a healthy next launch. KEL-134 output is
bounded diagnostics only; logs are not lifecycle or reap proof.

Child death, host death and requested Quit remain distinct. KEL-143 rechecks
KEL-117/KEL-118 against the live guardian path and narrows/refutes them from
current evidence instead of reopening completed work by assumption.

### AC7 — physical-Mac quarantine and exact restore

KEL-144 refreshes and persists toolchain/Bun paths, versions, permissions,
rustup components/targets, Homebrew state and standalone restore commands before
mutation. It then uses KEL-144's same-filesystem quarantine/restore mechanism,
runs AC1–AC6 from a separate project against immutable prebuilt artifacts, and
restores even when product acceptance fails.

Pass requires:

- `cargo`, `rustc`, `rustup` unavailable and known homes absent from original
  paths during the run;
- no compilation inside the acceptance window;
- AC1–AC6 retain their independent oracles;
- post-restore path/version/component/target/permission/Bun state matches the
  preflight receipt;
- post-restore repository format, warning-denied Clippy and workspace tests
  pass.

No quarantined Rust bytes are deleted until restore verification completes.

### AC8 — evidence provenance

The final artifact records exact landed/source SHAs, product digests,
signing/provenance classification, Mac/OS/Bun/webview facts, CLI/host/Bun/
descendant executable identities, app/profile identity, commands/raw evidence,
negative controls and the KEL-144 before/quarantine/restore receipt.

Deleting one row's evidence keeps that row incomplete. Window survival, HELLO,
a process exit, or a log line cannot substitute for another row.

## 4. Design

### Ownership/reuse decisions

| Concern | Existing owner | Product-spine rule |
| --- | --- | --- |
| Native window | KEL-96 / core + wv | Host remains sole owner. |
| Bun generation | KEL-75/KEL-96 + macOS guardian | One supervisor/generation owner; no second restart loop. |
| kipc semantics | KEL-133 | No parser/policy/deadline fork. |
| TS transport | KEL-136 `@keld/kipc` | One read/write/HELLO owner. |
| Renderer route | architecture 05 + KEL-80 slice | Host-mediated; first slice is `window.keld.invoke`. |
| Lifecycle | KEL-72 behavior | KEL-142 extracts/reuses it for `@keld/api`; no duplicate loop. |
| Guard | KEL-102 | One session snapshot + `dispatch_privileged`. |
| Filesystem | KEL-130 | Retained broker owns scope/effect semantics. |
| Output | KEL-134 | Diagnostics only. |
| Signing/distribution | KEL-103 + KEL-141 | Child decisions; no certificate choice here. |
| Rust quarantine | KEL-144 | Final machine mutation owner only. |

Rejected: direct page->Bun transport, copied lifecycle/kipc code, TypeScript
authorization, stdout readiness, inherited-PATH Bun authority, window-only
recovery proof, broad compat/schema/native-role/shared-memory work.

### Renderer and app seams

The page is untrusted. A pre-load trusted script exposes only the required
`window.keld.invoke` slice. Host state supplies webview/navigation/application
generation identity; caller payload never selects authority. The first product
call may use KEL-98's bounded echo declaration as its typed proof. KEL-142
consumes KEL-80's bounded product-spine routing artifact without claiming the
broader replay/benchmark program complete.

`@keld/api` does not exist in the current tree. KEL-142 creates the minimum
package after its child spec freezes exact exports. Parent-required semantics
are one canonical app-link owner, host lifecycle, one typed handler, no public
endpoint/token/principal API, and callback/pending-call retirement on generation
loss. This parent does not authorize a general channel framework or
`@keld/schema`.

### Guarded operation

The Bun handler receives the host-mediated renderer request, then KEL-140's
adapter calls the exact KEL-102/T3 shipping route. The adapter does not
authorize. Trusted host context resolves the caller; the single guard boundary
lends KEL-130's permit/broker state. Renderer code receives typed results/errors,
never roots, handles, permits or manifest bytes.

### Recovery

KEL-143 composes KEL-96/T3 generation recovery with KEL-75/T4a ordering. The
host-owned window/document remains while Bun rotates. The document nonce is only
a continuity oracle, never authority. New work begins after complete old
revocation and fresh authentication/Ready. No automatic replay crosses
generation loss.

### Distribution/runtime selection

KEL-141 owns acquisition, verification, install location and launch. Its child
spec consumes KEL-103's approved artifact/signing contract and removes inherited
`PATH` as the Bun-selection authority. The host-selected Bun path is bound to a
digest, revision/provenance and update owner that project/CLI/child input cannot
replace.

KEL-139 deliberately does not choose artifact format, signing identity, secret
storage or update mechanism.

### Current -> target seam

Current main already has the host-owned window/app-link/guardian/Bun generation,
but `@keld/api` and the live renderer bridge are absent, KEL-102/T3 is not
landed, and distribution remains source/dev-stage biased.

After this spine, those same native/runtime owners remain. The renderer adds
only a host-mediated call; Bun stays host-minted application logic; guard/native
owners perform privileged effects. No new authority root or process is added.

## 5. Boundaries

KEL-139 itself changes only:

- `docs/specs/kel139-macos-product-spine.md`;
- generated `llms.txt` / `llms-full.txt` through `just llms` if included.

Implementation owners:

- **KEL-141** — prebuilt CLI/host/pinned-Bun distribution; consumes approved
  KEL-103.
- **KEL-142** — renderer `invoke` + minimum `@keld/api` lifecycle/channel;
  consumes KEL-80 bounded slice and landed KEL-133/KEL-136.
- **KEL-140** — filesystem product adapter; requires exact KEL-102/T3,
  KEL-142 and KEL-130/T1.
- **KEL-143** — same-document recovery + second guarded operation; consumes
  KEL-96/T3, KEL-75/T4a, KEL-134, KEL-140/KEL-142.
- **KEL-144** — final Rust quarantine, integrated run and exact restore.

KEL-139 spec work must not touch production code, Cargo/CI, signing decisions,
receiver corpus, native traversal, capture policy, transport implementation,
supervisor/generation ownership or KEL-78 containment claims.

## 6. Tasks

- [ ] **T0 / KEL-139 — approve this contract.**
  Generated-doc freshness, routine gates, exact-tip architecture/public-API/
  permission review, then explicit human approval of the exact spec content.

- [ ] **T1a / KEL-141 — distribution spine.**
  Entry: approved KEL-139 + exact approved KEL-103 predecessor. Freeze pinned
  Bun authority before code; produce immutable CLI/host/Bun and prove ordinary
  prebuilt launch from a non-checkout, read-only location.

- [ ] **T1b / KEL-142 — renderer/API spine.**
  Entry: approved KEL-139 + landed KEL-133/KEL-136 + exact passed KEL-80
  product-spine bridge slice. Freeze `@keld/api` exports in a child spec, then
  implement one `window.keld.invoke` call and real host lifecycle without a
  second transport owner. T1a/T1b may run in parallel.

- [ ] **T2 / KEL-140 — guarded filesystem vertical.**
  Entry: exact KEL-142 + KEL-102/T3 + KEL-130/T1 artifacts. Prove real-Mac
  allow/exact bytes and independent deny/no-effect.

- [ ] **T3 / KEL-143 — recovery continuity.**
  Entry: passed KEL-140/KEL-142 + exact KEL-96/T3, KEL-75/T4a, KEL-134,
  KEL-133. Reconcile KEL-117/KEL-118 first. Prove same document, full stale
  authority retirement, fresh Ready, second guarded call, no silent replay and
  ordered Quit/reap.

- [ ] **T4 / KEL-144 — clean-machine acceptance.**
  Entry: exact landed T1a/T1b/T2/T3. Refresh preflight, quarantine Rust, run
  immutable product from separate project, restore on all exits, and verify
  exact restored state plus repository gates.

A parent issue status never substitutes for a named task artifact.

## 7. Test plan

| AC | Proof | Independent falsifier |
| --- | --- | --- |
| AC1 | real Mac prebuilt launch + executable census | inherited PATH/checkout binary substitution |
| AC2 | real WKWebView + OS pointer + host trace | stale navigation/forged identity/slow consumer |
| AC3 | TS contract + real app-link | hold host Ready; import-time ready must not pass |
| AC4 | real fs allow/deny + OS sentinel | caller-selected authority or outside-byte mutation |
| AC5 | hostile child crash + window/document identities | reload/change nonce or accept stale token |
| AC6 | process handles/tree + next launch | omit revoke/reap attribution/order |
| AC7 | before/quarantine/restore receipts | forced product failure or altered restored state |
| AC8 | landed evidence inventory | remove one AC's evidence and require incomplete |

Fuzzing is not a KEL-139 documentary acceptance gate. Children reuse the existing
`cargo-fuzz` raw-byte targets for hostile transport inputs. If a child adds a new
untrusted parser/decoder or accepted byte grammar, that child adds or extends the
owned `cargo-fuzz` target and promotes every finding to a deterministic regression.
Structured bridge, lifecycle and product-state behavior stays under deterministic
hostile-transcript, subprocess and real-OS tests rather than blind fuzzing.

Tests wait on observable conditions; sleeps are not synchronization. Timeouts are
kill switches. Crash/lifetime hazards run in child processes. One OS never
qualifies another.

## 8. Review gates triggered

For the KEL-139 spec PR:

- unsafe: none;
- public API: **yes** — required `window.keld.invoke` surface and behavioral
  floor for KEL-142;
- permission model: **yes** — all privileged fs work remains exclusively behind
  KEL-102/KEL-130 and no renderer/Bun authority is widened;
- dependency addition: none;
- wire protocol: none — current KEL-133/KEL-136 kipc is reused.

Public-API and permission reviews bind the exact final diff. They do not approve
child code. Any child requiring new unsafe, dependency, accepted wire bytes, or
permission representation stops for its own gate.

## 9. Perf impact

No performance claim and no new benchmark prerequisite.

KEL-141 reports artifact size; KEL-144 may report cold launch-to-window using
architecture-01 census/clock semantics. A measurement is not declared within
budget unless the existing KEL-129/scoreboard contract makes it scoreable.
Children preserve existing bounded queues, deadlines, filesystem ceilings and
output bounds.

## 10. Open questions

None at the parent-contract level.

These are explicit child entry gates, not unresolved KEL-139 design questions:

- KEL-103 is still draft; KEL-141 cannot infer its signing decisions.
- KEL-102/T3 is not landed; KEL-140 cannot substitute T2/generic KEL-102.
- `@keld/api` is not tracked; KEL-142 must approve exact exported identifiers.
- KEL-80 remains broader Backlog; KEL-142 needs only its exact passed bounded
  product-spine artifact.
- macOS strict-profile evidence remains separate KEL-78 work.
