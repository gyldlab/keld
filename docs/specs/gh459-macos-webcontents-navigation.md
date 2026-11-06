# Spec: macOS webContents identity and load completion
Status: draft
Linear: GH-459 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; docs only, do not merge.

## 1. Goal & non-goals

Expose the existing host-minted webview identity and a correlated loadURL path whose Promise resolves after the matching
did-finish-load listeners, without weakening resource or navigation authority.

Non-goals: GH538 owns full navigation/failure vocabulary; GH462 owns result evaluation; GH543 owns message delivery. Policy,
preload worlds, new-window admission and session ownership remain separate.

## 2. Spec refs

- [Owning issue GH-459](https://github.com/gyldlab/keld/issues/459) and its latest public owner decisions.
- [Architecture 05](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/05-webview-and-native.md): native ownership, engine boundary and capability separation.
- [Current facade and identity](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/electron/src/browser-window.ts).
- [Engine navigation](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-wv/src/engine.rs).
- [Native implementation](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-wv/src/wkwebview/mod.rs).
- [Registry owner](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-core/src/app_session/window_registry.rs).
- [N1 wire collision and policy correction](https://github.com/gyldlab/keld/issues/459#issuecomment-6560238948).
- [Single navigation-authority dependency](https://github.com/gyldlab/keld/issues/459#issuecomment-6371834142).
- [Electron webContents contract](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/web-contents.md).

Verified baseline: Current main's WebContents facade has the destroyed surface but no loadURL. WebEngine.navigate returns command
admission, not completion. The N1 record identifies a real proposed wire-tag collision and an unresolved CSP selector; it is
review material, not a qualified runtime implementation.

All proposed API, schema, ownership and permission changes below are future contracts. This documentation PR changes no runtime
boundary and approves none of those changes.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given two host registry windows, when their webContents.id values are read, then they differ and each equals its own host-minted
webview identity.

Independent negative control: Replace host identity with a facade-local counter; assert equality against native readback.

### AC2

Given an admitted file load, when the matching native finish arrives, then did-finish-load listeners run once before the
associated Promise resolves.

Independent negative control: Resolve on command admission or commit rather than matching finish.

### AC3

Given a cross-document commit followed by a hash change, when events arrive, then only the commit emits did-navigate and the hash
change preserves navigation identity.

Independent negative control: Mint another generation or emit did-navigate on the hash change.

### AC4

Given destruction while a load is pending, when the host releases the webview, then destroyed fires once, isDestroyed is true and
pending work terminates once with the registered destroyed outcome.

Independent negative control: Keep the old completion callback alive and resolve it after destruction.

### AC5

Given stale caller, old attempt or retired document completion, when it arrives, then it cannot settle the current load or restore
prior authority.

Independent negative control: Match only webview ID and ignore attempt/document/role correlation.

### AC6

Given the draw.io finish-listener fixture, when it sets zoomFactor=1 and awaits setVisualZoomLevelLimits(1,1), then it completes
under explicitly recorded registration-safe divergences.

Independent negative control: Remove the stub or falsely report nondefault zoom was applied.

### AC7

Given registered future-policy/new-window handlers, when a page calls window.open before that feature is qualified, then
registration succeeds but no window is created and no allow-all handler is installed.

Independent negative control: Install an unconditional native new-window handler.

### AC8

Given the shared CreateWithPreload and LoadUrl schema candidates, when independent vectors are decoded by Rust and TypeScript,
then both variants remain distinct and all existing vectors retain their meaning.

Independent negative control: Assign both requests the same numeric tag; both decoders' discriminant controls must fail.

## 4. Design

### 4.1 Host identity and pending record

Reuse GH531's host registry and existing webview IDs. Retain at most one admitted pending load per view for this first slice; a
second concurrent call is rejected with a registered busy outcome until the broader supersession contract is approved. Correlate
authenticated role, view, current navigation identity and native attempt; never accept identity from a page payload.

### 4.2 Navigation authority

Consume GH542's one host-owned navigation state. A trusted explicit full-document load revokes before engine entry; matching
replacement commit uses that owner's checked transition. Failed/cancelled loads do not resurrect the old document lease.
Same-document preservation requires trusted classification, not URL-string similarity or a renderer claim.

### 4.3 Completion and failures

Engine command success means admission only. Matching finish emits listeners, then settles that request. Engine refusal,
destruction and role/authority retirement terminalize pending state once; do not leave a Promise hanging because full
did-fail-load vocabulary is deferred. GH538 supplies complete error-event semantics without creating another completion owner.

### 4.4 Wire allocation

Preserve the N1 correction: old request tags and concurrent CreateWithPreload/LoadUrl candidates must be reconciled in one owning
schema ledger. Do not assume tag 7 or fall back to tag 6. Generated Rust/TypeScript codecs and independent literal vectors land
together. No second parser, channel or implicit navigation during create.

### 4.5 Resource and CSP gate

File containment stays with the existing resource owner. The exact compiled windows.<key>.web.csp selector and trusted window
binding need the manifest/policy owner's qualified decision. A raw response header or unchanged source hash is not policy proof.
Use unchanged original assets, truthful URL/UA/query and positive/negative inline/wasm/resource probes; never loosen script
authority to obtain a happy path.

### 4.6 Compatibility surface

Keep zoom default 1 and diagnose unapplied nondefault values. setVisualZoomLevelLimits is a documented no-op Promise for the
scoped compatibility slice. before-input-event and future navigation handlers are registration-only; pasteAndMatchStyle is a typed
unsupported operation. will-attach-webview never emits while webviewTag is unsupported. The facade remains the permanent Electron
vocabulary owner.

### 4.7 Evidence and implementation admission

Use the cited immutable source and re-read live owner claims before implementation. A public issue receipt is reported evidence,
not a test run performed by this drafting session. Reuse current owners and generated contracts; approval of this draft must not
silently approve an upstream draft dependency. Keep each unavailable native or policy prerequisite visible.

## 5. Boundaries

Future implementation stays in BrowserWindow/WebContents, the existing window request/completion owner and current native
navigation adapter. Do not introduce a new WebEngine method for this first slice or assign policy by issue label.

This publication changes only `docs/specs/gh459-macos-webcontents-navigation.md` and grouped-PR/issue metadata. Shared policies,
architecture, generated schemas, code and unrelated work remain with their existing owners.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Approve N1's exact shared schema and CSP/window-binding owner records; record pending identity/order/registration cells.

### T2

- [ ] Implement authenticated load admission and one correlated completion record through the current native seam, including
terminal failures.

### T3

- [ ] Add the permanent facade members and registration-safe divergences with independent decoder/identity/order controls.

### T4

- [ ] Run real native unchanged-original file navigation, resource/CSP negatives and destruction/stale-attempt races; update only
proved cells.

## 7. Test plan

Every AC has a distinct observable and negative mutation in section 3. Map the resulting tests and evidence rows to GH-459;
preserve pending-fail/unknown status until the actual implementing commit passes the relevant fixture.

Drive independent Rust/TypeScript literal vectors and actual host identity readback. Separate command admission, native commit and
finish. The real original-app fixture must preserve source bytes and prove guarded resource/CSP behavior; model success cannot
qualify navigation authority.

Documentation validation in this batch covers immutable reference/readback, all ten sections, independent controls,
source-versus-proposal wording and file limits. Runtime, native and original-app tests are planned, not executed. GitHub CI status
belongs to the exact published head; a documentation-only green run does not prove platform behavior.
The user restricted this session to GitHub. Local `just ci`, native execution, hooks and machine session closeout were not run.
Official upstream GitHub sources were used within that source restriction; no Context7 or external-machine evidence is claimed.

## 8. Review gates triggered

Public API, existing wire-schema owner and resource/permission owner alignment. The unresolved N1 wire/policy conditions block
implementation, not docs publication.

Status remains draft. Implementation admission requires the exact owning decisions in section 10 and approval of this contract.
Draft PR publication is not that approval.

## 9. Perf impact

One bounded pending load per view, existing transport limits and one authority owner. Measure retained records after
failure/navigation churn; no startup or paint timing claim.

No numerical product performance claim is made without a reproducible pinned workload, environment and receipt.

## 10. Open questions (block implementation)

1. Approve the shared request-tag ledger without collision.
2. Qualify the exact CSP selector/profile/window binding and native attempt classification.
3. Approve initial concurrent-load refusal and terminal error registration with GH538 and the central error owner.

Independent review must refute or fix concrete findings on the published revision. Remaining platform qualification is an
acceptance gap, not evidence of a completed feature.

