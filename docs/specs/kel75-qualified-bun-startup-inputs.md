# Spec: qualified Bun startup selection for the macOS development primary
Status: draft
Linear: KEL-75 · Owner: GYLDLAB / @0monish · Updated: 2026-10-31

This is a separate proposed amendment to the approved KEL-75 contract. It does
not amend that approved file by implication. Production implementation requires
exact-text approval of this text and the failing-first tests in §7. No new
launcher, module resolver, import walker, source copier, configuration parser,
supervisor or approval registry is authorized. GH1037's verification and draft
publication remain independent.

Decision record: four owner decisions (D1–D4) were recorded on 2026-10-31 by an
AI agent under the user's standing architecture delegation, in
[GH499 comment 6546582815](https://github.com/gyldlab/keld/issues/499#issuecomment-6546582815),
and mirrored on KEL-75. Their evidence packet is
`.git/keld-closeout/drawio-mvp-continuation-20261031-claude/kel75-decision-packets.md`
in the main checkout, SHA-256
`68bce79e59a81c727e38a8fcd42dc16d78156ba43e09e4b445421e4e3e00d68a` (cited below
as "packet"; run ids E1–E9 are its Appendix A). A recorded decision is not
exact-text approval of this amendment. §10 lists what each decision settles and
what stays UNKNOWN. This revision carries one boundary change: the development
stage moves out of the project tree (§4, "Stage location").

## 1. Goal & non-goals

Before the existing macOS Guardian starts a Bun primary from a general source
stage, bind its effective startup selection to qualified inputs. Either the
existing command factory constructs the selected invocation before any candidate
module can evaluate, or it returns a typed refusal. Every successor uses the
same selected runtime, stage, configuration, preload graph and ordinary
environment, with only the existing fresh generation bootstrap changing. The
stage that the selection reads lives outside the project tree, so no live
project file can enter that selection through ancestor resolution.

Non-goals: implementing GH499's installed-package-set admission (GH499 owns it;
this amendment consumes its result); implementing GH502's package; an import
walker, `bun build --metafile` oracle, preload plugin or second resolver;
general module/config discovery in KEL-75; stage copy rules beyond capturing the
project-root `package.json` (the stage *location* does change, §4); adding a
public configuration key, flag, file, wire message, principal or permission; an
allow-list environment carrier (GH452); source re-staging on restart; installer
or signed-runtime acquisition (KEL-141); renderer/preload isolation; other-OS
support; changing the approved role/lifecycle/Guard contracts; Strict or
product proof; replacing GH593.

## 2. Spec refs

- Architecture01 ownership; architecture03 §§1–3 identity, authentication and
  default denial; architecture06 §1 versioned runtime/supervision and §2 tooling.
- Approved [KEL-75](kel75-principalized-bun-child-roles.md): the one
  `Supervisor`, `ChildPreparer`, generation lease, authentication and revocation.
- Approved [KEL-96](kel96-no-flag-host-boot.md): executable-located boot,
  retained stage validation, closed descriptor and Guardian process boundary.
- Approved [KEL-140 §§4.5–4.6](kel140-macos-public-filesystem.md): the one fixed
  policy capture and prepared-entry/canonical-transport baseline. Its prepared
  bundle is not general raw package/source admission.
- GH1037 draft at `159a724d6e5e3f1e6ae544ae906ccc4da4988b4b`,
  `docs/specs/gh1037-multifile-dev-staging.md` AC7/8 and §10: source capture
  cannot establish complete effective startup inputs; AC8 preserves a qualified
  installed graph's relative layout when its owner supplies one.
