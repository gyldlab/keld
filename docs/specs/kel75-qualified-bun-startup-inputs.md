# Spec: qualified Bun startup selection for the macOS development primary
Status: draft
Linear: KEL-75 · Owner: GYLDLAB / @0monish · Updated: 2026-10-31

This is a separate proposed amendment to the approved KEL-75 contract. It does
not amend that approved file by implication. Production implementation requires
exact-text approval and the resolved owner interfaces in §10. No new launcher,
module resolver, source copier, configuration parser or approval registry is
authorized. GH1037's verification and draft publication remain independent.

## 1. Goal & non-goals

Before the existing macOS Guardian starts a Bun primary from a general source
stage, bind its effective startup selection to qualified inputs. Either the
existing command factory constructs the selected invocation before any candidate
module can evaluate, or it returns a typed refusal. Every successor uses the
same selected runtime, stage, configuration, preload graph and ordinary
environment, with only the existing fresh generation bootstrap changing.

Non-goals: completing GH499's delivered graph; implementing GH502's package;
general module/config discovery in KEL-75; a bundler or preload plugin; changing
the stage layout/copy rules; adding a public configuration key, flag, file,
wire message, principal or permission; source re-staging on restart; installer
or signed-runtime acquisition; renderer/preload isolation; other-OS support;
changing the approved role/lifecycle/Guard contracts; Strict or product proof.

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
  cannot establish complete effective startup inputs.
