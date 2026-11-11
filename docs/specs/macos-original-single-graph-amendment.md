# GH-1404 proposed original-graph entry bootstrap amendment

Status: **PROPOSED — NOT APPROVED FOR SIGNED RELEASE** (2026-11-10).
Tracking: [GH-1404](https://github.com/gyldlab/keld/issues/1404).
Source review: [draft PR #1469](https://github.com/gyldlab/keld/pull/1469).

This is an amendment proposal, not a replacement for the exact-content
approvals of [macOS local packaging](macos-local-app-packaging.md) and
[installed graph source](macos-installed-graph-source.md). Any adoption of
changed launch semantics REQUIRES fresh security, CODEOWNERS, and original-app
compatibility approval. Earlier approvals do not apply automatically.

## 1. Problem and evidence

The reviewed original-graph Guardian launches pinned Bun with the immutable
installed launch-facts preload, the graph-verified API runtime-facts preload,
and then the verified original app entry. Bun 1.4.4 on Windows evaluates CLI
`--preload` and the entry in different module caches. The preload authenticates
the Host link and receives RuntimeProcessInit ACK; the entry creates a second
authenticated link whose first lifecycle CALL is Subscribe rather than Init.
The original entry then fails its genuine Host Ready deadline with
`KELD-IPC-006`. Increasing the timeout, reimporting the wrapper by another
package alias, or guarding `parentPort` did not repair it.

The root-level single-graph proof [run 42809556689](https://github.com/gyldlab/keld/actions/runs/42809556689)
passed on actual Windows Bun with published API/KIPC packages: one link,
genuine HELLO/Init ACK/subscriptions/Host Ready, then user entry and clean
exit. The independently Host-selected package-private
`bootHostSelectedEntry` helper has additional missing selector/endpoint,
invalid loader and rejected Init ACK refusals. This evidence does NOT prove
that the signed macOS original application accepts new main-entry semantics.

After negative Windows runs on runtime-variable path imports, the exact
app-root **static literal** import compiled from the original selection
passed hosted Windows in [run 42827682061](https://github.com/gyldlab/keld/actions/runs/42827682061):
the real packed test entered the original after Init/ACK on one link and
received Ready. The two old CLI dual-phase preload tests remained failed.

## 2. Proposed source and custody

The installed original graph SHALL have a **fourth Core-owned generated
file**, `keld-installed-runtime-entry.mjs`, at the signed app root. For an
original graph, CLI SHALL deterministically render its exact bytes from
Core's compiled source template and the independently selected original
entry path **before** Pack captures the closed graph and signs the bundle.
The same single renderer SHALL emit a syntactic literal ESM
`import("./src/main.ts")` binding, not a variable or app-selected runtime
specifier. Non-graph installed builds keep inert fixed template bytes and
their existing direct-entry route. The private app-root module is neither
an application-selected resource nor a public SDK export. It MUST be part
of the graph census.

Core SHALL independently read the generated file by a retained no-follow
handle, regenerate expected exact bytes from the *sealed boot-selected entry*
using the same compiled trusted template and renderer, compare them to the
opened file, and retain its file identity. The Guardian SHALL revalidate the entire
original graph, selected original entry, generated files, policy and bundle
identity before **every** child generation. An absent, substituted, re-signed,
or same-size-tampered generated file MUST cause refusal before user code.

The existing immutable launch-facts preload remains first. Only in an
independently admitted installed-original graph, Guardian SHALL run the fixed
app-root generated module as Bun's one entry and supply the already retained
boot/graph-selected original entry as its only argument. The fixed module
MUST invoke the existing `bootHostSelectedEntry`, await the genuine Host
RuntimeProcessInit ACK and both required subscriptions, and only then
dynamically import that original entry in the **same ESM module graph**. The
original entry path MUST NOT come from the application's config, mutable
environment, IPC, renderer input, or an unverified argument.

The existing development, non-graph installed, and utility routes SHALL keep
their established direct-entry contract. No new KIPC channel, Host principal,
authentication source, global capability registry, retry, fake Ready, or
relaxed I/O deadline is introduced. The trust boundary remains the signed
Host, its retained boot and graph, the Guardian, and the actual KIPC token.

## 3. Module and path compatibility

Running a private bootstrap as Bun main **changes the imported original's**
`import.meta.main`, `Bun.main`, and `process.argv[1]`. The approved original
contract formerly expected `[bundled_bun, packaged_entry]`. The proposed
bootstrap yields `[bundled_bun, verified_bootstrap, packaged_entry]`, and the
original entry is a dependency rather than the Bun main. These differences
MUST be challenged against the pristine original draw.io application and
other supported original graphs. Keld MUST NOT spoof these properties through
untrusted globals, invent launch facts, or silently present the change as
equivalent. Product approval is blocked until all original-entry-dependent
behavior is demonstrated to remain correct or an explicit reviewed contract
change is approved.

Bun can normalize macOS `/var` and Windows short-path aliases differently.
The root module SHALL compare actual filesystem identity of the selected
entry's parent chain with the verified bootstrap directory, reject symlink
escapes and root substitutions, and treat ambiguous inode/device responses as
failure. The pre-sign Core renderer SHALL accept only canonical
**URL-neutral ASCII original-relative names** in the current pinned Bun
version: letters, digits, slash, dot, underscore and hyphen, with no
dot/dotdot components. All `#`, `%`, `?`, literal `%2e%2e`, backslashes,
quotes and other unsupported names SHALL fail at producer/admission rather
than be interpreted as a query, fragment or path traversal. Parent bundle
directory spellings remain independently qualified and MUST NOT be rejected
simply for a URL-sensitive character outside the *selected relative entry*.
The dynamic ESM import argument SHALL be a **literal** selected by the
pre-sign producer, with runtime physical identity check against the Host
argument. This is a **compatibility limitation requiring approval**, not a
claim that all graph-v1 names are supported.

## 4. Mandatory regression and acceptance gates

1. **Pure source tests:** assert precisely four Core-generated owner leaves
   and byte-for-byte identity, including the fourth root bootstrap. Confirm
   compiled app-root path, independently sealed original entry and non-graph
   fallback, and reject same-root wrong-name bootstrap, wrong mode or missing
   matching graph selection before constructing a child command.
2. **Real installed SDK on Windows Bun 1.4.4:** run the exact rendered
   bootstrap with genuine HELLO, first RuntimeProcessInit/ACK, both
   subscriptions, Host Ready, one authenticated link and clean exit. Retain
   negative missing Host selector, nonexistent endpoint, bad ACK, invalid
   entry/path, traversal, attempted extra connection and original 5-second
   deadlines. Test unsupported original-relative `#`, `%`, `?` and literal
   `%2e%2e` path components for pre-sign refusal, and prove a changed runtime
   original argument cannot redirect the sealed literal import to another
   module.
3. **Original legacy acceptance migration:** the owner of dirty
   `packages/@keld/api/src/runtime-preload.test.ts` MUST retain its strict
   Init/ACK/Ready, one-link, early-runtime-facts, tamper and refusal checks
   when updating Windows to the supported single-graph route. No skip,
   waiver, assertion reduction or deadline increase is acceptable. Keep
   supported legacy preload behavior tested on applicable platforms.
4. **Signed macOS release and provenance:** independently verify the actual
   generated Pack app, codesign, copied/re-located app and DMG, eight
   generated-source removal/tamper refusals, wrong original graph or policy,
   changed original/main inode and per-generation reopening. Preserve
   signed Host/Guardian private invocation identity and app role ownership.
5. **Real original application:** verify pristine draw.io cold startup,
   app path/argv/main semantics, renderer loading, Open/Edit/Save with XML
   readback, Save/Discard/Cancel, dirty close/reopen, Quit, relaunch,
   process descendant retirement and zero orphaned sessions on an
   exclusively claimed macOS device. A synthetic test role is insufficient.
6. **Exact-head hosted CI:** Rust Clippy/tests and Bun/typecheck across all
   supported hosted platforms, Gitleaks, CodeQL, provenance, dependency
   review, CODEOWNERS and aggregate `CI required` MUST pass. The historical
   original #1447 Gitleaks ancestor still needs security adjudication; no
   scanner exclusion, hidden secret, history rewrite or forged green status
   is permitted.

## 5. Approval and disposition

The source candidate SHALL stay in a draft, unmerged PR until the required
independent security and signed-original owners approve its exact final
diff and all native acceptance artifacts. The active owner's five dirty
GH-1404 files SHALL remain untouched until ownership is explicitly
transferred or their owner commits a reviewed migration. A main-based
integration PR MUST preserve the original work's provenance and satisfy the
real main-based CI policy; a stacked PR's locally green gate is not a
substitute.

The recorded Windows proof supports the one-graph mechanism only. It does
not waive compatibility review, the old Windows failing tests, the signed
app/DMG work, or historical security findings.