- Approved GH499 contract at `5e6a8269056668b8408bad83ce925ccae217cb71`,
  A01/A08/A10/A15; approved GH502 contract at
  `fef3b9db2475aaf9449f2f298cc67dc2c82909eb`, R05/R07/R08 and §7.
  [GH499 coordination](https://github.com/gyldlab/keld/issues/499#issuecomment-6543672236)
  names the producer assignment; it does not supply a completed predicate.
- Approved GH497 contract (branch `agent/gh-497-migrate-output-contract-20261024`,
  `docs/specs/gh497-first-proof-migration-output.md`, not on main) M15/M18
  consume the same graph-admission and no-auto-install links (packet N7).
- Approved [KEL-139](kel139-macos-product-spine.md) "Distribution/runtime
  selection": KEL-141 owns Bun acquisition, verification, install location and
  launch, and removes inherited `PATH` as the Bun-selection authority.
- GH452 owns any new declared environment carrier/reserved-name floor, together
  with KEL-96 schema and KEL-75 spawn approval. GH593 (open; `milestone:next`,
  `needs-spec`) owns "every Bun role" missing-alias runtime-install hardening;
  D4 is its first partial delivery only. GH467/GH484 own application
  browser-preload behavior; KEL-78 owns OS containment. These are distinct
  owners.
- **Deviation from approved text.** Approved KEL-96 fixes the non-release stage
  at `<project>/.keld/dev/<launch-nonce>/` (`kel96-no-flag-host-boot.md` lines
  103 and 285), and architecture 06 §1 (line 45) describes host removal of the
  validated `.keld/dev/<nonce>` root. The stage-location decision deviates from
  both. The PR that lands this text as approved must carry the matching KEL-96
  and architecture 06 amendments (KEL-96/CLI owner). This draft concern edits
  only this file, so those amendments are a named predecessor of approval.

### Current facts and limits

At main `0f5128462d1329ad1945e6639c1956d38242dd4f`,
`keld-host/src/main.rs::guardian_bun_command` validates stage/entry identity,
refuses staged-root `bunfig.toml`/`.bunfig.toml`, selects the optional canonical
`src/kipc-transport.ts` CLI preload and removes inherited `BUN_OPTIONS`.
`GuardedPrimaryPreparer::prepare` calls that factory for each generation.
Existing command/sentinel tests cover those specific controls. This does not
prove complete source/package/configuration/environment qualification.

Facts added in this revision. Code anchors were re-checked at `0f512846`.
[F-code] is cited code, [F-run] is one local run on macOS arm64 with the Bun
below (not a Keld-harness or CI result), [I] is inference.

- **Stage-ancestor resolution.** This contradicts the first draft's non-goal
  "no change to the stage layout" and its AC7 "never re-reads source".
  [F-code] `stage_dev_boot` places every stage under
  `project_root.join(".keld/dev")` (`crates/keld-cli/src/boot.rs:428`). The Host
  cleanup validator `dev_stage_cleanup_root_for` (`crates/keld-host/src/main.rs:345–371`)
  and the Guardian's `guardian_stage_cleanup_root` (`:540–560`) require that
  `.keld/dev/<32 hex>` shape. [F-run] With a staged entry at
  `<project>/.keld/dev/<32-hex nonce>/src/`, cwd = stage and
  `--no-env-file --no-install`, Bun evaluated a package present only in the live
  project's `node_modules`, a `tsconfig.json` `paths` shim and a `package.json`
  `imports` target (E3a–E3c). A `node_modules` inside the stage did not stop the
  fall-through (E8a); a stage-root `tsconfig.json` or `package.json` took
  precedence (E8b/E8c). Control: the same entry copied outside the project tree
  fails with `Cannot find package`. That was reproduced twice by the recording
  agent (GH499 comment 6546582815) and again for this revision on 2026-10-31,
  with an isolated `HOME` and install cache after checking that no scratch
  ancestor held `node_modules`, `package.json`, `tsconfig.json`,
  `jsconfig.json`, `bunfig.toml` or `.env`: under the project the run printed
  `PROJECT_NODE_MODULES_FOO_EVALUATED` and `ENTRY_RAN` (exit 0); outside it
  printed `error: Cannot find package 'foo'` (exit 1).
- [F-code] `keld create` writes a project-root `package.json` with
  `"type": "module"` (`crates/keld-cli/src/template.rs:67`,
  `templates/hello/package.json`). No stage captures it, so every current
  healthy stage reads that live file (packet N2). The hello template has no
  `tsconfig.json`, and its `src` imports only relative files and builtins.
- [F-code] No artifact or code admits a package, lock, installed tree or graph
  today. `stage_dev_boot` (`boot.rs:325–598`) writes only the host copy, the
  entry, the renderer, an optional `src/kipc-transport.ts`,
  `keld.permissions.jsonc` and `keld.boot.json`.
- [F-code] Host and Guardian are one binary: the Host spawns `current_exe`
  (`crates/keld-core/src/app_session.rs:2392–2414`) with
  `--keld-internal-macos-guardian-v1` (`keld-runtime/src/macos_guardian.rs:138`),
  root, entry, entry `dev`, entry `ino` and boot mode; the Guardian requires
  exactly 7 arguments (`main.rs:443`). Nothing carries a graph identity. The
  descriptor schema is closed and already binds
  `permissions: {file, content_sha256}`
  (`crates/keld-core/src/app_session/boot_contract.rs:41–50,140–149`); schemas
  1 and 2 are accepted (`:89–125`).
- [F-code] The factory runs `Command::new("bun")` (`main.rs:490`), a `PATH`
  lookup in the Guardian's inherited environment, and architecture 06 records
  Bun pinning/download as not built. AC2's pinned absolute executable is
  therefore unmet today; KEL-141 owns it.
- [F-run] `BUN_INSPECT_PRELOAD` runs after the canonical `--preload` and before
  the entry, also under `--no-env-file --no-install`; an empty value aborts
  startup (E1, E7c). `NODE_PATH` adds a resolution root under `--no-install`
  (E7a). A stage `bunfig.toml` with `[install] auto = "force"` overrides
  `--no-install` (E6c). The full observed-controls table is in §4.

Local metadata-only observation identifies Bun `1.4.4+38afee6c8`, binary SHA256
`0a2237cdcfa0d16ef501d9acca9f0026309ef348bea7393a64e8d1e3609565cc`.
Official `bun-v1.4.4` resolves to source
`38afee6c84fcf48c0edb2466aa4b27d287a59c30`. Version/help and source inspection
are not a real invocation/negative-control pass. The one-run observations above
are real invocations of this `PATH` binary, not Keld-harness controls. No
runtime upgrade is proposed.

## 3. Acceptance criteria (binary, each becomes a test)

1. **One external graph input (D1).** Given the produced stage and GH499's
   data-level installed-package-set admission over the captured lock and
   installed `package.json` identities, the KEL-75 selection retains that
   binding. Admission passes only when the root `electron` resolves to the
   approved `@keld/electron` artifact (name, version, integrity) and no other
   installed package is named `electron`. A missing alias, a root substitution,
   a dependency-local `node_modules/<dep>/node_modules/electron`, and a missing,
   foreign, incomplete or wrong-stage admission each refuse before any
   application, dependency or candidate preload evaluates. The package-local
   singleton, an alias string or `--no-install` alone cannot satisfy this
   criterion.
2. **Runtime and entry.** The initial selection identifies one absolute Bun
   executable and source/version-qualified behavior, the validated private
   stage and the existing admitted entry identity. A different executable,
   stage or entry refuses before candidate evaluation. Development identity
   checks do not claim signed-release or malicious-same-user authenticity. The
   absolute pinned executable comes from KEL-141. While the factory launches
   `bun` by name (`main.rs:490`), this criterion is reported **unmet**; this
   slice does not substitute its own `PATH` resolution.
3. **Effective input completeness (scoped).** Every input that the §4
   observed-controls table records as honored is, on the qualified path,
   refused, suppressed by a qualified pinned-runtime flag, excluded by the stage
   location or admitted by its existing owner. A census test over the
   `BUN_*`/`NODE_*` names present in the pinned binary fails when a name is
   neither classified in that table nor recorded UNKNOWN. Do not silently copy,
   ignore, synthesize or override active configuration to obtain a pass. Two
   recorded exceptions are explicit rather than silent: the existing
   `BUN_OPTIONS` removal (kept unchanged in this slice) and the stage
   producer's capture of the project-root `package.json`. UNKNOWN names are not
   qualified, and this slice is never reported as complete effective-input
   qualification.
4. **Preload order and identity.** The selected canonical KIPC preload, if
   present, retains its existing artifact/namespace contract. No extra config,
   CLI or inherited preload can execute first or replace/extend it. In
   particular, `BUN_INSPECT_PRELOAD` present in the Guardian environment with
   any value, including empty, refuses before child creation. A healthy
   selected preload and normal entry remain observable in that order. Absence
   continues to use the existing no-sidecar path where that prepared-entry
   contract allows it.
5. **Ordinary environment (D3).** The Guardian freezes one environment map
   once, before the first Bun invocation. Each prepared command, for G1 and
   every restart, carries exactly that map plus the existing per-generation
   `KELD_APP_LINK`; no value is re-read from the process environment after the
   freeze. `BUN_INSPECT_PRELOAD` or `NODE_PATH` present refuses with the §4
   typed error. The existing `BUN_OPTIONS` removal is kept. No parent/global
   environment mutation, new bootstrap variable or app-selected authority is
   permitted. Secret values are not printed or stored in receipts.
6. **No runtime acquisition in this qualified path (D4).** Precondition: the
   existing stage `bunfig.toml`/`.bunfig.toml` refusal (`main.rs:480–489`),
   because a stage `[install] auto = "force"` overrides `--no-install` (E6c).
   Every child constructed from this selection passes `--no-install` and
   supplies no package-manager/script-dispatch fallback. With no `node_modules`
   in the stage or any ancestor, a missing installed target cannot evaluate a
   warmed-cache or foreign package. An installed admitted package still works.
   This is independent of installed graph identity (AC1). It is credited only
   as GH593's first, macOS-primary-only partial delivery under the three §4
   conditions and is never reported as satisfying #593, GH593 AC2/AC3, GH497
   M18 or GH499 A15's network/updater/observer clause.
7. **Frozen recovery.** After G1 retires, the existing preparer constructs G2
   from the same selected inputs or refuses before execution. Because the stage
   sits outside the project tree (AC10), G2 resolves only captured stage bytes
   and never re-reads live project source, configuration or environment into a
   different selection. Before each generation the factory rechecks the entry
   identity (existing) and the files bound by the graph digest (AC11); a
   changed bound file refuses before the candidate marker. Source changes
   outside the captured stage do not silently re-stage and do not reach G2.
   Fresh token/principal and old-generation revocation remain independently
   required.
8. **Failure and compatibility.** Any refusal preserves the owning typed error,
   input class and remedy, creates no candidate module marker and uses existing
   lease/reap/stage cleanup. No deterministic retry, alternate executable,
   config deletion, source flattening or grant broadening follows. A healthy
   prepared-entry/canonical-transport project retains existing behavior,
   including after the stage relocation, with its root `package.json` captured.
9. **Scope honesty.** The result changes neither Guard decisions nor OS
   containment, renderer origins or product receipts. Other roles/platforms and
   the wider GH593 matrix are not reported qualified by this macOS-primary slice.
10. **Stage outside the project tree (boundary change, D1).** Given a project
    whose live `node_modules`, `tsconfig.json` `paths` or `package.json`
    `imports` supplies a module the captured stage does not contain, when
    `keld dev` stages and the Guardian starts G1, then resolving that module
    fails before any app or foreign sentinel prints, while the healthy captured
    stage, including its captured root `package.json`, starts. No ancestor of
    the stage root is the project root or lies inside it.
11. **Graph digest carrier (D2).** Given a Host that validated a schema-3
    `keld.boot.json` with `graph: {file, content_sha256}`: when the Guardian is
    launched with a well-formed but different digest, or a bound graph file
    changes after Host validation, the Guardian refuses with the typed error
    naming the graph-digest input class before the first Bun command is
    constructed; when a bound file changes between G1 and G2, G2's preparation
    refuses through the existing failed-preparation lease path. The unchanged
    control starts and restarts normally.

## 4. Design

### First principles and reuse

**One boundary change: stage location.** CLI remains the stage producer;
Host/Guardian retain their current process/crash ownership; the existing
generation owner mints identity; Guard authorizes privileged operations. Handle,
crash and principal ownership do not change, so under root AGENTS this is not an
ownership change. It does change the I/O and trust boundary of what the selected
Bun child can resolve, and it changes an approved KEL-96 layout, so it is
written here as a boundary change with its own gates (§8). Every other change
alters the inputs accepted by the existing spawn preparation, not handle or
authority ownership. No renderer receives a new handle, endpoint or selection
capability.

| Atom / owner | Input → output and failure | Independent first falsifier |
|---|---|---|
| Graph identity / GH499 (data-level package set), GH502 consumer | Captured lock + installed `package.json` identities → admit or refuse; wrong package/stage refuses | Dependency-local foreign Electron marker executes before refusal |
| Stage location / CLI `stage_dev_boot` (producer), KEL-75 (requirement) | Project → nonce stage with no project ancestor; a live project file reachable by ancestor resolution is a failure | Live-project-only package evaluates from a staged entry |
| Carrier / KEL-96 descriptor + Guardian argv | Host-opened graph bytes → digest → Guardian one-shot check + per-generation recheck | Wrong digest or changed bound file reaches the first Bun command or G2 |
| Environment / KEL-75 Guardian | Inherited environment → one frozen map; named loader controls refuse | Inherited preload/path variable reaches a child |
| Selection / KEL-75 preparer | Qualified graph + runtime/config/env facts → one immutable local selection | Unlisted effective input changes evaluated code |
| Capture / CLI, KEL-140 | Existing retained captured bytes → same stage identity | Policy captured twice or live source replacement selected |
| Authentication / existing bootstrap | Fresh generation secret → authenticated link bound to that generation | A selection digest or env value treated as a principal |
| Authorization / Guard | Unchanged verified policy → same allow/deny | Qualification changes a grant or bypasses dispatch |
| Containment / KEL-78 | Existing profile → existing OS claim | Startup pass reported as Strict containment |
| Lifecycle / Supervisor and lease | Same selection + fresh generation → one child/revocation/reap path | G2 rereads ambient input or old authority survives |
| Evidence / named input owners | Pinned source plus actual controls → scoped acceptance | Help/source presence or a one-run local observation reported as a harness pass |

The exact runtime/graph/stage identity is an explicit edge between atoms. A
second explicit edge: GH499's package-set check (AC1) is complete only when the
routes around `node_modules` are closed, which KEL-75 owns (`NODE_PATH` and
inherited preload through AC4/AC5; live `tsconfig.json` `paths`, `package.json`
`imports` and ancestor `node_modules` through the stage location, AC10). A third:
the healthy installed-alias positive (AC1, AC6) depends on the producer having
captured the installed tree into the relocated stage (GH1037 AC8 with GH499's
admission); without that capture the relocation fails closed. None of these
passes substitutes for another. Memory is one cold, immutable selection retained
by the current preparation owner; generation-specific bootstrap values remain
lease-owned. Source/graph capture I/O stays with its existing owner; preparation
performs only the owner's defined identity rechecks. No new hot-loop
allocation, queue or background watcher is proposed.

Existing primitives considered:

- Retain the existing `guardian_bun_command` factory and
  `GuardedPrimaryPreparer` callback/lease path. Its root-config refusal and
  `BUN_OPTIONS` removal are real partial controls, not a reason to bypass
  effective-input qualification.
- Retain the existing source/policy capture and upstream Bun resolver.
  GH499 supplies a data-level installed-package-set check (D1); this owner
  validates its applicability. A KEL-75 import walker/config parser would
  duplicate owned semantics and is rejected.
- Rejected as the graph oracle: `bun build --metafile`. It executed an app
  macro (E9, `MACRO_EVALUATED_AT_BUILD`), covers static imports only, and its
  bundler resolution is not shown equal to `bun run`.
- Retain the actual pinned Bun command-line primitives where source/docs and
  runs support them; qualify their resulting behavior in the Keld harness
  before claiming an enforcement pass.
- A package singleton, generic file-copy success, metadata hash by itself,
  late import error or `--no-install` alone cannot prove pre-evaluation graph
  identity. A second resolver/plugin and a new approval registry are rejected.
- Copying all active startup inputs or silently discarding them is rejected.
  The first slice refuses unqualified inputs instead of changing app semantics.

### Stage location (boundary change, D1)

The stage MUST NOT sit under the project tree. The producer is the sole
dev-stage producer, `crates/keld-cli/src/boot.rs` `stage_dev_boot`
(`crates/keld-cli/AGENTS.md`: "`boot.rs` is the sole dev-stage producer"),
changed under the KEL-96 layout contract by its KEL-96/CLI owner. KEL-75 owns
the requirement and its acceptance (AC10). The same KEL-96 change updates the
Host cleanup and Guardian stage validators that require
`<project>/.keld/dev/<32 hex>` today (`main.rs:345–371`, `:540–560`), because
they decide which directory the Host deletes.

Proposed producer obligations:

- Create the owner-private `0o700` nonce stage in a location that has no
  project ancestor. This draft does not select the concrete directory; that
  is the KEL-96/CLI owner's choice, judged by AC10.
- Capture the project-root `package.json` byte-for-byte as a staged leaf so
  its module semantics (`"type": "module"`, packet N2) are preserved. A stage-root
  `package.json` takes precedence over any ancestor (E8c).
- Capture nothing else from the project beyond what the existing producer and
  GH1037's approved tree contract already capture.

Rejected: keeping the stage in the project and binding the project ancestors as
admitted live inputs rechecked every generation (packet option (b)), because
that relaxes AC7 and turns live files into selection inputs; a stage-local
`node_modules` as a barrier, because it does not stop the fall-through (E8a);
`--tsconfig-override`/`--config` workarounds (AC3).

Platforms: `stage_dev_boot` places the macOS, Linux and Windows stages under
the same `project_root.join(".keld/dev")` (`boot.rs:428`). This slice requires
the relocation for the macOS primary only. Whether the producer also moves the
Linux and Windows stages is UNKNOWN here and belongs to KEL-96/CLI; if they
differ, KEL-96 records a named per-platform layout rather than an implicit
divergence.

UNKNOWN:

- A captured root `package.json` keeps any `imports` map, and a stage-root
  `tsconfig.json`/`jsconfig.json` captured under GH1037 would keep `paths`.
  Both are resolution routes inside the stage that GH499's package-set check
  does not see; `tsconfig.json` `paths` can map a bare `electron` (E3b). This
  draft selects no parser. Admitting or refusing these routes is an open
  GH499/KEL-75 item (§10); until it is settled, AC1/AC3 are not claimed for a
  project that uses them. The hello template uses neither.
- Whether an ancestor of the chosen location (for example the user's home
  directory) holds `node_modules`, `package.json` or `tsconfig.json`. The chosen
  location must have none; AC10 is the observable.

### Proposed internal input/output interface

The **logical** input is one KEL-75-owned `QualifiedBunStartup` value, constructed
only after the following owner results have passed. It is private process-local
state, not a new user file, manifest, environment token or public Rust API.

| Required field | Exact meaning and owner |
|---|---|
| Runtime identity | Absolute selected Bun executable, byte/OS identity, version/revision and qualified selector behavior. KEL-141 owns acquisition, verification, install location and launch. Today `bun` is launched by name (`main.rs:490`), so this field is unmet until KEL-141 supplies the executable; this development slice does not require installed-release completion and does not claim AC2. |
| Stage and entry | Existing validated executable-located root (relocated outside the project tree), relative entry, retained identity and boot mode; KEL-96/CLI remain owners. |
| External graph binding | GH499's data-level installed-package-set admission result (D1), produced at staging by the existing producer calling GH499's check, written as a staged leaf and bound by `graph.content_sha256` in the descriptor (D2). GH502 R07 consumes it. The leaf's exact name and schema are GH499's (UNKNOWN). A bare hash/author assertion is invalid. |
| Effective selection | The §4 observed-controls classification, the stage-location exclusion and explicit absence facts, with their capture epoch. No guessed discovery algorithm is part of KEL-75. |
| Canonical preload | Absent under the current compatible prepared-entry contract, or the exact existing transport artifact and resolved transitive identity in the admitted graph. A caller-supplied substitute is invalid. |
| Ordinary environment | One frozen key/value map captured by the Guardian before the first launch (D3); values retained in memory only. App-declared keys require GH452's separately approved implemented carrier/floor. |
| Fixed argv intent | Existing host-selected boot mode, canonical preload when present, `--no-env-file`, `--no-install` and the absolute entry; no app-provided Bun flags, package script, inspector, watch or package-manager invocation. |

The existing factory's proposed internal seam is:

```rust
fn guardian_bun_command(
    selected: &QualifiedBunStartup,
    app_link: &str,
) -> Result<std::process::Command, keld_runtime::RuntimeError>;
```

This sketch replaces the existing private argument list only in the later owned
implementation; it is not a stub to land or a claim the type exists. The current
preparer retains its existing `FnMut(&str) -> Result<Command, RuntimeError>` seam:
the closure owns the same immutable selection and receives only that attempt's
fresh app-link. No new supervisor/trait or bootstrap variable follows.

### Host→Guardian graph carrier (D2)

Host and Guardian are different processes running the same binary; a Rust value
in the Host is not available in the Guardian. The carrier reuses one existing
pattern per hop.

1. **CLI → Host.** A new boot-descriptor schema version (3; schemas 1 and 2
   exist) adds a closed `graph: {file, content_sha256}` object that mirrors
   `permissions` (`boot_contract.rs:41–50,140–149`): a fixed literal file name,
   a `sha256:` digest of 64 lowercase hexadecimal digits, and
   `deny_unknown_fields` retained. Producer: `stage_dev_boot`. Parser: the
   existing `parse_boot_bytes`. The Host opens the leaf with the existing
   root-relative no-follow open (`open_relative_file`, `app_session.rs:1801`),
   hashes the bytes it opened and compares them, as `load_verified_manifest`
   does for permissions; it never reopens by path after its check (KEL-96
   line 352).
2. **Host → Guardian.** One added argument, the 64-character lowercase hex
   digest, under a bumped marker `--keld-internal-macos-guardian-v2` with
   exactly 8 arguments. A v1 marker or any other count refuses as malformed
   through the existing `parse_guardian_boot_mode` path. The digest is
   integrity data, not a principal, token or grant.
3. **Guardian check, once.** After `connect_guardian_registration`
   authenticates the KGR1 registration and before `Supervisor::start_prepared`
   (`keld-runtime/src/macos_guardian/supervision_runner.rs:340`), the Guardian
   reopens the leaf under its validated root, hashes it, compares the result
   with the argv digest, re-verifies the files the leaf binds, and only then
   builds the one immutable `QualifiedBunStartup`, including the frozen
   environment (D3).
4. **Recheck on every restart.** `guardian_bun_command` re-hashes the bound
   files next to the existing `reopen_validated_entry` check (`main.rs:468`).
   A mismatch refuses with the typed error through the existing
   failed-preparation lease path (`keld-runtime/src/macos_guardian/guarded_lease.rs:77–85`).

Each consumer hashes the bytes it reads itself. Bun still reads the installed
files after the check; that window is the existing cooperating-developer trust
level, not a same-user attacker guarantee.

Proposed seam for step 3 (a sketch of the existing `keld-runtime` function, not
a claim that the code exists):

```rust
pub fn run_guarded_primary<R, S, C, A>(
    liveness: R,
    // Runs once: after KGR1 authentication, before Supervisor::start_prepared.
    // Returns the per-generation factory that owns the immutable selection.
    select: S,
    quit_ack: A,
) -> Result<CapturedOutput, RuntimeError>
where
    R: Read + AsFd,
    S: FnOnce() -> Result<C, RuntimeError>,
    C: FnMut(&str) -> Result<Command, RuntimeError> + Send + 'static,
    A: Write;
```

The preparer keeps its `FnMut(&str)` seam; the selection moves into the returned
factory. A selection failure returns before any generation exists, so no lease
or endpoint needs revocation. Hashing reuses `keld-core`'s existing digest and
relative-open primitives: [F-code] `keld-host` depends only on `keld-core` and
`keld-runtime`, and `keld-runtime` has `sha2` only as a dev-dependency. A new
direct crate edge would be a dependency gate and is not proposed.

Rejected:

- **Per-restart KGC1 `PREPARED` payload.** It is minted per generation, mixes
  session-fixed values into the generation bootstrap, shares the 384-byte limit
  with the app-link and changes the reviewed private T3 wire.
- **Guardian re-parses `keld.boot.json`.** It duplicates KEL-96's sole
  validation owner and reopens a path the Host already validated.
- **Environment variable.** The Bun child inherits the Guardian environment, so
  the value would leak to the app; architecture 06 defines the bootstrap as
  `KELD_APP_LINK` only.
- **Path-only sidecar named in argv.** It reopens a path without a digest.

Output is either the one prepared child command plus the existing generation
lease, or an error before candidate evaluation. A successful graph/selection
check may precede fresh endpoint provisioning; if provisioning already occurred,
the existing failed-preparation lease path must revoke it before returning.
No app, dependency, preload, macro or package script is imported to perform
admission. A no-entry marker alone is insufficient if an earlier preload ran.

### Exact proposed qualified-path invocation

Use the selected absolute executable (KEL-141; today `bun` by name, so AC2 is
unmet) and absolute entry, existing stage cwd, fixed native-facade define and
optional canonical `--preload` selection. For this path, add the pinned
runtime's `--no-env-file` and `--no-install` before `run`, matching the existing
argv order; pass no `--env-file`, app-controlled options, install mode or
package-script name. Pinned source/docs support these mechanisms and one-run
local observations agree (E2, E5); Keld-harness controls remain required (§7).
Automatic environment files are refused as a source of startup values; required
values must already be in the frozen ordinary map. The draft adds no generated
Bun configuration file to disable them.

Preserve the existing staged-root Bun-config refusal. It is a precondition of
AC6, because a stage `[install] auto = "force"` overrides `--no-install` (E6c).
Any additional effective runtime/path configuration must be explicitly admitted
by its existing graph/startup owner or startup refuses. No
`--config`/`--tsconfig-override` workaround is selected. Parent-folder,
`~/.bunfig.toml` and `$XDG_CONFIG_HOME/.bunfig.toml` files were ignored in one
run each (E4a–E4c, E6a); that is recorded, not generalized into a claim that no
global configuration matters.

### Environment and observed controls (D3)

The Guardian freezes one environment map before the first launch: its inherited
environment at selection time, with the existing `BUN_OPTIONS` removal applied
(kept unchanged in this slice). If `BUN_INSPECT_PRELOAD` or `NODE_PATH` is
present, with any value including empty, selection refuses with the typed error
below. Each prepared command applies exactly that map with ordinary
`Command::env_clear` followed by the map, plus the per-generation
`KELD_APP_LINK`; the lease's existing registration-variable removal stays.
Nothing mutates the process-global environment.

This is a deny-list for this development slice. It is not exhaustive, and the
allow-list migration stays GH452's. A census test (§7) extracts the
`BUN_*`/`NODE_*` names present in the pinned binary (the packet counted 181
strings; presence shows a name exists, not what it does) and fails when one is
neither classified below nor recorded UNKNOWN.

Typed error, proposed: reuse `KELD-CORE-037`, the code every neighbouring
Guardian refusal already emits (stage config `main.rs:485`, transport checks
`:504–509`, stage identity `:544–556`). Its registry entry
(`docs/engineering/keld-error-codes.md`, `KELD-CORE-037`: "Fix the named dev
lease, guardian, app-link, Bun, window, or ordered-cleanup failure") already
covers a Bun-startup refusal, so no new registry entry is needed. Proposed
message shape: `KELD-CORE-037: inherited Bun loader control NODE_PATH can add a
module-resolution root outside the qualified stage. Unset it for the keld dev
launch and relaunch.`, with the `BUN_INSPECT_PRELOAD` form naming the preload
class. Collision check on 2026-10-31: `KELD-CORE-047` is the highest registered
core code (`keld-error-codes.md:985`), and `git log --all -S"KELD-CORE-048"`
finds no use on any ref. If exact-text review requires a distinct code,
`KELD-CORE-048` is the proposed next free value, and its registry entry lands
with the implementation.

Observed controls: Bun `1.4.4+38afee6c8`, macOS arm64, the `PATH` Bun, one run
each (packet Appendix A and the 2026-10-31 reproduction above). These are not
Keld-harness or CI results, and no real Electron package was involved.

| Input | Run | Observed result | Status in this slice |
|---|---|---|---|
| Canonical KIPC `--preload` | E1c | Runs first | Kept (existing) |
| `BUN_OPTIONS` | E4g | Its `--preload` ran | Kept: existing removal (`main.rs:520`; tests `:808–830`, `:832–875`) |
| Stage `bunfig.toml` / `.bunfig.toml` | E4d, E6c | Preload ran; `[install] auto = "force"` overrode `--no-install` | Kept: existing refusal (`main.rs:480–489`); precondition of AC6 |
| Parent-folder `bunfig.toml` | E4a | Not read | Excluded (one run); negative test |
| `~/.bunfig.toml`, `$XDG_CONFIG_HOME/.bunfig.toml` | E4b, E4c, E6 | Preload ignored; `auto = "disable"` ignored; global `auto = "force"` + `--no-install` refused | Excluded (one run); negative test |
| `BUN_INSPECT_PRELOAD` | E1b, E7c | Runs after the canonical preload, before the entry; empty value aborts startup | **Refuse when present** (new) |
| `NODE_PATH` | E7a | Honored under `--no-install` | **Refuse when present** (new) |
| `NODE_OPTIONS` `--require` / `--import` | E4e, E4f | Not honored | Excluded (one run); passes in the frozen map; negative test |
| `.env`, `.env.local`, `.env.development`, `.env.production` | E2 | Loaded from the cwd only; `NODE_ENV` selects the variant; `--no-env-file` disables all; an explicit `--env-file` still loads | Suppressed by `--no-env-file`; never pass `--env-file` |
| Runtime auto-install | E5, E6 | With no `node_modules` anywhere a tampered cached package was evaluated; `--no-install` refused with no manifest download; any `node_modules` in the cwd or a parent turns the default auto mode off | Suppressed by `--no-install` plus the stage `bunfig` refusal |
| `BUN_INSTALL_CACHE_DIR`, `BUN_CONFIG_REGISTRY`, `NPM_CONFIG_REGISTRY` | E5 | Select cache/registry; [I] no effect once `--no-install` is set | Suppressed by `--no-install` [I]; values pass in the frozen map |
| Project-ancestor `node_modules` | E3a, E8a, outside-tree control | Resolved from the live project; a stage `node_modules` did not stop it; outside the tree: `Cannot find package` | Excluded by stage location (AC10) |
| Project `tsconfig.json` `paths` | E3b, E8b | Shim evaluated; a stage-root `tsconfig.json` takes precedence | Excluded by stage location (AC10) |
| Project `package.json` `imports` / `type` | E3c, E8c | `imports` target evaluated; a stage-root copy takes precedence | Excluded by stage location; root `package.json` captured; captured `imports` UNKNOWN (above) |
| `jsconfig.json` | not run | — | UNKNOWN; treated like `tsconfig.json` until run |
| `tsconfig.json` `extends` outside the stage | not run | — | UNKNOWN |
| `BUN_INSPECT`, `BUN_INSPECT_CONNECT_TO`, `BUN_INSPECT_NOTIFY` | not run | Names present in the binary | UNKNOWN |
| `BUN_RUNTIME_TRANSPILER_CACHE_PATH`, `BUN_JSC_*` | not run | Names present in the binary | UNKNOWN |
| `KELD_NATIVE_WINDOW_FACADE`, `KELD_HOST_OWNS_FACADE_STAGE_CLEANUP` in the child | not run | [I] inherited by the Bun child | UNKNOWN; classification is KEL-96's |
| Executable selection (`PATH`) | code | `Command::new("bun")` (`main.rs:490`) | Gap: AC2 unmet until KEL-141 |

Explicit unknowns beyond the table: every name Bun 1.4.4 honors; whether the
observed exclusions hold on other machines, under CI or for the pinned
production runtime; Windows and Linux behavior.

### Recovery, errors and the auto-install edge

The selection is made once before the first Bun generation, after KGR1
authentication (D2). The factory reuses that value, rechecks the entry identity
and the graph-bound files, and refuses a changed still-observed input. It never
recaptures ambient env/config/package selection into G2. Generation-specific
credentials remain fresh and are revoked by the existing owner. This is the
cooperating-developer stage's existing trust level, not a new atomic-filesystem
or same-user attacker guarantee.

Preserve the nested runtime failure path at the existing Guardian boundary, with
the typed code proposed under D3. The message names the input class and
instructs restaging through `keld dev` with an owner-qualified
graph/configuration; it never suggests permission broadening, deleting user
configuration or trying another resolver. A non-owning stage/graph provider
keeps its own typed failure rather than being swallowed or converted into
success.

**No-runtime-acquisition scope (D4).** The AC6 property is accepted as GH593's
first, macOS-primary-only partial delivery. It is consistent with GH499 A15's
auto-install clause and GH502 §7 for consumers that launch only through this
path, and it is narrower than #593's "every Bun role" and GH497 M18's "every
owned generation", ordinary or declared-env launch. Three conditions apply:

1. The stage `bunfig` refusal is an explicit precondition (E6c).
2. #593 AC1's conformance entry lands before this slice is credited to #593
   (root AGENTS: "Compat lands a conformance entry first").
3. #593 AC2 (typed `KELD-COMPAT` diagnostic) and AC3 (declared role-environment
   key), GH497 M18's declared-environment launch, and GH499 A15's
   network/updater/observer clauses remain open with their owners.

It MUST NOT be recorded as replacing #593. It would contradict GH497 and GH502
§7 if it were accepted instead of #593, if a stage `bunfig` override got
through, or if any draw.io or R07 Bun process started outside this path. Waiting
for all of #593 is rejected (GH499 comment 6543672236), as is reading the
`milestone:next` label or the "installed alias never reaches auto-install" note
as a waiver (GH497 line 79). An installed foreign package remains GH499/R07
rejection, not auto-install hardening.

Capabilities/manifest: none. Public kipc, principal and grants: unchanged. The
private boot-descriptor schema-3 `graph` field and the v2 Guardian marker (D2)
are KEL-96 private carrier changes under its wire/schema gate (§8). Browser
preload/origin and OS containment are unchanged. Windows/Linux keep existing
behavior and are explicitly unqualified.

## 5. Boundaries

This draft changes only `docs/specs/kel75-qualified-bun-startup-inputs.md` and
owner linkage on KEL-75. Do not edit the approved original spec, GH499/GH502
contracts, GH1037's active fixture, architecture docs, generated corpus, Cargo,
CI fingerprints, environment files or production code in this draft concern.
The KEL-96 layout and architecture 06 §1 amendments that the stage relocation
requires land with the approval PR (§2), owned by KEL-96/CLI.

After approval, the implementation stays inside existing owners and files:

- `crates/keld-host/src/main.rs`: `guardian_bun_command` (flags, frozen map,
  loader-control refusal, per-generation graph recheck),
  `run_supervised_guardian`/`parse_guardian_boot_mode` (v2 marker, 8
  arguments), `dev_stage_cleanup_root_for`/`guardian_stage_cleanup_root`
  (relocated stage shape), and the focused existing launcher tests.
- `crates/keld-runtime/src/macos_guardian.rs` (`SUPERVISED_GUARDIAN_ARG` v2) and
  `macos_guardian/supervision_runner.rs` (`run_guarded_primary` one-shot
  selection).
- `crates/keld-core/src/app_session.rs` (Guardian argv, Host graph open/hash)
  and `app_session/boot_contract.rs` (schema 3 `graph`).
- `crates/keld-cli/src/boot.rs` `stage_dev_boot` (location, root `package.json`
  capture, graph leaf and descriptor field once GH499's check exists).

Must not touch: GH499's admission logic, GH502's resolver/package, GH452's
environment carrier, KEL-141 runtime acquisition, the kipc wire, Guard policy,
or the workspace dependency set. No new crate/helper framework is justified.

## 6. Tasks (ordered, each one reviewable concern)

- [ ] T0: publish this amended draft for exact-source external review and obtain
  exact-text approval, landing the KEL-96 layout and architecture 06 §1
  amendments in the same PR. If T1 is to be credited to #593, land #593 AC1's
  conformance entry first. No production implementation before approval.
- [ ] T1 (KEL-75, macOS Guardian factory): land the failing-first AC4, AC5 and
  AC6 tests (§7), then add `--no-env-file --no-install`, the frozen
  environment map, the `BUN_INSPECT_PRELOAD`/`NODE_PATH` refusal and the
  binary-name census test. Keep the `BUN_OPTIONS` removal and the stage
  `bunfig` refusal unchanged. Keep other roles out of scope.
- [ ] T2 (KEL-96/CLI producer, boundary change): land the failing-first AC10
  test, then relocate the macOS stage out of the project tree, capture the
  root `package.json`, and update the Host/Guardian stage validators. Compose
  with GH1037's actual normal producer when that artifact exists.
- [ ] T3 (GH499 + KEL-96 + KEL-75): once GH499's package-set check exists, land
  the failing-first AC1 and AC11 tests, then write its staged leaf and add
  descriptor schema 3 `graph`, the v2 Guardian marker, the one-shot Guardian
  check and the per-generation recheck. Prove package refusal before
  evaluation and a healthy sibling.
- [ ] T4: prove selected G1→G2 conservation, hostile ambient changes and fresh
  credential/revocation behavior through the existing Guardian/Supervisor.
  Do not create a new restart loop or infer other-OS/product acceptance.

## 7. Test plan

Every row marked "Yes" is written first and observed failing against the
current code for the stated reason before its fix lands (root AGENTS: "bugs
land a regression test before the fix"). Rows marked otherwise state why.

| AC | Independent observable and required one-fault control | Failing first on current code |
|---|---|---|
| 1 | Actual produced stage; GH499's check reads the installed tree as **data** (real npm `electron` only as data). Missing alias, root substitution, and dependency-local `node_modules/dep/node_modules/electron` each refuse with the typed error before the app or foreign sentinel; the admitted installed-alias positive runs. | Yes: no admission exists, so no typed refusal precedes evaluation. |
| 2 | Actual executable/stage/entry identity and existing typed refusal. Replace one selected object; no fallback image executes; a valid same-input command stays positive. The executable cell is reported unmet until KEL-141 and is not written against `PATH`. | Executable cell: unmet by design; stage/entry cells are existing tests. |
| 3 | Census test: extract the `BUN_*`/`NODE_*` names from the pinned binary (revision `38afee6c8`; a different binary fails rather than skips) and require each to be classified in §4 or recorded UNKNOWN. Control: one unclassified name makes it fail. Negative tests for the "Excluded (one run)" rows (parent-folder and global/XDG `bunfig`, `NODE_OPTIONS`) through the real factory command. A self-reimplemented resolver is not the oracle. | Yes: no census exists. |
| 4 | Extend `guardian_inherited_preload_cannot_run_before_qualified_capture`. Set `BUN_INSPECT_PRELOAD` non-empty, then empty: selection returns the typed error naming the preload class and `INSPECT_PRELOAD_RAN` never prints. Healthy control without the variable prints `CANONICAL_PRELOAD` then `ENTRY_RAN`. Remove only the gate: E1b's order `CANONICAL_PRELOAD`, `INSPECT_PRELOAD_RAN`, `ENTRY_RAN` returns. | Yes: E1b runs today. |
| 5 | `NODE_PATH` naming a directory with a sentinel package: typed refusal and no `NODE_PATH_PACKAGE_EVALUATED`; remove only the gate and the marker prints (E7a); the canonical preload and entry still run in the healthy control. A real child prints its redacted environment: G1 and G2 each equal the frozen map plus `KELD_APP_LINK`, with no parent mutation. GH452's declared-key/floor cells remain its owner. | Yes: E7a is honored today. |
| 6 | Fixture with **no** `node_modules` in the stage or any ancestor (otherwise E6e/E6f mask the negative), an isolated `BUN_INSTALL_CACHE_DIR` holding a tampered cached package under the missing alias's name: the command from the selected factory exits non-zero, `CACHE_PACKAGE_EVALUATED` never prints, and no install/network witness appears. Remove only `--no-install` and the marker prints (E5l). A stage `bunfig.toml` with `[install] auto = "force"` refuses before spawn. Separate positive: a stage whose captured `node_modules` holds the admitted installed alias runs. Help/argv alone cannot pass. | Yes: no `--no-install` today, so E5i evaluates the cached package. |
| 7 | Same Host/Guardian session G1→G2. After G1, add or change a package in the live project `node_modules` and change an ambient value; G2 evaluates neither and retains the exact selection, or refuses before its candidate marker. Separate fresh-link/old-token negative proves lifecycle; one pass does not prove the other. | Yes for the live-project change while the stage sits under the project (E3a). |
| 8–9 | Typed error and bounded existing cleanup after failed preparation; the healthy hello project still starts after relocation. Assert unchanged policy/Guard results and honest unsupported OS/product labels; no retry/deadline relaxation. | No: the healthy project is the existing positive baseline and must stay green through T2. |
| 10 | Project whose `node_modules/foo`, `tsconfig.json` `paths` shim and `package.json` `imports` target exist only in the live project; a staged entry imports each (three cases). After relocation each fails before its `PROJECT_*_EVALUATED` marker and the app sentinel, and the healthy captured stage starts. Control: the same fixture staged under `<project>/.keld/dev/<nonce>` prints the markers (E3; reproduced 2026-10-31). | Yes on the current `stage_dev_boot` location. |
| 11 | (a) After Host validation, change one byte of the staged graph leaf before Guardian selection; (b) launch the Guardian with a well-formed wrong digest; (c) change one bound byte between G1 and G2. (a) and (b) refuse before the first Bun command with no candidate marker; (c) refuses in G2's `prepare` through the failed-preparation lease path. Each has an unchanged control that starts. The assertion names the graph-digest input class, so today's 7-argument parser rejecting an 8-argument call as malformed does not count as a pass. | Yes: no carrier exists. |

Use temporary resources already owned by the fixtures, observable milestones,
one builder and real macOS for claimed runtime/OS results. Before each run,
check that no fixture ancestor holds `node_modules`, `package.json`,
`tsconfig.json`, `jsconfig.json`, `bunfig.toml` or `.env`, and use an isolated
`HOME` and install cache. No sleeps, broad environment rebinding, warm-cache
inference or foreign-package import to cause the refusal. No `.env*` file edits
are authorized by this draft; owners must use permitted independent
fixtures/input receipts for that control. Format, warning-denied workspace
Clippy/tests and applicable hosted gates remain required for the later
implementation. None is claimed run by this draft.

## 8. Review gates triggered

- Public API/architecture: runtime startup selection and prepared-entry
  compatibility contract; the stage-location boundary change against the
  approved KEL-96 layout and architecture 06 §1 (I/O and trust boundary;
  handle, crash and principal ownership unchanged); the proposed one-shot
  selection parameter of `keld-runtime`'s public `run_guarded_primary`. The
  `guardian_bun_command` seam is private and proposed.
- Permission model/security: untrusted startup code must not run before
  admission; the relocated stage changes which directory the Host's cleanup
  validator may delete (`dev_stage_cleanup_root_for`,
  `guardian_stage_cleanup_root`); the loader-control deny-list is explicitly
  non-exhaustive; grants and OS containment remain unchanged and separately
  owned.
- Wire protocol (private): the boot-descriptor schema-3 `graph` field and the
  v2 Guardian marker with 8 arguments are KEL-96 private carrier/schema changes
  under its wire/schema review (KEL-96 lines 789–790 and 814–817). Public kipc
  wire: none.
- Unsafe: none proposed. Dependency addition: none; hashing reuses
  `keld-core`'s existing `sha2` edge.

The user's external reviewer owns qualifying review. This author supplies
neither SourceGO nor implementation/product approval.

## 9. Perf impact

No measured performance claim. This is cold selection/identity work and one
retained immutable value, with no new steady-state queue or watcher. The
per-restart recheck of graph-bound files is cold-path I/O proportional to the
bound set; its cost on a large captured installed tree is unmeasured. Before any
performance claim, separately count input census (bound file count), work
(bytes hashed), reads/copies, queue, clock/statistic and artifact identity.
Existing simpler prepared-entry behavior is the compatibility baseline; no
rewrite is justified by language or file count.

## 10. Open questions — decided (delegated, awaiting exact-text approval)

The first draft's four open questions were decided on 2026-10-31 under the
user's standing architecture delegation (decision record above). Each entry is a
decision with rationale and a falsifier, not exact-text approval of this
amendment.

1. **D1 — GH499 delivered graph owner and stage location.** Decided: GH499 owns
   a data-level installed-package-set admission. The root `electron` resolves to
   the approved `@keld/electron`, no other installed package is named
   `electron`, and lock plus installed identities are captured. There is no
   import walker and no second resolver; `bun build --metafile` is rejected
   (executes app macros, static imports only, bundler resolution not shown
   equal to `bun run`). KEL-75 owns closing the alternative resolution routes:
   `NODE_PATH` (D3), live `tsconfig.json` `paths`, `package.json` `imports` and
   ancestor `node_modules` (stage location). The stage MUST NOT sit under the
   project tree; this boundary change is written into §4 before T2.
   Falsifier: a package present only in the live project `node_modules`,
   imported from a staged entry, fails before any app sentinel while the
   healthy frozen stage passes (AC10), plus the AC1 refusal cases.
   UNKNOWN: the exact staged leaf name and schema of GH499's artifact; whether
   the Host re-runs GH499's check or only compares a digest; how GH1037's
   all-symlink refusal interacts with isolated-linker installs ([F-run] this
   machine's `~/.bunfig.toml` sets `[install] linker = "isolated"`; [I] such
   trees are made of symlinks); whether draw.io performs a non-literal
   `require` that resolves outside `node_modules`; admitting or refusing a
   captured `package.json` `imports` map and stage-root `tsconfig.json`/
   `jsconfig.json` `paths`; the concrete stage location and whether Linux and
   Windows stages move too.
2. **D2 — Host→Guardian carrier (KEL-96 + GH499).** Decided: CLI→Host carries
   a closed `graph: {file, content_sha256}` under a new descriptor schema
   version, mirroring `permissions`, and the Host hashes the bytes it opened.
   Host→Guardian adds one hex-digest argument under the v2 marker (8
   arguments). The Guardian checks the digest once after the authentication
   handshake and before `Supervisor::start_prepared`, and rechecks the bound
   files on every restart in `guardian_bun_command`. Rejected: per-restart
   `PREPARED` payload, Guardian re-parsing `keld.boot.json`, an environment
   variable, a path-only sidecar. Falsifier: a wrong-stage or changed-graph
   digest refuses before the first Bun command is constructed, and again on G2
   (AC11). UNKNOWN: whether the Host may trust the producer's GH499 run or must
   re-run it; the per-restart recheck cost (unmeasured, §9); whether the
   relocated stage's validator change lands in the same KEL-96 amendment as
   the carrier.
3. **D3 — effective Bun 1.4.4 startup inputs (KEL-75).** Decided: the Guardian
   freezes one environment map before the first launch and reuses it on every
   restart; `BUN_INSPECT_PRELOAD` and `NODE_PATH` present refuse with a typed
   error (proposed `KELD-CORE-037`, §4); the existing `BUN_OPTIONS` strip is
   kept unchanged in this slice; the qualified command adds
   `--no-env-file --no-install`; this slice uses a deny-list with a census test
   over the names in the pinned binary, and the allow-list migration stays
   GH452's. Falsifier: one unqualified preload or redirect sentinel fails
   before its marker while the canonical KIPC preload still runs (AC4, AC5).
   UNKNOWN: other `BUN_INSPECT*` names, `BUN_RUNTIME_TRANSPILER_CACHE_PATH`,
   `BUN_JSC_*`, `jsconfig.json`, `tsconfig.json` `extends` outside the stage,
   Keld-private variables inherited by the child, and whether the one-run
   exclusions hold elsewhere.
4. **D4 — narrow no-runtime-acquisition guarantee.** Decided: the
   macOS-primary-only property in AC6 is accepted as GH593's first partial
   delivery, consistent with GH499 A15's auto-install clause and GH502 §7 for
   consumers that launch only through this path, under the three §4
   conditions. It is not a replacement for #593. Falsifier: with no
   `node_modules` in the stage or any ancestor and a tampered warm cache, the
   missing-alias import refuses with no cache marker, removing only
   `--no-install` makes the marker print, a stage `[install] auto = "force"`
   refuses before spawn, and the intact installed alias still runs (AC6).
   UNKNOWN: whether draw.io's first proof starts any Bun role besides the
   primary; whether a pinned production runtime behaves like the development
   `PATH` Bun; Windows and Linux behavior.

Still open and not settled by D1–D4: exact-text approval of this amendment
(user/repository owner through the external reviewer); the KEL-96 and
architecture 06 §1 text amendments; KEL-141's executable for AC2; GH499's
artifact; #593 AC1's conformance entry before any #593 credit.

Until exact-text approval of this text and the failing-first tests in §7 exist,
general-tree production startup remains blocked. The original prepared-entry
contract, GH1037's bounded verification and the existing approved source
decisions are retained; no repeated approval request for their unchanged bytes
is required.

### Current-documentation receipt (2026-10-31)

- Applicability: Bun startup selection/preload/environment/runtime-install
  semantics. Context7 used `/oven-sh/bun` for the bounded startup-input query;
  it advertised 1.4.0/1.4.2 versions, so discovery was not used as 1.4.4 authority.
- Official primary: [runtime options](https://bun.sh/docs/runtime),
  [Bun configuration](https://bun.sh/docs/runtime/bunfig),
  [environment](https://bun.sh/docs/runtime/environment-variables),
  [module resolution](https://bun.sh/docs/runtime/module-resolution),
  [auto-install](https://bun.sh/docs/runtime/auto-install), retrieved 2026-10-31.
  These rolling pages identify current 1.4.4; pinned source is authoritative for
  version-specific code claims, and actual controls remain required.
- Pinned primary source: Bun
  [`Arguments.rs:1019–1083,1209–1210`](https://github.com/oven-sh/bun/blob/38afee6c84fcf48c0edb2466aa4b27d287a59c30/src/runtime/cli/Arguments.rs#L1019),
  [`bunfig.rs:262–310`](https://github.com/oven-sh/bun/blob/38afee6c84fcf48c0edb2466aa4b27d287a59c30/src/bunfig/bunfig.rs#L262),
  [`env_loader.rs:624–653`](https://github.com/oven-sh/bun/blob/38afee6c84fcf48c0edb2466aa4b27d287a59c30/src/dotenv/env_loader.rs#L624),
  [`arguments.rs:142–153`](https://github.com/oven-sh/bun/blob/38afee6c84fcf48c0edb2466aa4b27d287a59c30/src/bunfig/arguments.rs#L142).
- Supported claims: Bun has active preload/path/environment inputs;
  `BUN_OPTIONS` supplies arguments; the pinned parser maps `--no-env-file` to
  disabled default files and `--no-install` to disabled runtime cache acquisition;
  explicit env-file handling is distinct. Pinned preload construction also
  reads `BUN_INSPECT_PRELOAD`. These facts support proposed mechanisms, not a
  claim that all runtime input paths are qualified.
- Configuration limit: current docs distinguish `bun run` local runtime config
  from package-manager global configuration, while runtime auto-install has
  its own registry/config discovery. No universal global-config exclusion or
  CLI-overrides-every-config claim is used. An explicit override flag has not
  been chosen as an admission workaround.
- Fallback/blocker: the one-run local controls in §4 (packet Appendix A and the
  2026-10-31 reproduction) are not Keld-harness, CI or other-OS results.
  Keld-harness suppression/preload/env/install/fresh-generation controls remain
  UNRUN, and the UNKNOWN rows remain unqualified until their named owners
  settle them.