- Approved GH499 contract at `5e6a8269056668b8408bad83ce925ccae217cb71`,
  A01/A08/A10/A15; approved GH502 contract at
  `fef3b9db2475aaf9449f2f298cc67dc2c82909eb`, R05/R07/R08 and §7.
  [GH499 coordination](https://github.com/gyldlab/keld/issues/499#issuecomment-6543672236)
  names the producer assignment; it does not supply a completed predicate.
- GH452 owns any new declared environment carrier/reserved-name floor, together
  with KEL-96 schema and KEL-75 spawn approval. GH593 owns broader missing-alias
  runtime-install hardening. GH467/GH484 own application browser-preload
  behavior; KEL-78 owns OS containment. These are distinct owners.

### Current facts and limits

At main `0f5128462d1329ad1945e6639c1956d38242dd4f`,
`keld-host/src/main.rs::guardian_bun_command` validates stage/entry identity,
refuses staged-root `bunfig.toml`/`.bunfig.toml`, selects the optional canonical
`src/kipc-transport.ts` CLI preload and removes inherited `BUN_OPTIONS`.
`GuardedPrimaryPreparer::prepare` calls that factory for each generation.
Existing command/sentinel tests cover those specific controls. This does not
prove complete source/package/configuration/environment qualification.

Local metadata-only observation identifies Bun `1.4.4+38afee6c8`, binary SHA256
`0a2237cdcfa0d16ef501d9acca9f0026309ef348bea7393a64e8d1e3609565cc`.
Official `bun-v1.4.4` resolves to source
`38afee6c84fcf48c0edb2466aa4b27d287a59c30`. Version/help and source inspection
are not a real invocation/negative-control pass. No runtime upgrade is proposed.

## 3. Acceptance criteria (binary, each becomes a test)

1. **One external graph input.** Given GH499's admitted exact delivered graph
   for the actual stage/importer contexts, the KEL-75 selection retains that
   binding; missing, foreign, incomplete or wrong-stage admission refuses
   before any application, dependency or candidate preload evaluates. The
   package-local singleton and an alias string cannot satisfy this criterion.
2. **Runtime and entry.** The initial selection identifies one absolute Bun
   executable and source/version-qualified behavior, the validated private
   stage and the existing admitted entry identity. A different executable,
   stage or entry refuses before candidate evaluation. Development identity
   checks do not claim signed-release or malicious-same-user authenticity.
3. **Effective input completeness.** Every startup-affecting config, path map,
   package context, explicit/default environment-file source and preload in
   the qualified runtime's actual selection is either represented by its
   existing owner's admitted binding or known to be excluded by a qualified
   existing mechanism. Unqualified/unknown selection refuses; do not silently
   copy, ignore, synthesize or override active configuration to obtain a pass.
4. **Preload order and identity.** The selected canonical KIPC preload, if
   present, retains its existing artifact/namespace contract. No extra config,
   CLI or inherited preload can execute first or replace/extend it. A healthy
   selected preload and normal entry remain observable. Absence continues to
   use the existing no-sidecar path where that prepared-entry contract allows it.
5. **Ordinary environment.** Capture the owner-qualified ordinary environment
   once before the first Bun invocation; each prepared command has exactly that
   effective map, plus the existing per-generation `KELD_APP_LINK`. Unknown
   loader/runtime controls refuse rather than inheriting an unqualified value.
   No parent/global environment mutation, new bootstrap variable or app-selected
   authority is permitted. Secret values are not printed or stored in receipts.
6. **No runtime acquisition in this qualified path.** Every child constructed
   from this selection disables runtime dependency auto-install and supplies no
   package-manager/script-dispatch fallback. A missing installed target cannot
   evaluate a warmed-cache or foreign package. An installed admitted package
   still works. This is independent of installed graph identity.
7. **Frozen recovery.** After G1 retires, the existing preparer constructs G2
   from the same selected inputs or refuses before execution. It never re-reads
   source configuration/environment into a different selection. A changed
   still-observed stage/runtime/config identity causes refusal; source changes
   outside the captured stage do not silently re-stage. Fresh token/principal
   and old-generation revocation remain independently required.
8. **Failure and compatibility.** Any refusal preserves the owning typed error,
   input class and remedy, creates no candidate module marker and uses existing
   lease/reap/stage cleanup. No deterministic retry, alternate executable,
   config deletion, source flattening or grant broadening follows. A healthy
   prepared-entry/canonical-transport project retains existing behavior.
9. **Scope honesty.** The result changes neither Guard decisions nor OS
   containment, renderer origins or product receipts. Other roles/platforms and
   the wider GH593 matrix are not reported qualified by this macOS-primary slice.

## 4. Design

### First principles and reuse

**No boundary change:** CLI remains the stage producer; Host/Guardian retain
their current process/crash ownership; the existing generation owner mints
identity; Guard authorizes privileged operations. This amendment changes the
inputs accepted by the existing spawn preparation, not handle or authority
ownership. No renderer receives a new handle, endpoint or selection capability.

| Atom / owner | Input → output and failure | Independent first falsifier |
|---|---|---|
| Graph identity / GH499, GH502 consumer | Actual delivered files/lock/importers → existing-owner graph admission; wrong package/stage refuses | Dependency-local foreign Electron marker executes before refusal |
| Selection / KEL-75 preparer | Qualified graph + runtime/config/env facts → one immutable local selection | Unlisted effective input changes evaluated code |
| Capture / CLI, KEL-140 | Existing retained captured bytes → same stage identity | Policy captured twice or live source replacement selected |
| Authentication / existing bootstrap | Fresh generation secret → authenticated link bound to that generation | A selection digest or env value treated as a principal |
| Authorization / Guard | Unchanged verified policy → same allow/deny | Qualification changes a grant or bypasses dispatch |
| Containment / KEL-78 | Existing profile → existing OS claim | Startup pass reported as Strict containment |
| Lifecycle / Supervisor and lease | Same selection + fresh generation → one child/revocation/reap path | G2 rereads ambient input or old authority survives |
| Evidence / named input owners | Pinned source plus actual controls → scoped acceptance | Help/source presence reported as executed behavior |

The exact runtime/graph/stage identity is an explicit edge between atoms.
None of these passes substitutes for another. Memory is one cold, immutable
selection retained by the current preparation owner; generation-specific
bootstrap values remain lease-owned. Source/graph capture I/O stays with its
existing owner; preparation may perform only the owner's defined identity
rechecks. No new hot-loop allocation, queue or background watcher is proposed.

Existing primitives considered:

- Retain the existing `guardian_bun_command` factory and
  `GuardedPrimaryPreparer` callback/lease path. Its root-config refusal and
  `BUN_OPTIONS` removal are real partial controls, not a reason to bypass
  effective-input qualification.
- Retain the existing source/policy capture and upstream Bun resolver.
  A KEL-75 import walker/config parser would duplicate owned semantics and is
  rejected. GH499 supplies the graph; this owner validates its applicability.
- Retain the actual pinned Bun command-line primitives where source/docs support
  them; qualify their resulting behavior before claiming an enforcement pass.
- A package singleton, generic file-copy success, metadata hash by itself,
  late import error or `--no-install` alone cannot prove pre-evaluation graph
  identity. A second resolver/plugin and a new approval registry are rejected.
- Copying all active startup inputs or silently discarding them is rejected.
  The first slice refuses unqualified inputs instead of changing app semantics.

### Proposed internal input/output interface

The **logical** input is one KEL-75-owned `QualifiedBunStartup` value, constructed
only after the following owner results have passed. It is private process-local
state, not a new user file, manifest, environment token or public Rust API.

| Required field | Exact meaning and owner |
|---|---|
| Runtime identity | Absolute selected Bun executable, byte/OS identity, version/revision and qualified selector behavior; KEL-75 consumes the existing runtime-acquisition owner for installed releases. This development slice does not require installed-release completion. |
| Stage and entry | Existing validated executable-located root, relative entry, retained identity and boot mode; KEL-96/CLI remain owners. |
| External graph binding | GH499's exact producer/artifact identity, actual stage/package/lock/file/importer binding and non-evaluating acceptance result; GH502 R07 consumes it. Its concrete owner interface is unresolved in §10; a bare hash/author assertion is invalid. |
| Effective selection | Owner-qualified list/binding of the actual config/package/path/preload inputs and explicit absence/exclusion facts, including their capture epoch. No guessed discovery algorithm is part of KEL-75. |
| Canonical preload | Absent under the current compatible prepared-entry contract, or the exact existing transport artifact and resolved transitive identity in the admitted graph. A caller-supplied substitute is invalid. |
| Ordinary environment | Captured effective key/value map and qualification result; values retained in memory only. App-declared keys require GH452's separately approved implemented carrier/floor. |
| Fixed argv intent | Existing host-selected boot mode, canonical preload when present, absolute entry and the qualified runtime suppression options below; no app-provided Bun flags, package script, inspector, watch or package-manager invocation. |

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
the closure borrows the same immutable selection and receives only that attempt's
fresh app-link. No new supervisor/trait, bootstrap variable or wire field follows.

Host and Guardian are **different processes**. A Rust value in the Host is not
automatically available in the Guardian. KEL-96 and GH499 must settle how their
existing validated input selection supplies the above graph binding at that
boundary before this design can be approved. This draft selects no extra argv,
sidecar, serialization format or parser and does not pretend that carrier exists.
The Guardian constructs/retains its one local selection after that verified
handoff and before its first Bun preparation.

Output is either the one prepared child command plus the existing generation
lease, or an error before candidate evaluation. A successful graph/selection
check may precede fresh endpoint provisioning; if provisioning already occurred,
the existing failed-preparation lease path must revoke it before returning.
No app, dependency, preload, macro or package script is imported to perform
admission. A no-entry marker alone is insufficient if an earlier preload ran.

### Exact proposed qualified-path invocation

Use the selected absolute executable and absolute entry, existing stage cwd,
fixed native-facade define and optional canonical `--preload` selection. For
this path, add the pinned runtime's `--no-env-file` and `--no-install`; pass no
`--env-file`, app-controlled options, install mode or package-script name.
These are proposed mechanisms supported by the pinned source/doc receipt below,
not test results. Automatic environment files are refused as a source of startup
values; required values must already be in the owner-qualified ordinary map.
The draft adds no generated Bun configuration file to disable them.

Preserve existing staged-root Bun-config refusal. Any additional effective
runtime/path configuration must be explicitly admitted by its existing graph/
startup owner or startup refuses. No `--config`/`--tsconfig-override` workaround
is selected. The exact selected input set and selection exclusion proof must
account for the actual runtime command and importer contexts, including package
auto-install configuration where applicable; a blanket claim that no global
configuration matters would be unsupported.

The existing `BUN_OPTIONS` removal remains. Pinned Bun source also reads
`BUN_INSPECT_PRELOAD`; a nonempty unqualified value must refuse before child
creation, not merely be logged or accepted after the canonical preload. This
known control is not an exhaustive list of honored environment names. The exact
qualified ordinary map/startup-control inventory is a blocking §10 decision.
Apply only the selected map to the child using ordinary `Command` operations;
do not mutate process-global environment. Only the existing per-generation
`KELD_APP_LINK` differs between generations. No new declared-env grammar is inferred.

### Recovery, errors and the auto-install edge

The selection is made once before the first Bun generation. The factory reuses
that value, rechecks the owning runtime/stage identity predicates and refuses a
changed still-observed input. It never recaptures ambient env/config/package
selection into G2. Generation-specific credentials remain fresh and are revoked
by the existing owner. This is the cooperating-developer stage's existing trust
level, not a new atomic-filesystem or same-user attacker guarantee.

Reuse `KELD-CORE-037` at the existing Guardian boundary and preserve the nested
runtime failure path. The message names the input class and instructs restaging
through `keld dev` with an owner-qualified graph/configuration; it never suggests
permission broadening, deleting user configuration or trying another resolver.
No new error code is reserved here. A non-owning stage/graph provider keeps its
own typed failure rather than being swallowed or converted into success.

GH593 is milestone-next because the installed-alias first proof does not
intentionally request auto-install. **This draft does not require all of GH593
to finish for GH1037 verification, draft publication or a pure sibling diagnostic.**
Approved GH499 A15 and GH502 §7 nevertheless require no runtime acquisition at
their concrete consumers. The proposed bounded reconciliation is to qualify
that property on this existing selected macOS-primary path, preserving the
installed alias positive and missing-alias/cache negatives. GH593 retains the
later all-role/other-path scope and declared-key/fresh-generation coverage where
those owners introduce it. No issue label or help flag is acceptance evidence;
Root and the source owners must approve this exact applicability at §10. An
installed foreign package remains GH499/R07 rejection, not auto-install hardening.

Capabilities/manifest: none. Public kipc, principal and grants: unchanged.
Private boot-carrier changes, if required, belong to KEL-96 and need a separately
reviewed contract before approval. Browser preload/origin and OS containment are
unchanged. Windows/Linux keep existing behavior and are explicitly unqualified.

## 5. Boundaries

This draft changes only `docs/specs/kel75-qualified-bun-startup-inputs.md` and
owner linkage on KEL-75. Do not edit the approved original spec, GH499/GH502
contracts, GH1037's active fixture, architecture docs, generated corpus, Cargo,
CI fingerprints, environment files or production code in this draft concern.

After approval, the smallest implementation belongs to the existing macOS
Guardian command factory/preparer and focused existing launcher tests. CLI
capture and GH499 graph production stay with their owners. Any required KEL-96
carrier/schema amendment is a named predecessor, not scope silently acquired
by this document. No new crate/helper framework is justified.

## 6. Tasks (ordered, each one reviewable concern)

- [ ] T0: publish this draft for exact-source external review; bind the existing
  GH499 graph input, KEL-96 cross-process carrier and environment inventory,
  then obtain exact-text approval. No production implementation before approval.
- [ ] T1: qualify the selected macOS command with actual pinned-runtime controls
  in the existing factory harness, including a successful canonical preload/
  entry and killed preload/env/install faults. Keep other roles out of scope.
- [ ] T2: compose the qualified selection with GH1037's actual normal producer
  and GH499's admitted graph when those owner artifacts exist. Prove exact
  source/input identity, package refusal before evaluation and healthy sibling.
- [ ] T3: prove selected G1→G2 conservation, hostile ambient changes and fresh
  credential/revocation behavior through the existing Guardian/Supervisor.
  Do not create a new restart loop or infer other-OS/product acceptance.

## 7. Test plan

| AC | Independent observable and required one-fault control |
|---|---|
| 1 | Actual delivered-stage graph admission; remove alias, root-substitute and dependency-local-shadow with foreign package as **data**. Both app and foreign/preload sentinels stay absent; admitted installed-alias positive remains healthy. |
| 2 | Actual executable/stage/entry identity and existing typed refusal. Replace one selected object; no fallback image executes. Keep a valid same-input command positive. |
| 3 | Owner's effective-input matrix, including inherited/wildcard path mappings and startup config. Mutate exactly one admitted identity or introduce an unqualified selected input; reject before its sentinel. Presence-only checks or a self-reimplemented resolver are not the oracle. |
| 4 | Reuse `guardian_inherited_preload_cannot_run_before_qualified_capture` and existing no-sidecar/config tests. Add known `BUN_INSPECT_PRELOAD` negative plus an unapproved config/preload redirect. Remove the relevant gate and observe the hostile marker while legitimate markers still work. |
| 5 | Exact child effective map with redacted evidence and no parent mutation. One allowed data-key change is reflected only after a new selection; forbidden/unqualified loader key refuses before spawn. GH452's separate declared-key/floor cells remain its owner. |
| 6 | Actual command from selected factory, missing installed alias with independently prepared warm cache: no foreign evaluation/install/network witness. Remove only suppression and require the original negative to fail; admitted installed dependency still executes. Help/argv alone cannot pass. |
| 7 | Same Host/Guardian session G1→G2, change only an ambient selection input after G1. G2 either retains exact qualified selection or refuses before candidate marker. Separate fresh-link/old-token negative proves lifecycle; one pass does not prove the other. |
| 8–9 | Typed error and bounded existing cleanup after failed preparation; subsequent healthy project works. Assert unchanged policy/Guard results and honest unsupported OS/product labels; no retry/deadline relaxation. |

Use temporary resources already owned by the fixtures, observable milestones,
one builder and real macOS for claimed runtime/OS results. No sleeps, broad
environment rebinding, warm-cache inference or foreign-package import to cause
the refusal. No `.env*` file edits are authorized by this draft; owners must
use permitted independent fixtures/input receipts for that control. Format,
warning-denied workspace Clippy/tests and applicable hosted gates remain
required for the later implementation. None is claimed run by this draft.

## 8. Review gates triggered

- Public API/architecture: runtime startup selection and prepared-entry
  compatibility contract; the shown Rust seam is private and proposed.
- Permission model/security: untrusted startup code must not run before
  admission; grants and OS containment remain unchanged and separately owned.
- Unsafe: none proposed. Dependency addition: none. Public wire protocol: none.
  A selected private boot carrier would require its KEL-96 wire/schema gate;
  it is unresolved and not authorized here.

The user's external reviewer owns qualifying review. This author supplies
neither SourceGO nor implementation/product approval.

## 9. Perf impact

No measured performance claim. This is cold selection/identity work and one
retained immutable value, with no new steady-state queue or watcher. Before any
performance claim, separately count input census, reads/copies, work, queue,
clock/statistic and artifact identity. Existing simpler prepared-entry behavior
is the compatibility baseline; no rewrite is justified by language or file count.

## 10. Open questions

1. **GH499 owner / external graph input:** supply the exact non-evaluating
   actual-stage/importer graph admission artifact and validation interface.
   Root's coordination comment 6543672236 is an assignment edge, not its completion.
   First check: missing/root-foreign/dependency-local foreign Electron refuses
   before candidate/app sentinel while the admitted installed graph is healthy.
2. **KEL-96 + GH499 / process boundary:** select and review how the existing
   validated boot selection passes that binding to the separate Guardian.
   Current root/entry/device/inode/boot-mode arguments do not carry it. No new
   sidecar/argv/schema/parser has been selected here. First check: wrong-stage
   or stale binding cannot construct the first Bun command.
3. **KEL-75 / exact selection and environment inventory:** freeze the full
   honored input/discovery set for the pinned command and actual importer/cwd,
   including whether any package-auto-install config remains readable after
   suppression. The known controls above do not prove completeness. Select the
   exact ordinary map and prove legitimate startup remains healthy; do not
   guess a universal allowlist or broaden GH452's declared-env authority.
4. **Root + GH499/GH502/KEL-75 source owners / narrow applicability:** approve
   the qualified-primary no-runtime-acquisition reconciliation above against
   GH499 A15/GH502 §7 and GH593's milestone-next rationale. Do not require
   unrelated GH593 completion or silently waive explicit approved text.

Until these exact decisions and exact-source approval exist, general-tree
production startup remains blocked. The original prepared-entry contract,
GH1037's bounded verification and the existing approved source decisions are
retained; no repeated approval request for their unchanged bytes is required.

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
- Fallback/blocker: actual suppression/preload/env/install/fresh-generation
  controls and complete effective-input inventory are UNRUN/unproved here;
  missing ownership, carrier or selection proof remains the named §10 blocker.
