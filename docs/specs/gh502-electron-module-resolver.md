# Spec: one installed Electron module resolver (GH-502)

Status: approved

Linear: GH-502 (GitHub tracker under #517) · Owner: @0monish / GYLDLAB · Updated: 2026-10-28

Authority: [GH-502](https://github.com/gyldlab/keld/issues/502) latest Agent Brief; [X02-A1 binding decision, GH-412](https://github.com/gyldlab/keld/issues/412); [X03-T3 conformance owner, GH-569](https://github.com/gyldlab/keld/issues/569). Original design basis: `origin/main 522acf123f703a450a1710efea5ddf677470c10e`.

Disposition: **OPEN / DRAFT / DO NOT MERGE**. This draft neither authorizes implementation nor establishes installed-app/OS/product evidence.

## 1. Goal & non-goals

Make installed `@keld/electron` the **single package** consumed by an application dependency aliased under the name `electron`. In one application role, the bare module and four subpaths (`electron/main`, `electron/renderer`, `electron/common`, `electron/utility`) resolve through one package `exports` map to one entry. CommonJS (CJS) `require()` and ECMAScript module (ESM) imports from both app code and an installed `node_modules` dependency observe the same `app` singleton. A genuinely distinct second evaluation within the same role fails with a typed duplicate-entry diagnostic.

The dependency alias in a migrated application's `package.json` is **X02-T3's** responsibility. GH-412 selected package alias + exports, rejecting preload-plugin, `bunfig.toml` alias and `tsconfig.paths` as competing runtime resolvers.

Non-goals: no app-side migration output (#497/#499), root workspace changes, new facade members (BrowserWindow, dialog, shell), process.* fields (X03-T4), boot payload (#452), updater adapter (F09-T1), auto-install guard (X03-T6), Host/renderer (#446/#449), KIPC, permission, sandbox, source policy or native draw.io product scoring (#589).

## 2. Spec refs and actual baseline

- `docs/architecture/04-electron-compat.md` sections 2–3.1 currently document the KEL-72 `tsconfig.paths` resolver and inert `bunfig` table. GH-412's **later approved decision** is package-manifest dependency alias. #497's [draft PR #1172](https://github.com/gyldlab/keld/pull/1172) already proposes an architecture/migration correction; GH-502 can consume it only after that owner's review/integration. No architecture-file edit belongs to this spec branch.
- At inspected `origin/main 41b81ecbf0f56b753aebd52cfe81b233f009965c`, `packages/@keld/electron/package.json` still has only `"exports": {".": "./src/index.ts"}` and no declared runtime dependencies. Its default object now contains `app` and the implemented `BrowserWindow` subset; named exports also include `ELECTRON_VERSION_SHIM` and `isCallError`, so named/default key parity remains unqualified. The `@keld/api` and `@keld/kipc` package manifests also declare no runtime dependency edges. No `workspace:*` dependency is present in these three inspected manifests.
- `packages/@keld/electron/src/app.ts:8` and `src/link.ts:7` import/re-export `../../api/src/*`. In turn `packages/@keld/api/src/link.ts` imports `../../kipc/src/transport.ts`. These paths escape package roots. Source-tree success therefore cannot prove package closure in isolated `node_modules`.
- `packages/@keld/electron/fixtures/tsconfig.json` still maps bare `electron` to source. Its `fixtures/bunfig.toml` retains the documented inert `[alias]` table. Both are slated for deletion/replacement within the KEL-72 **owned fixtures**, not in migrated apps.
- [Node.js v26 package exports](https://nodejs.org/api/packages.html) documents subpaths and import/require conditional resolution; [Bun package alias guidance](https://bun.sh/guides/install/npm-alias) documents installed alias names. The GH-412 receipt cites Electron **v44.4.5**'s five-module resolver source, historical Bun **1.4.2** and Node 26 probes. **Current main pins Bun 1.4.4**; no historical execution claim transfers to it.
- GH-569 owns the prior, pinned Electron-v44.4.5 conformance cells and KEL-74 expected-status semantics. Neither this doc nor later GH-502 production may mint a product result or alter its scorer.

## 3. Acceptance criteria (binary and falsifiable)

| ID | Observable required of a subsequent, approved implementation | Independent negative control |
|---|---|---|
| R01 | All five public specifiers in a disposable installed consumer resolve through the aliased package to exactly **one physical entry file** | Route `./utility` to a second file; target identity test RED |
| R02 | Each of five specifiers × ESM default, ESM named `app`, ESM namespace and CJS require × caller in app or real installed dependency resolves to the same `app` instance, on actual macOS Bun and Node 26 | Make `require` and `import` conditions point to different modules; exact identity RED |
| R03 | Default's enumerable Electron member keys exactly match the facade's public ESM named exports (excluding `default` itself) | Remove ESM `default`, or retain named-only `isCallError`/version helpers: export-set test RED |
| R04 | Fresh installed `require('electron')` synchronously returns the facade; no top-level await in the **resolved dependency graph** | Insert top-level await or unresolved TypeScript in the graph; require fails with a concrete error |
| R05 | A packed install, with the repository source tree unavailable, resolves imports within the packed package or *declared installed* runtime dependencies (`@keld/api`, `@keld/kipc` where required) | Leave `../../api`/`../../kipc` relative source escapes or omit a required dependency; isolated install fails |
| R06 | Cached imports of one canonical entry are accepted; a distinct second entry evaluation **within the same role/realm** refuses with typed `KELD-COMPAT-*` code, first and second observed locations, and corrective guidance | Explicitly evaluate two distinct genuine Keld entry URLs in the permitted duplicate fixture; typed refusal required. A competing configuration is independently rejected by R08 |
| R07 | The owned installed-graph/launch admission rejects a missing alias or a target outside the approved installed `@keld/electron` artifact before evaluating application or dependency code that can import `electron`. GH-502 consumes this prerequisite; its package-local duplicate guard does not supply it. No real npm Electron module or executable-path fallback is evaluated. | Remove the alias or substitute/shadow it with the actual foreign npm Electron package; admission must fail before either foreign or app entry is evaluated. Any error after importing the foreign package, including a path-type error or duplicate diagnostic, is RED |
| R08 | KEL-72 fixtures use the same installed dependency mechanism and retain existing lifecycle/transport behavior; their obsolete `tsconfig.paths.electron` and inert `bunfig [alias]` are removed | Restore a competing `tsconfig.paths.electron` mapping and require the owner's configuration/graph admission to reject it. Separately, evaluate two distinct Keld entries in the permitted duplicate fixture and require R06's typed diagnostic. Restoring inert `bunfig [alias]` remains a static configuration-presence rejection |
| R09 | At independently pinned #499 versions, `electron-store`, `electron-context-menu`, `electron-is-dev` (ESM default), `electron-dl` (ESM named) and `electron-log` (CJS require) load the shim **from their own package code** | Substitute the npm path string or unrelated shim; import-time behavior/identity RED |
| R10 | The installed package records actual Bun 1.4.4 version/image, Node 26 version, OS/arch, package and lock digest. Historical Bun 1.4.2 is a separately measured regression lane | Re-label an observation from another runtime/version or platform; lane admission RED |
| R11 | After separately owned GH-569 conformance cells land, real resolver tests flip their reviewed expected states only with passing evidence; unrelated X03-T4 process cells remain with their owner | Skip/ignore a named test, or flip fail/unknown without real case; admission RED |
| R12 | The current KEL-72 `app` lifecycle, error propagation, version honesty and sole canonical worker transport remain intact; import-only proofs never produce GH-589 app/OS pass | Fake a product success from a namespace import; scorer remains nonpublishable |

R01–R12 observables are distinct, with pre-evaluation installed-graph admission an explicit prerequisite edge into execution tests: an exports-map pass, package-data install, duplicate guard or no-auto-install pass cannot substitute for it. A valid exports map cannot prove dependency closure, a no-output acceptance cannot prove a live case, and package import success cannot prove application activation.

## 4. Design

### 4.1 Boundary atoms, owners and failure modes

| Atom / source of authority | Input to output | First falsifier |
|---|---|---|
| GH-412 / X02-T3 installed dependency | `package.json` alias → installed `node_modules/electron` | Delete alias, verify no silent npm-path fallback |
| GH-502 exports | Five specifiers → one physical file | Map one subpath elsewhere |
| GH-502 with API/KIPC owners | Packed sources + declared dependencies → isolated complete graph | Remove sibling source tree, require/import fails |
| GH-502 CJS/ESM facade | One synchronously loadable module → default/named/require identity | Remove default or split require target |
| GH-502 duplicate guard | Cache key + first canonical location → reject only a different actual evaluated entry | Load one URL twice then different URL in one realm |
| GH-502 KEL-72 fixture migration | Locked fixture alias → same module from fixture/dependency | Restore competing `paths.electron`; the owner’s configuration/graph admission refuses it. R06 separately tests two actual Keld entry evaluations |
| GH-569 conformance | Pinned normative reference + live case → honest expected status | Missing/ignored test or unsupported green result |
| GH-589 product evidence | Actual Host/OS event and receipt → product record | Import-only marker falsely claimed as document save |

**No process, permission, protocol or authority boundary change.** The one-instance slot is only a diagnostic. Legacy same-user code can modify JavaScript globals; a singleton key does not authenticate Host origin or grant OS authority.

### 4.2 One package entry, no dual loader

The minimal exports sketch consumes the existing package-local source file, subject to actual Node 26 and Bun tests:

```json
{
  "exports": {
    ".": "./src/index.ts",
    "./main": "./src/index.ts",
    "./renderer": "./src/index.ts",
    "./common": "./src/index.ts",
    "./utility": "./src/index.ts"
  }
}
```

Every target must have the **identical string**. Different import/require condition paths are forbidden. If Node 26 cannot load the *packed* transitive TypeScript graph with no top-level await, the approved implementation may produce **one** deterministic package-local `dist/index.js` and map **all five** keys to it, documenting that runtime failure and the one pack/build command. Never paper over a failed ESM/CJS case with a second evaluator, plugin or runtime alias.

Use the existing `@keld/api` application lifecycle owner and canonical transport. Change outgoing `@keld/api` imports to declared **package dependencies**, not repository-relative paths; do not copy API or KIPC internals. Today `@keld/api` itself has cross-package relative sources, so its isolated package closure is a concrete external owner gate, independent of fixing GH-502's own imports. Before the implementing claim, record the existing tracker and winning owner that will supply each packable API/KIPC artifact, its pinned package/lock identity and producer command. If no tracker owns that prerequisite, the repository owner must assign or create the bounded packaging task before R05 can pass; GH-502 does not silently acquire API/KIPC writes. Preserve the canonical stamped transport as its existing readable Worker entry; an SDK single-entry build must not inline, copy or replace that transport to conceal a missing package edge.

The inspected implemented Electron member set contains `app` and the `BrowserWindow` subset; this resolver task adds no facade members. Non-Electron helper values such as `isCallError` or shim-version constants must live in a separately designated internal API if truly needed, not be inserted into Electron's default object merely to pass key parity. This is resolver work only; missing Electron member implementations remain under their own facade tickets. Before R03 can pass, GH-502 must remove or relocate non-Electron helper value exports from the public Electron entry, reconcile affected callers under their owners, and keep the namespace-key assertion unfiltered except for `default`.

The duplicate-instance diagnostic compares **real observed canonical module URLs** per realm/role. A repeated import/require of the same cached module must not throw. A second evaluated URL must throw an `Error` with a stable unused KELD-COMPAT code, the two locations and a fix sentence. `KELD-COMPAT-001` through `-009` are already assigned to evidence errors; reserve the chosen new code with the [central error registry](../engineering/keld-error-codes.md) owner in the implementing integration. This draft does not claim a registered error code. Cross-process/worker realms are independently scoped; a global-slot guard is not security provenance.

X02-T3, not GH-502, installs the `"electron": "npm:@keld/electron@<approved-pinned-version>"` dependency into a migrated app and removes app-level paths. GH-502 may change the resolver in its **KEL-72 test fixtures**. Always test a real installed package lock and directory, not a monorepo alias or fabricated npm package record.

### 4.3 Template interfaces

- **Capabilities/manifest (security spec 03):** none. The external application `package.json` alias is an X02-T3 build configuration, not a KELD permission grant.
- **Wire/protocol (spec 02):** none. No new IPC channel, principal or transport.
- **Platform:** real macOS arm64 Bun 1.4.4 and Node 26 for current source; historical Bun 1.4.2 separate. Windows, Linux, webpack, tsc and `bun build --compile` remain UNKNOWN until separately exercised.
- **Runtime seam:** initial package resolution/import before app use, isolated installed package graph, one role module cache, stable typed failure for a duplicate actual evaluation. No OS handles or new process owner.
- **Migration unit:** current `@keld/electron` entry and KEL-72 owned fixtures. Keep shipped `app` lifecycle observable. X02-T3 app manifest, updater and #497's architecture/migration output are separately owned. No temporary dual resolver or implicit fallback.

## 5. Boundaries

**This spec-only PR edits:** `docs/specs/gh502-electron-module-resolver.md`, and no other tracked file. Do not touch root shared files, architecture docs, #446/#449/#497 branches, production runtime, CI, source pins or scorer.

**Later separate implementing PR, only after human exact-source approval:** `packages/@keld/electron/package.json` and its entry/owned tests/fixtures. An actual need to modify `@keld/api`, `@keld/kipc`, central error registry, root generated documents or architecture 04 must be routed to the **respective owner**; this spec grants no cross-owner write.

## 6. Tasks (one reviewable responsibility per stage)

- [ ] T0 — Publish this exact **specification only** as OPEN/DRAFT; get human `Status: approved`. Align GH-412's architecture correction through #497's owner, not by an unreviewed local edit.
- [ ] T1 — GH-569 owner lands X03-T3 conformance cells and pinned Electron v44.4.5 reference evidence, with explicit fail/unknown expected statuses. This is a prerequisite to GH-502 code.
- [ ] T2 — After T0/T1, GH-502 owner implements one installed package entry + exports and actual CJS/ESM identity tests. The separate API/KIPC owner supplies any missing transitive package closure before claiming R05.
- [ ] T3 — Migrate KEL-72 owned fixtures from `tsconfig.paths` and inert `bunfig [alias]` to the installed alias. Exercise all listed negative controls and maintain current lifecycle behavior.
- [ ] T4 — X02-T3/#497/#499 own app-side alias and migrate output. GH-589 later owns real macOS product acceptance and truthful scoreboard publication.

## 7. Test plan

R01–R04: create a **disposable installed** package fixture outside the source checkout with immutable alias/lock. Run five specifiers × four import forms × two call sites on each real selected Bun/Node binary; compare `app` reference identity across every result, not just truthiness of import. Enumerate `Object.keys(default)` versus actual named namespace keys, excluding `default`. Mutations: remove default, redirect one subpath, split import/require, insert top-level await.

R05: pack/install the exact files plus declared dependencies without sibling repo source packages, then assert every real resolved path is inside its declared installed package; a relative `../../api` escape must turn RED. Retain the exact packed artifact, producer/consumer commands, lock/cache inputs and per-file byte identity before evaluating its entry. The 2026-10-28 GH-589 research fixtures using an unchanged SDK directory `file:` dependency, both default and `--install-links=true`, stopped offline on uncached `@types/bun` metadata before any SDK import; these are tool-input failures, not passing packed-package closure or executed runtime failures. Do not remove SDK devDependencies, invent siblings or substitute a repository alias to turn those fixtures green. The implementing owner must qualify a real artifact and its required package-manager inputs independently.

R06: positive same-URL cache reload, negative second evaluated URL in same realm, verify code and both canonical locations; separate process/Worker starts clean. R07: use the actual foreign package as installed data only; run the owning non-evaluating alias/graph admission and assert refusal before launch/import. Missing alias, root substitution and dependency-local shadowing are separate cases. Do not import real Electron to obtain the failure. R08: restore competing alias configuration and assert the owner's configuration rejection; the separate R06 control explicitly loads two genuine Keld entries and checks duplicate diagnostics. Restoring inert `bunfig [alias]` fails its separate static configuration-presence test. Before R07 or installed third-party execution, X02-T3/#497/#499 must identify the winning owner and exact artifact for non-evaluating installed alias/graph admission; X03-T6/[GH-593](https://github.com/gyldlab/keld/issues/593) must supply the independent no-runtime-auto-install gate through the launcher owner; #593 remains a prerequisite, not evidence that the gate is implemented. GH-502 consumes these prerequisites and adds no resolver hook. Missing ownership, approved contract or evidence blocks those predicates. No-auto-install alone does not provide pre-evaluation admission and does not block a package-owned installer. If a foreign package is selected without admission, its main may conditionally spawn `process.execPath` ([Node documentation](https://nodejs.org/api/process.html#process-execpath)) with its installer script when no executable path is present and `ELECTRON_OVERRIDE_DIST_PATH` is unset; `process.execPath` is the current runtime executable, not universally Node. This source path was inspected, not executed. R09: real import-time entry of each lock-pinned third-party consumer, not a stub, only after admission passes. R10: retain actual binary versions, runtime and pack SHA and OS/arch. R11: named GH-569 live test selection and expected-status mutation. R12: rerun existing KEL-72 lifecycle/transport fixtures.

Use isolated folders, fixed lockfiles, exact selectors, no sleeps as readiness proof and no OS installation claims from package import. Native Host/macOS GUI and full Rust workspace builds are **not** tests of this docs-only draft.

## 8. Review gates triggered

Spec-only: none of unsafe, production public API, permission, dependency additions or wire changes. **Implementing PR:** public API and dependency-package graph review, with central error-code owner for its new typed diagnostic. Independently review the final source and actual OS/runtime evidence before any compatibility claim.

## 9. Perf impact

No runtime change from this spec. The future guard adds cold entry-evaluation work; measure import startup and module count in both Bun and Node at the selected exact versions before making performance assertions. No new hot KIPC work is proposed.

## 10. Open questions / implementation dependencies

The **design choice** (single dependency alias, one module entry, five exports, no dual resolver) is settled by GH-412. **Blocking work** remains: human approval of this draft, GH-569's separately owned conformance implementation, packable `@keld/api` → `@keld/kipc` dependency graph, and #497's architecture/migration reconciliation. If Node26/Bun requires a generated single-file target, first capture the actual installed-case failure and then apply `4.2's one-target alternative; do not invent a second resolver or declare the existing TypeScript package Node-compatible without executing it.
