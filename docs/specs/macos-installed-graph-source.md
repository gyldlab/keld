# Spec: Generic original source and preload in a local macOS installation
Status: draft
Linear: GH-1404 (#517 fallback) · Owner: GYLDLAB · Updated: 2026-11-06

This is the proposed original-content integration amendment to
[GH-1404 macOS local packaging](macos-local-app-packaging.md) §§4.3–4.6, A12 and A14.
The user already authorized the local ad-hoc goal and routine technical decisions.
The exact source/boot/graph extension below still needs independent root review and
recorded delegated spec approval before implementation. No native acceptance is
claimed by this draft.

## 1. Goal & non-goals

Ship the original, source-pinned draw.io desktop application through KELD's real
installed macOS Host, bundled Bun, guarded Electron facade, original preload and
verified webapp resources. A *different* app with different root, entry and preload
paths must work through the same shipping framework binary. The installable .app
and DMG use existing local ad-hoc signing and ephemeral browser-profile admission.

Non-goals: a draw.io path/hash switch inside shipping Rust; arbitrary preload
execution, URL navigation or filesystem grants; a private maintainer certificate
or trust server; an additional Host/Guardian, policy engine or principal; a TypeScript
evaluator; publisher identity, Strict containment, a persistent browser profile,
notarization, an updater, Windows/Linux implementation, a native harness launcher,
or a whole-checkout copy. Source qualification is evidence of *our delivered
original app*, not an entitlement that every developer must obtain from KELD.

## 2. Spec refs and observed baseline

- [Architecture 01](../architecture/01-overview.md), [03](../architecture/03-security.md),
  [04](../architecture/04-electron-compat.md), [06](../architecture/06-runtime-and-tooling.md);
  [GH-1404](macos-local-app-packaging.md) is the approved packaging/installed-boot
  mission. [GH-499](https://github.com/gyldlab/keld/issues/499) owns graph admission,
  [GH-502](https://github.com/gyldlab/keld/issues/502) provider receipts,
  [GH-553](https://github.com/gyldlab/keld/issues/553) verified window CSP,
  [GH-484](https://github.com/gyldlab/keld/issues/484) original app preload,
  [GH-542](https://github.com/gyldlab/keld/issues/542) document ownership and
  [GH-589](https://github.com/gyldlab/keld/issues/589) the original installed
  acceptance consumer. GH-540/GH-552 and GH-480 retain IPC/menu ownership.
- Current recipient `39e6d503db74f098fe227b15030d1d0530e04f19` plus WIP:
  `crates/keld-cli/src/build.rs` captures config, policy and existing three
  `keld-launch-facts` bootstrap files; `crates/keld-pack/src/macos/capture.rs`
  owns the no-follow resource inventory and `assemble.rs:143-216` materializes
  the .app **before** signing. `keld-core/src/app_session/boot_contract.rs`
  accepts closed **boot schemas 1–4**; `launch_facts.rs` separately defines
  **installed launch-facts schema 6**, not a boot schema. Keep that codec and its
  verified pre-entry facts intact.
- Immutable original composition `a88352ae3b7c2e2f7c26019ccd7f8907baa7405e`:
  `graph_contract.rs` has graph `keld.stage.graph/v1`, fixed
  `keld.stage-graph.json`, a sorted `{path,size,sha256}` census, and
  `ApprovedGraphCaptureInput` with independently expected package.json,
  package-lock.json and **three** unpacked provider tree hashes in order
  `node_modules/electron`, `node_modules/@keld/api`, `node_modules/@keld/kipc`.
  It explicitly excludes Host, boot, policy and graph leaves from the graph.
  `original_webapp.rs`, `graph_contract.rs` and
  `wkwebview/macos_resource_source.rs` currently fix the webapp to draw.io's
  `index.html` path; `wkwebview/macos_bridge.rs` fixes an original 3,201-byte
  preload and a draw.io-specific digest. Those literals are not a generic owner.
- Original source pins are draw.io desktop
  `2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e` and webapp
  `24b76c2cbd55d88e354042e8d329a2e4708972bc`; the historical V10
  preview main differs from the pristine original. The 2026-11-06
  `.git/keld-closeout/macos-packaging-impl-20261105/resume-20261106/original-source-closure-v1.json`
  is optional review evidence, not a build input, secret, qualified provider or
  signed-release receipt. No foreign WIP is automatically adopted.

## 3. Acceptance criteria (binary)

1. **Configuration:** An explicit-Legacy local facade with closed
   `rendererSource:{root,entry}` and one build-selected preload path/digest
   selects those exact project-relative members. Unknown/duplicate/dynamic
   fields, renderer+source conflicts, missing Legacy, traversal/symlink or
   an absent required source reject before publishing an app. Existing boot
   schemas 1–4 and unmodified development/renderer behavior remain byte-compatible.
2. **Exact sealed graph:** Pack captures the declared app files and framework
   bootstrap first, then the existing GH-499 graph v1 independently verifies
   package/lock, three provider trees and the *actual copied* resource census.
   A new closed installed boot descriptor binds the graph digest, main entry,
   selected web root/entry, exact preload path/digest and permission digest.
   No unknown, uncensused, changed or symlinked app-source member may reach
   signing; Host, Bun and Info.plist retain their separate Pack checks.
3. **Independent input claims:** Wrong source package, lock, any one provider
   tree, original preload or qualified Host/CSP pair fails release-input
   qualification. A successful self-consistent ad-hoc build cannot substitute
   for an independently established artifact/source receipt.
4. **Installed origin:** Host and Guardian derive the same signed local app
   root, read the exact closed boot and graph from retained no-follow handles,
   compare graph main entry to boot entry, verify the selected source and
   preload against graph rows, and recheck before each Bun generation.
   A root/entry/digest substitution refuses without a dev lease or fallback.
5. **Guard and preload:** Only explicit Legacy plus GH-553's existing resolved
   window CSP and GH-542's actual current document admit original content.
   The app's absolute preload selector must map to the one pinned project-relative
   file, matched by SHA-256 **before** executing any JS in the untrusted app
   preload world; retain existing 512 KiB/window and 8 MiB/Host budgets,
   process-fact and argument bounds and isolated native handlers. Mismatch
   has zero partial preload effects.
6. **Navigation and resources:** An original `file://` request must match
   exactly the bound installed `root/entry`; only the existing bounded query
   value crosses as data. An alternate URL, path, fragment, query escape,
   different window/document or resource outside the selected root is denied.
   Native `keld-resource` admission still verifies graph bytes and Guard CSP;
   a graph row alone is not a browser grant.
7. **Generic native cell:** A signed, relocated macOS arm64 app with an
   unrelated source root, nested entry filename and different *pinned* preload
   actually renders through native WebKit and performs one authorized and
   one denied IPC action. Reusing the draw.io root, entry or preload constant
   makes the positive test fail.
8. **Original native cell:** The real pinned original draw.io main, original
   webapp, real packed SDK providers and unchanged accepted preload execute
   Launch → Open → Edit → Save → read back XML → unsaved Close → Cancel →
   Save/Discard → Reopen → Quit, with native observations, selected policy,
   child census and clean restart. An inert page or a HELLO receipt fails.
9. **Artifact evidence:** Independently install/relocate the signed .app and
   the DMG copy, break one sealed byte and one input qualification pin, and
   observe refusal. Record actual Host/Bun/app/graph/DMG hashes and cleanup
   outcomes. The **first Mac only** is evaluated; a second Mac, Intel and
   non-macOS targets remain unverified.

## 4. Design

### 4.1 Ownership and trust atoms

| Owner / boundary | Inputs and sole output | Failure and independent falsifier |
| --- | --- | --- |
| CLI config + GH-499 | Literal selected source, build-only expected hashes → canonical selectors/approved graph input | Duplicate/path alias; wrong independently packed provider |
| Pack capture + compiler | Retained pre-sign app tree → exact graph and boot leaves, then signature | Mutate copied leaf between inventory and signing; unsigned or unlisted byte |
| Core installed boot + Guardian | Current signed code, retained app root + graph SHA → selected source/entry per generation | Relocate, swap graph/root after open, mismatch main entry |
| GH-553 Guard | Exact verified permissions + qualified Host/provider pair → existing per-window CSP | Denied resource request creates no effect |
| GH-484 + GH-542 WK | Graph-verified source/preload image + committed document → one bounded original view | Different preload hash/path or stale document has no JS/native effect |
| GH-589 native evidence | Real installed original application → saved file and lifecycle observations | Mock editor, abandoned child or changed installed artifact fails |

Local ad-hoc signing establishes the selected bundle's integrity and internal
consistency, **not publisher identity**. The main JS runs in explicitly declared
Legacy and retains ambient OS authority outside the broker. This amendment
does not claim containment of that JS. KELD's Host-enforced IPC and webview
grants remain default-deny. Publisher trust, Strict and updater authority
cannot be inferred from the graph, ad-hoc seal, a build hash or a passed test.

### 4.2 Closed configuration, boot and graph

The sole application-facing addition is an optional literal root in
`keld.config.ts`:

```ts
rendererSource: { root: "client/web", entry: "pages/editor.html" }
```

It is valid only with `initialWindow: "none"` and explicit
`/profile:"legacy"` in the existing permissions manifest. Both values use
the existing GH-499 canonical project-relative path grammar; `root` is a
directory under `Contents/Resources/app`, and `entry` is a regular file
*relative to that root*. They must not select a protected authority, test,
credential or provider directory. No `file://` URL, absolute directory,
port, glob or permission string is a configuration value. `rendererSource`
does not change the existing initial renderer contract.

The **build-only** `keld.build.ts` closed graph qualification branch adds
`graphSource: {packageSha256, lockSha256, providerTrees:
{electron,api,kipc}, preload:{path,sha256}}`. Each digest is exactly 64
lowercase hex characters, and `preload.path` is a single canonical project-
relative file in the known original app source. The existing independent
`host.sha256` input serves the Host pairing; do not duplicate it as a new
credential. These values are **claimed expected data**, not trusted just
because the app author wrote them. They contain no secrets. For **our**
distributed original draw.io app, the release owner must separately bind
them to reviewed desktop/webapp commits, real source file receipts, actual
packed-and-extracted electron/API/KIPC artifacts and the qualified Host
input *outside the target app tree*. Every mismatch blocks original-release
qualification; absence of that record is an explicit open gate. Other
developers can build their own cooperative local ad-hoc apps without a
private KELD account or maintainer signature, but cannot claim KELD-qualified
original draw.io evidence on their own hash assertions.

Add a *new exact closed installed graph-facade boot variant* (proposed boot
`schema:6`, unrelated to launch-facts schema 6). Its fields are the
existing allowed application metadata, `entry`, fixed
`permissions:{file,content_sha256}`, fixed
`graph:{file:"keld.stage-graph.json",content_sha256}`,
`rendererSource:{root,entry}`, and
`preload:{path,content_sha256}`. No `renderer`, `auth`, `profile`,
`sourceUrl`, alternate graph filename or extra keys. The Guard manifest
remains the only profile/grant declaration. Core compares boot entry with
`GraphDocument.entry`, verifies every selected source/preload row exists
in its bound graph and rechecks the current root. No graph v2 is needed:
the existing *closed* `keld.stage.graph/v1` already binds all file
paths, lengths and digests; the signed boot owns the additional **selection**
without creating another graph authority. If a demonstrable validator
invariant forbids that reuse, the owner must document the exact failing
control and obtain a separate closed v2 review before introducing it.

### 4.3 One pre-sign capture/compile transaction

Retain Pack's `Capture::plan` / `Capture::materialize` and
`assemble_signed_app` transaction. Extend its **graph mode only** with
a tightly typed, one-shot pre-sign finalize seam immediately after
`Capture::materialize`, before `sign_bundle`. This is a caller-bound
compiler operation, not a general resource writer: Core/CLI receives a
retained, no-follow **copied** app-root handle and bounded captured inventory,
and returns only the two fixed generated leaves
`keld.stage-graph.json` and `keld.boot.json`. Pack reserves these names
in its collision plan, writes them once, reopens and checks their exact bytes
and identities, rechecks the complete inventory and then signs. The existing
plain build mode keeps its current precompiled boot path.

Order for graph mode: validate declared Legacy/policy and independent input
pins; plan only explicit app main, selected source root, single preload,
package.json, package-lock.json and qualified provider trees (plus required
inert app resources); capture those and the existing **three**
`keld-launch-facts` generated bootstrap files into Pack's owned staging
app root without symlinks/special files; GH-499 preflight and graph-v1
census the *copied* bytes, including generated bootstraps, excluding Host,
Bun, boot/policy/graph authority leaves under their existing rules; compile
the closed boot with the generated graph SHA and exact captured policy
digest; seal the two leaves; recheck same physical inventory, provider
pins and identity; sign inside-out, verify copied .app and DMG, and publish
only on success. The graph must describe **what is sealed**, never an
earlier mutable checkout or a post-sign recomputation.

The existing Pack path and size ceilings, output overlap checks, no-follow
opens and excluded authoring/secrets (`.git`, `.keld`, `.env*`,
`keld.config.ts`, `keld.build.ts`, private keys, credentials, tests,
private evidence) remain in force. No unlisted checkout copy, implicit
node_modules search or separate shadow capture implementation. If the
pre-sign hook cannot preserve this inventory identity, refuse graph mode;
do not replace it with a second copier or self-generated expected hashes.

### 4.4 Runtime selection and preload

Extend the original GH-499 `AdmittedStageGraph` verified-root/accessor,
GH-484 `AppPreloadBudget` and GH-542 WebKit resource/document owners.
They take the *Host-validated boot selector* instead of fixed
`drawio/src/main/webapp/index.html`, `src/main/electron-preload.js`,
`index.html` or `ORIGINAL_DRAWIO_PRELOAD_SHA256` constants.
A descriptor-relative no-follow walk opens only `rendererSource.root`;
selected `rendererSource.entry` is present in verified assets. The
main-role absolute `webPreferences.preload` input must match exactly
`installedAppRoot/preload.path`, and the retained, bounded image must
match both the graph row and the **boot-bound build-selected digest**
before execution. Our delivered original app additionally requires an
external, independent source qualification receipt. Preserve per-window
arguments,
captured process versions, app-world/private registry, native document
owner, per-window and aggregate preload reservations. A missing/changed
pin refuses; merely finding a file in the graph does not authorize its
execution. Do not invent generalized `require` or `contextBridge`
capabilities.

The original app's `loadFile` physical URL is an untrusted selector:
accept only the exact file URL for the bound installed root/entry; retain
the current bounded query parser as **data only**. The URI cannot supply
a root, asset, CSP choice, alternate entry or native principal. Native
resources remain on the existing `keld-resource` scheme, owned current
document and GH-553 resolved CSP, with per-file graph hashes and
no-follow reads. Validate Host/Guardian bundle root, boot/graph/policy
integrity and selected preloads at cold admission and on each generation.
The existing installed launch-facts schema6, three bootstrap modules and
private `local-bundle-v1` parent/Guardian role are reused without a
second launch-facts record or a fake development lease.

No new application KIPC channel, trusted service, policy/permission schema
or native principal is introduced. Current development/renderer boot
schemas 1–4 and their producer/parser behavior stay unchanged; the
frozen donor's development graph schema5 and developer-stage launch-facts
schemas 4–5 are **not** reinterpreted as installed boot authority.
Existing app-session, IPC, dialog, menu and Quit owners supply only
their previously approved, selectively reviewed original behavior.

## 5. Boundaries

Implement selectively after approval in CLI `src/build{,/config}.rs` and
the shared `src/boot.rs` compiler; Pack
`src/macos/{capture,assemble}.rs` as the *only* file capture/signing owner;
Core `src/app_session/{boot_contract,macos_installed,graph_contract,original_webapp}.rs`
plus its existing app-session/Guardian handoff; Guard's GH-553
window-CSP selection; WebKit GH-484/GH-542 original preload/source
delivery; existing Electron/API/KIPC generated bootstrap/facade callers.
The last four Core paths are an **ownership map**, not evidence that the
missing graph/original modules have already been integrated.

The frozen `a88352ae` source alone is not a compile-complete recipient:
the November 6 closure record identifies Core
`compat_router.rs`, `menu_router.rs`, `single_instance.rs`, native
`menu/macos/target.rs`, Electron `session-unsupported.ts` and
generated API `menu.generated.ts` as additionally needed by that
composition; only the existing generator may produce generated code.
Adopt each dependency by current-caller need and owner review. Do not
bulk-import the frozen tree or overwrite active installed launch facts.

Do not touch Windows/Linux production code, root `AGENTS.md`,
workspace manifests, global IPC wire format, private credentials,
foreign working trees, main branch or unowned native/GUI work.

## 6. Tasks (ordered vertical slices)

- [ ] **T0 — independent exact-content approval.** Root reviews this
  amendment against GH-1404/GH-499/GH-553/GH-484 and records delegated
  approval plus exact revision. Until then, implementation is blocked.
- [ ] **T1 — generic installed source, real native cell.** Add the closed
  installed boot/source compiler and the one Pack pre-sign seam; integrate
  only required graph/CSP/preload owners with their existing protections.
  Sign, relocate and execute a **non-draw.io** app whose root, nested entry
  and pinned preload all differ from draw.io. Prove current-document native
  load, a permitted IPC effect, a denied IPC non-effect, and deterministic
  source/preload substitution refusal before proceeding.
- [ ] **T2 — actual original app.** Supply an independently extracted
  provider/Host/source qualification receipt; package the exact accepted
  original draw.io main/webapp/preload and required generated SDK peers;
  reconcile only failing compile-owner imports and the existing
  dialog/menu/IPC/quit contracts. No alternate loader or test-only main.
- [ ] **T3 — installed workflow closeout.** Exercise the entire original
  Open/Edit/Save/unsaved-Close/Cancel/Save/Discard/Reopen/Quit sequence
  on the signed, relocated .app and verified DMG copy with real file
  readback, child-generation and cleanup observations. Final evidence
  and the second-Mac gap are reported separately.

## 7. Test plan

| Criteria | Independent oracle and single-fault control |
| --- | --- |
| 1, 2 | Closed parser/unit + Pack real-tree test. Inject duplicate/unknown keys, `../`, symlink root, missing/case-colliding entry, unexpected file, resource/boot collision; no published artifact. Mutate a copied file after census before signing and require refusal. |
| 3 | Change package SHA, lock SHA, each of the three actual extracted provider tree hashes, selected preload digest and separately qualified Host; require the *affected boundary* to fail. Self-derived installed-tree hashes cannot count as independent receipt evidence. |
| 4, 5 | Actual signed app subprocess and Guardian generations: alter boot graph SHA, graph entry, selected root or preload file; change root identity after opening. Check no Bun or native preload effects, and a healthy fresh app still starts. Mutate one byte before preload execution and verify zero marker. |
| 6 | Real WK main-document test: bound `file://.../pages/editor.html?x=%252f` preserves query; sibling file, foreign root, fragment, encoded traversal, wrong authority and stale document refuse. Native resource outside selected root yields no body, even if listed for Bun. |
| 7 | Different named app/source/preload and signed relocation, native WebKit load + Guard-allowed/denied effects; removing the selected-root binding fails this fixture. |
| 8, 9 | Exact original pinned application with OS-visible window/menu/dialog activity, saved XML readback, unsaved veto/Cancel and orderly Quit; independent PID/start identity/relaunch and copied DMG verification. Change one post-sign byte and one external source pin. |

For each control capture the real command, input identity, observed
exit/error, absence of forbidden effect and owner. No mock/inert
fixture proves the installed workflow. Preserve the current app's
`cargo fmt --check`, warning-denied clippy, workspace tests, strict
Bun/TypeScript, generated-code and OS/native checks under the existing
workflow; report unrun ones as unverified. Test against owned temporary
copies without sleep-based synchronization.

## 8. Review gates triggered

**Public API/config:** additive literal `rendererSource`, build-only
expected-source object and closed installed boot variant; independent
compiler/architecture review. **Permission model:** GH-553 original
selected CSP and exact Legacy/Guard boundary; independent security
refutation. **Unsafe:** GH-484/GH-542 native source/preload changes
require exact operations allowed by nearest crate rules and
independent native review. **Wire protocol:** closed *private boot*
schema changes, no public KIPC channel change; review its Host/Guardian
reader and stale-generation behavior. **Dependencies:** none planned;
a new dependency reopens the gate. Independent reviewer must see the
exact final diff before any implementation qualification claim.

## 9. Perf impact

Cold graph capture/readback increases one-time packaging and installed
admission I/O; preserve GH-499's existing 8 MiB graph limit, 32,768
file cap and streaming hashes and GH-484's 512 KiB preload per window
and 8 MiB aggregate. Compare native startup and retained memory against
the same installed inputs before making a performance claim. No new
runtime/service/process or performance gain is proposed.

## 10. Open questions / approval and evidence gates

No additional user account, maintainer secret or signing purchase is
required for the approved **local ad-hoc** goal. Pending gates are
owner-owned evidence and review: (1) root's exact-content approval of
this draft; (2) independent original source, Host and packed/extracted
provider qualification values, including an immutable preload pin,
none supplied by the historical V10 ZIP alone; (3) proof that Pack's
one-shot graph finalizer binds the *copied* tree without introducing a
parallel resource reader or graph-version change; (4) the required
native generic/original acceptance. The second Mac remains unverified.
Any failure keeps that criterion open and names its existing owner;
it is not cured by widening Legacy, CSP or preload admission.
