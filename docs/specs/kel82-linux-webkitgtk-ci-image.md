# Spec: pinned Linux WebKitGTK CI environment

Status: draft
Linear: KEL-82 · Owner: GYLDLAB · Updated: 2026-09-09

## 1. Goal & non-goals

Remove live archive acquisition from the two CI consumers that need WebKitGTK: the
Ubuntu row of `clippy + test` and `linux-gui-smoke`. One protected publisher resolves
an exact Ubuntu 24.04 amd64 package set from signed, timestamped snapshots and publishes
it as an attested public OCI bundle. Consumers pull the locked digest, verify it, and
install from its network-disabled local repository before running their existing Keld
commands on an `ubuntu-24.04` GitHub-hosted runner.

The pinned contract covers package roots, downloaded bundle bytes and their trust
chain. GitHub updates the host image independently, so the consumer records its
`ImageOS`/`ImageVersion` and refuses any incompatible package plan before host mutation.

Non-goals:

- Moving MSRV into Linux. MSRV runs on macOS and owns no WebKitGTK acquisition.
- Pinning the complete GitHub runner OS or installing a frozen base-system closure over
  a newer runner.
- Increasing timeouts, retrying a mirror, accepting an alternate source, or falling
  back to network apt or an unverified cache.
- Running strict Bubblewrap tests inside a nested job container or adding container
  privileges/capabilities to make them pass.
- Changing Keld product code, permissions, IPC, tests, Bun pins, package-manager
  metadata, or application performance claims.

## 2. Spec refs

- [`AGENTS.md`](../../AGENTS.md) § Engineering principles and verification floor
- [`docs/architecture/03-security.md`](../architecture/03-security.md) §4 Linux
  defense-in-depth enforcement
- [`docs/architecture/06-runtime-and-tooling.md`](../architecture/06-runtime-and-tooling.md)
  §1 Linux strict mechanism
- [`docs/specs/kel78-strict-profile-sandbox.md`](kel78-strict-profile-sandbox.md) §4
  Linux candidate and §7 hostile-catalog oracles
- [`.agents/ci.md`](../../.agents/ci.md) § Required workflow and routing
- [`.agents/dependencies.md`](../../.agents/dependencies.md) § Authoritative checks
  and change discipline
- [`.agents/testing.md`](../../.agents/testing.md) § Failure-first proof and CI tiers
- [Ubuntu snapshot service](https://ubuntu.com/server/docs/how-to/software/snapshot-service/)
- [Ubuntu archive verification](https://documentation.ubuntu.com/security/software-integrity/archive-verification/)
- [GitHub artifact attestations](https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/use-artifact-attestations)
- [GitHub Container registry](https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-container-registry)

No architecture or product boundary changes.

## 3. Acceptance criteria (binary, each becomes a test)

1. Given current CI, when package acquisition is classified, then exactly one protected
   publisher may download package bytes, one read-only freshness workflow may fetch
   signed security metadata, exactly two consumer steps may install from the offline
   bundle, and no other executable archive/apt acquisition exists. MSRV is outside all
   four sites.
2. Given `bundle.lock.json`, when it is checked, then its schema, resolver digest,
   architecture, roots, snapshot sources, pockets, components, trusted keyring,
   `InRelease`/`Packages`/`Sources` hashes, sorted binary/source package closure and
   every `.deb` SHA-256 are complete and canonical. `bubblewrap` is an explicit root
   rather than a transitive accident.
3. Given the same lock and recipe, when two clean builds run, then their normalized
   bundle payload, SPDX 2.3 SBOM, OCI layer/config/manifest descriptors and predicted
   subject digest are byte-identical. A live source, changed key, pocket, index,
   package, architecture, timestamp, file metadata or tool pin fails before publish.
4. Given an `ubuntu-24.04` consumer host, when `host preflight` runs with every network
   apt source disabled, then the offline solver either produces a plan with the exact
   root versions and satisfied dependency relations or fails before mutation. Any
   downgrade, removal, Essential-package replacement, missing local `.deb`, or
   unsatisfied dependency is `KELD-CIIMG-004`; the host image identity is in the receipt.
5. Given a passing preflight, when `host install` runs, then only the verified,
   explicitly trusted local repository is enabled, every exact root is installed,
   `dpkg --audit` and the offline dependency check pass, and the receipt records
   installed roots and dependency versions. No package byte is fetched from a network
   source.
6. Given the installed bundle and existing host AppArmor/user-namespace prerequisite,
   when strict Linux tests run, then `/usr/bin/bwrap` has the locked package/file
   identity and the existing filesystem, network, namespace, descriptor and descendant
   denial assertions pass unchanged. Removed host preparation, missing/mutated
   Bubblewrap and a default nested job-container probe each fail without a skip, retry,
   timeout change or widened privilege.
7. Given the Ubuntu `check` and GUI consumers, when workflow, router, evaluator, bundle
   lock or verifier inputs change, then both relevant call sites execute and their
   result/applicability reaches `CI required`. The Ubuntu `check` bundle step is
   unconditional whenever `rust=true`; `webkitgtk` continues to select packages, not
   whether dependency verification runs. The router sets both `rust=true` and
   `gui=true` for workflow, router, evaluator and `ci/linux-webkitgtk/**` changes
   (including locks, subjects, schemas and verifier tests). `linux-gui-smoke`
   consumes `changes.outputs.gui`; `CI required` treats either selected consumer
   being skipped as failure. `webkitgtk` remains a package-selection output.
8. Given the installed bundle in `linux-gui-smoke`, when the job runs, then the existing
   release host build, media-guard build, Xvfb/fluxbox controls, titled window and clean
   close/process checks run without an oracle or command reduction.
9. Given a publication, when provenance and SBOM attestations are verified, then both
   name the exact OCI subject, GitHub OIDC issuer, `gyldlab/keld` repository, protected
   main ref, source commit, SHA-pinned signer workflow and expected SLSA/SPDX predicate.
   Wrong issuer, repo, workflow, ref, commit, subject or predicate fails independently.
10. Given first publication, when T3 completes, then the package administrator records
    public visibility and exact `gyldlab/keld` association, and a clean process with an
    empty Docker config pulls the digest anonymously. `packages: write` and an OCI
    source label alone do not satisfy either check.
11. Given an unavailable, unknown, corrupt, unlinked, unattested, denied or candidate
    subject outside the exact T4a qualification mode, when a consumer starts, then it
    fails before host mutation/compilation. It never accepts a tag, network source,
    cached unverified bundle, `continue-on-error`, retry or skipped-green result.
12. Given rotation, when `subjects.json` changes state, then only
    `candidate → current → previous[] → retired[]` is legal; `denied` is an orthogonal
    terminal trust state and `preferred_rollback_digest` is an orthogonal reviewed
    selector, both checked before any pull. Main is the only supported ref until
    a reviewed manifest adds release refs. Every previous digest remains until its own
    `retain_until`, at least 30 days after supersession; physical GHCR and attestation
    deletion are cleanup actions, while the checked-in denial is the permanent trust
    oracle.
13. Given publication or the daily freshness check, when a newer applicable source
    version or a same-version index/hash change exists in the current
    `noble-security` pocket, then promotion/freshness fails with a read-only receipt.
    The KEL coordinator opens or updates the rotation record outside the workflow, and
    a reviewed replacement is due within 72 hours of first detection. This criterion
    proves published security-update freshness; it does not claim detection before
    Canonical publishes updated package metadata.

## 4. Design

### First-principles decomposition

| Atom | Owner, boundary and input/output | Failure mode | Independent observable |
|---|---|---|---|
| Consumer census | CI router/workflow: diff → publisher/freshness/check/GUI applicability | package download, metadata read and consumer install are conflated; call site skips green | One package resolver, one metadata-only freshness reader, two offline consumers; results admitted by `CI required` |
| Snapshot authentication | Resolver: deb822 sources + keyring → signed indexes | Mirror/pocket/key substitution supplies different packages | Fingerprint and `InRelease` signature/hash for every pocket |
| Package identity | Resolver: signed indexes → exact roots/closure | Missing, foreign-arch or changed `.deb` enters bundle | Canonical lock plus package SHA/source/version checks |
| Bundle identity | Deterministic packer: verified files → OCI subject | timestamps/order/config make builds diverge | Two-build byte and descriptor equality |
| Publisher identity | GHCR: package name + digest → public repository-linked subject | tag, wrong repository or private package is treated as Keld | API visibility/linkage plus anonymous digest pull |
| Publisher authentication | GitHub OIDC/Sigstore → signed attestation | self-hosted/foreign issuer signs | exact issuer, transparency evidence, hosted-runner restriction |
| Publisher authorization | Protected workflow/ref/permissions → allowed publication | PR/fork or different workflow publishes | repo/workflow/ref/source/signer digest and least-privilege negatives |
| Provenance | OCI subject + workflow → SLSA statement | signature exists for a different subject/source | exact SLSA predicate verification |
| SBOM | Package lock → deterministic SPDX statement | incomplete/floating scanner omits bundled package | lock-derived SPDX equality and independent predicate verification |
| Host compatibility | Offline solver: bundle + runner state → plan | downgrade/removal breaks mutable host | no-mutation preflight with exact rejection reason |
| Containment | Runtime owner: bwrap + host kernel policy → strict result | dependency pin greens while sandbox is absent/weaker | unchanged direct OS-denial tests and mutations |
| GUI | Existing smoke owner: installed roots → native-window result | build dependency proof substitutes for product smoke | existing media/title/process/close oracles |
| Lifecycle/revocation | Subject state owner: candidate/current/previous/retired/denied → trust decision | deleted/denied image remains accepted or strands main | pre-pull deny, ref census, transition/attestation cleanup and rollback receipts |
| Security freshness | Scheduled owner: locked source versions + current signed security indexes → fresh/stale receipt | published update is absent or workflow can publish/write tracking state | daily source/index comparison, read-only permissions and 72-hour coordinator record |

No atom proves another. In particular, a valid digest is not an attestation, an
attestation is not a complete SBOM, package installation is not containment, and
containment is not the GUI smoke.

### Machine-owned contracts

`ci/linux-webkitgtk/bundle.lock.json` uses `keld.linux-ci-bundle/v1` and contains
resolver/archive/package inputs only. `bundle.manifest.json` uses
`keld.linux-ci-manifest/v1` and contains hashes/descriptors derived from that input.
Neither file is placed inside the hashed bundle payload, avoiding self-reference. The
generator writes compact UTF-8 JSON with lexicographically sorted object keys, decimal
integers, lowercase SHA-256 and one trailing newline. Array sort keys are:

```text
sources: (uri, architecture)
suites/components/fingerprints/supported_refs: byte value
indexes: (source_uri, suite, component, architecture)
roots: name
packages: (name, architecture)
source_packages: (name, version, index_identity)
previous/denied/retired: digest
receipt.security_indexes: (source_uri, suite, component, architecture)
receipt.stale_sources: (name, locked_version, current_version, index_identity)
receipt.transitions: (effective_at, digest, from, to)
receipt.qualification_receipts: job
receipt.github_run.required_jobs: name
```

Every key is unique in its array; duplicate keys and noncanonical order fail lock
or receipt validation. Each binary package key resolves to exactly one version/file.

Required `bundle.lock.json` fields are:

```text
resolver = { image, digest, architecture: "amd64", tool_versions }
archive = { snapshot_id, snapshot_utc, sources[], indexes[], source_packages[] }
archive.sources[] = { uri, suites[], components[], architecture, signed_by }
archive.sources[].signed_by = { keyring_package, version, file_sha256, fingerprints[] }
archive.indexes[] = { source_uri, suite, component, architecture, inrelease_sha256,
                      packages_sha256, sources_sha256 }
archive.source_packages[] = { name, version, index_identity, stanza_sha256 }
roots[] = { name, version, reason }
packages[] = { name, architecture, version, source_name, source_version,
               source_index_identity, index_identity, filename, sha256,
               pre_depends, depends }
```

`pre_depends` and `depends` are the exact UTF-8 Debian control-field values after RFC822
line unfolding, or the empty string when absent; their token/alternative order is not
rewritten. `index_identity` references exactly one locked Packages index, and
`(source_name, source_version, source_index_identity)` references exactly one
`archive.source_packages[]` record from the corresponding locked Sources index.
`stanza_sha256` hashes the exact RFC822 stanza bytes from that verified index. When
binary metadata has no `Source` field, the generator uses binary name/version and the
Sources index paired with `index_identity` for all three source-reference fields.

SPDX generation consumes only this canonical projection:

```text
{ resolver, archive, roots, packages }
```

Its `DocumentNamespace` derives from that projection's SHA-256, and `Created` is
`archive.snapshot_utc`. The generator then hashes the finished SPDX bytes, constructs
the normalized payload, and finally writes `bundle.manifest.json`:

```text
lock_sha256 = <canonical bundle.lock.json bytes>
payload = { tree_sha256, spdx_path, spdx_sha256 }
oci = { name, manifest_digest, config_digest, layer_digest }
```

The initial exact root-name set is:

```text
libgtk-3-dev libwebkit2gtk-4.1-dev libjavascriptcoregtk-4.1-dev
libsoup-3.0-dev pkg-config xvfb xauth fluxbox wmctrl x11-utils xdotool bubblewrap
```

The compiler/linker remain declared `ubuntu-24.04` host prerequisites and are checked
before installation; `build-essential` is not allowed to drag a frozen libc toolchain
over the mutable runner. Before the apt solver runs, preflight resolves `cc`, `c++`,
`ld`, `ar`, and `make` to executable files, records their real paths, SHA-256 and
version output, and requires the Ubuntu GCC 13 / GNU binutils 2.42 / GNU Make 4.3
host toolchain (patch/package revisions are recorded, not frozen). Both compiler
`-dumpmachine` results must be `x86_64-linux-gnu`; cross-toolchain overrides fail.
In a fresh temporary directory it compiles, links and executes C11 and C++17 probes
using system headers/libc/libstdc++, checks ELF64 little-endian x86-64 output and
zero exit, then removes the probes. `ar` must create/list an archive and `make` must
build that same probe. Every probe has a 30-second kill switch; a missing command,
wrong family/major/target, missing development headers/libraries, nonzero exit or
expiry is `KELD-CIIMG-007` before package mutation. These are proposed admission
requirements to qualify on the two hosted image revisions in AC4, not a claim that
an unmeasured runner passes. Install rechecks tool hashes and the host package-state
hash against preflight; changed inputs require a fresh preflight, never a retry.

`archive.snapshot_id` is a required UTC calendar-valid `YYYYMMDDTHHMMSSZ` string;
`snapshot_utc` is the same instant serialized as `YYYY-MM-DDTHH:MM:SSZ`.
Each source URI is exactly `https://snapshot.ubuntu.com/ubuntu/<snapshot_id>/`;
`source_uri` in every index must match that source. No query, fragment, userinfo,
encoded path, moving alias, live archive fallback or redirect outside the same
snapshot prefix is accepted. The timestamp form follows the linked Ubuntu snapshot
service documentation. Lock validation rejects mismatched IDs before fetching bytes.

Snapshot sources contain only reviewed official Ubuntu URIs, `noble`,
`noble-updates`, and `noble-security`, with `main`/`universe`; proposed/backports and
every host source are disabled. Each lock records the exact deb822 fields, snapshot ID,
Ubuntu archive-keyring bytes/fingerprints, signed indexes and package-origin edge.

`ci/linux-webkitgtk/subjects.json` uses `keld.linux-ci-subjects/v1` and owns:

```text
registry = "ghcr.io"
repository = "gyldlab/keld-linux-ci"
supported_refs = ["refs/heads/main"]
candidate = null | { digest, publisher_workflow_sha256, declared_at,
                     published_source_commit: null | commit,
                     attested_signer_digest: null | commit,
                     published_at: null | timestamp }
current = null | { digest, publisher_workflow_sha256, declared_at,
                   published_source_commit, attested_signer_digest,
                   published_at, qualified_at }
previous[] = { digest, publisher_workflow_sha256, declared_at,
               published_source_commit, attested_signer_digest,
               published_at, qualified_at, superseded_at,
               successor_digest, retain_until }
preferred_rollback_digest = null | digest
retired[] = { digest, publisher_workflow_sha256, declared_at,
              published_source_commit, attested_signer_digest,
              published_at, qualified_at, retired_at }
denied[] = { digest, reason, decision_commit, denied_at }
attestation = { issuer, repository, signer_workflow, source_ref,
                provenance_predicate, sbom_predicate }
```

A candidate requires the precomputable SHA-256 of the reviewed publisher workflow file
and permits only publication-time fields to be null. It cannot become current until
every publication field is non-null. Subject verification resolves the attested signer
commit, checks the exact workflow path, and requires that commit's workflow blob to hash
to `publisher_workflow_sha256`. Removing a previous record instead of appending the
same digest to `retired[]` and producing the transition receipt is invalid. Each
promotion appends the former current digest to `previous[]`; entries coexist and own
their individual 30-day clocks. `preferred_rollback_digest`, when non-null, must name
one retained, non-denied previous entry.

The sole CLI is `ci/linux-webkitgtk/ci-image.sh`:

```text
ci-image.sh lock check --receipt-out <json>
ci-image.sh bundle build --output <oci-layout> --receipt-out <json>
ci-image.sh host preflight --bundle <dir> --receipt-out <json>
ci-image.sh host install --bundle <dir> --preflight-receipt <json> --receipt-out <json>
ci-image.sh subject verify --digest sha256:<hex> --receipt-out <json>
ci-image.sh subject verify --digest sha256:<hex> --candidate-publication \
  --receipt-out <json>
ci-image.sh subject verify --digest sha256:<hex> --candidate-qualification \
  --event <github-event.json> --receipt-out <json>
ci-image.sh subject verify --digest sha256:<hex> --preferred-rollback \
  --receipt-out <json>
ci-image.sh subject census --digest sha256:<hex> --receipt-out <json>
ci-image.sh subject transition --before <subjects.json> --after <subjects.json> \
  --receipt-out <json>
ci-image.sh freshness check --receipt-out <json>
```

Receipts use the same canonical JSON rules. `result` below applies to every
receipt except freshness, whose explicitly separate `freshness_result` union includes
`stale`. Validators dispatch on the exact schema identifier; stale is invalid in
all other receipts:

```text
result = { status: "passed" }
       | { status: "rejected", code: "KELD-CIIMG-00N", failed_predicate }
keld.linux-ci-lock-receipt/v1 = { action: "lock-check", lock_sha256,
  input_sha256, resolver_digest, snapshot_utc, result }
keld.linux-ci-host-receipt/v1 = { action: "host-preflight" | "host-install",
  lock_sha256, subject_digest, image_os, image_version, network_sources,
  plan, toolchain, installed_roots, dependency_versions, preflight_receipt_sha256,
  started_at, completed_at, result }
keld.linux-ci-subject-verify-receipt/v1 = { action: "subject-verify",
  mode: "current" | "candidate-publication" | "candidate-qualification"
      | "preferred-rollback",
  digest, provenance, sbom, qualification, result }
keld.linux-ci-subject-census-receipt/v1 = { action: "subject-census",
  digest, visibility, linked_repository, anonymous_pull, references, result }
keld.linux-ci-build-receipt/v1 = { action: "bundle-build", lock_sha256,
  manifest_sha256, payload_tree_sha256, spdx_sha256, config_digest,
  layer_digest, manifest_digest, output_path, result }
keld.linux-ci-freshness-receipt/v1 = { action: "freshness-check",
  checked_at, snapshot_utc, lock_sha256, subject_digest, workflow_run,
  security_indexes[], stale_sources[], result: freshness_result }
freshness_result = { status: "passed" }
        | { status: "stale", code: "KELD-CIIMG-008", failed_predicate }
        | { status: "rejected", code: "KELD-CIIMG-00N", failed_predicate }
keld.linux-ci-subject-transition-receipt/v1 = { action: "subject-transition",
  before_sha256, after_sha256, transitions[],
  rollback_pointer_change: null | { from: null | digest, to: null | digest },
  qualification_receipts[], census_receipt_sha256: null | sha256,
  freshness_receipt_sha256: null | sha256,
  github_run: null | github_run, result }
transitions[] = { digest, from, to, effective_at }
qualification_receipts[] = { job: "ubuntu-check" | "linux-gui-smoke",
  receipt_sha256 }
github_run = { repository: "gyldlab/keld", event_name: "pull_request",
  workflow_path, workflow_sha256, run_id, run_attempt, pr_number, head_sha,
  checked_out_sha, required_jobs[] }
required_jobs[] = { name, check_run_id, conclusion: "success" }
qualification = null | { event_name: "pull_request", repository: "gyldlab/keld",
  base_ref: "main", pr_number, head_sha, checked_out_sha, workflow_path,
  workflow_sha256, run_id, run_attempt, job }
```

### Nested receipt types and empty values

The following are closed records; all listed fields are required. `string` means a
nonempty UTF-8 string; `sha256` is 64 lowercase hexadecimal characters, `digest` is
`sha256:` followed by that hash, `commit` is 40 lowercase hexadecimal characters,
`timestamp` is UTC `YYYY-MM-DDTHH:MM:SSZ`, and IDs/counts/exit codes are integers
(counts and IDs nonnegative). Paths are normalized absolute host paths or relative
bundle paths as named below; relative paths reject `..`, backslashes and symlinks.
Versions are complete Debian version strings where package versions are named.
Booleans are JSON booleans. No implicit defaults, extra properties or arbitrary maps
are permitted. `failed_predicate` is a nonempty public diagnostic string; codes are
exactly the eight codes in the error table, not the literal `00N` placeholder.

```text
package_version = { name: string, architecture: "amd64" | "all", version: string }
plan = { before_dpkg_sha256: sha256, actions: package_action[] }
package_action = { name: string, architecture: "amd64" | "all",
  before_version: null | string, after_version: string,
  operation: "install" | "upgrade" | "keep", essential: boolean,
  deb_sha256: null | sha256 }
toolchain = { target: "x86_64-linux-gnu", tools: tool_identity[], probes: probe[] }
tool_identity = { name: "cc" | "c++" | "ld" | "ar" | "make",
  path: string, sha256: sha256, version_output: string, target: null | string }
probe = { name: "c11" | "cxx17" | "archive" | "make", exit_code: integer,
  output_sha256: sha256 }
attestation_evidence = { subject_digest: digest, bundle_sha256: sha256,
  issuer: string, repository: string, workflow_path: string,
  source_ref: string, source_commit: commit, signer_digest: commit,
  predicate_type: string, predicate_sha256: sha256, verified: boolean }
reference = { ref: string, commit: commit, path: string, digest: digest }
security_index = { source_uri: string, suite: "noble-security",
  component: "main" | "universe", architecture: "amd64",
  inrelease_sha256: sha256, packages_sha256: sha256, sources_sha256: sha256 }
stale_source = { name: string, locked_version: string, current_version: string,
  index_identity: string, reason: "newer-version" | "changed-source-stanza"
    | "changed-binary-stanza" | "changed-package-file" }
workflow_run = { repository: "gyldlab/keld", workflow_path: string,
  workflow_sha256: sha256, source_commit: commit, run_id: integer,
  run_attempt: integer, event_name: "schedule" | "workflow_dispatch" }
```

`installed_roots` and `dependency_versions` contain `package_version` records sorted
uniquely by `(name, architecture)`; their versions must match the actual post-install
package database. `plan.actions` has the same key. `install` requires
`before_version=null`, an absent package in the preflight package database, the
locked `after_version`, and the locked `.deb` hash. `upgrade` requires a non-null
`before_version` matching that database, a locked `after_version` strictly greater
under Debian version ordering, and the locked `.deb` hash. `keep` requires non-null
before/after versions equal under Debian version ordering, matching the installed
version, and null `deb_sha256`. An equal-version upgrade, downgrade, reinstall
labelled install, missing before-version for upgrade/keep, or inconsistent hash
is rejected with `KELD-CIIMG-004` before host mutation.
`network_sources` is a sorted unique string array of enabled apt network-source URIs
and must be empty for a passed host receipt. Tool identities sort by name and cover
exactly the five commands; compiler targets are required, others null. Probe names
sort uniquely and cover all four checks with exit_code zero on pass.
`provenance` and `sbom` each use `attestation_evidence`, with the respective expected
SLSA/SPDX predicate and all identity fields checked against `subjects.json`.
Every passed subject-verification or qualification receipt requires both records
to contain `verified: true`. Populated identity fields alone do not establish
verification. `verified: false` in either record contradicts a passed result and
must be rejected before qualification or promotion.
`references` contains `reference` records sorted by `(ref, commit, path, digest)`;
visibility is `"public" | "private" | "internal"`, linked_repository is a string,
and anonymous_pull is boolean. `security_indexes` and `stale_sources` use the above
records with the previously specified sort keys. Index identity names exactly one
signed security index; returned source names must belong to the locked source closure.
`qualification.job` is `"ubuntu-check" | "linux-gui-smoke"`; its IDs are positive
integers and its hashes/commits/timestamps use the scalar types above. Transition
`from`/`to` are `"candidate" | "current" | "previous" | "retired" | "denied"`;
legality is separately checked against the lifecycle, including orthogonal denial.

For every schema, a passed result requires all applicable evidence populated;
nonapplicable records/scalars are null and arrays are empty. Host preflight uses
empty installed/dependency arrays and null preflight hash; install requires a non-null
hash. A rejected result may set unestablished evidence to null/empty, never claim a
fabricated successful identity; applicable populated records still undergo full type
validation. Freshness passed/stale requires non-null lock/subject/run identity and
security indexes; stale additionally requires nonempty stale_sources and code 008.
Ordinary verification has null qualification; candidate-qualification requires it.
A non-promotion transition has null freshness hash; promotion requires that hash.
T2 must encode these per-action success/rejection constraints in its schemas before
implementation receipts can satisfy any acceptance row.

Receipt schemas are closed discriminated unions: unknown actions/fields fail, and fields
not meaningful to an action must be empty. `host-preflight` requires empty installed-root and
dependency-version arrays and a null `preflight_receipt_sha256`; `host-install` requires them and a
passed same-lock/same-host preflight receipt, then re-evaluates the plan. Subject census
has no verification mode and requires visibility, linked repository, anonymous pull and
references. Subject verify requires provenance/SBOM and a verification mode. Candidate
publication is a non-consumer T3b mode that requires the fully published digest to equal
`subjects.json.candidate`; it records attestation evidence but cannot authorize
pull/install or satisfy T4a. Preferred rollback requires the requested digest to equal
the checked-in pointer and a non-denied, available, fully attested `previous[]` entry
whose `retain_until` has not passed. Candidate qualification additionally
requires the digest to equal `subjects.json.candidate`, the GitHub event fields above,
and equality among event head, checked-out HEAD, workflow blob and hosted run identity;
T3c re-derives the run/head/jobs through GitHub rather than trusting the receipt alone.
The transition command owns state legality; `subjects.json` owns state; the reviewed
state-change PR plus its hosted CI artifact and Linear record retain the receipt.
Candidate-to-current promotion requires exactly one qualification receipt from each
named consumer job plus successful `clippy + test (ubuntu-24.04)`,
`Linux GUI smoke test (Xvfb)`, and `CI required` entries in one non-null `github_run`.
T3c re-derives all check-run IDs, conclusions and head/workflow identities through
GitHub. `github_run=null` is allowed only on a rejected local/unit transition.

Every command that fully reads its inputs and reaches a semantic decision atomically
writes one passed/rejected/stale receipt. Rejected receipts expose only input hashes,
public identity and the named predicate/code. Bad CLI invocation, unreadable bytes,
receipt-output failure, interruption or execution that reaches no decision leaves no
receipt. A stale freshness receipt exits nonzero with `KELD-CIIMG-008`.

| Code | Owner and fix |
|---|---|
| `KELD-CIIMG-001` | malformed/non-canonical lock; regenerate from the reviewed recipe |
| `KELD-CIIMG-002` | snapshot/key/index/package trust mismatch; refresh the lock from approved sources |
| `KELD-CIIMG-003` | bundle/SBOM/OCI digest mismatch; discard the artifact and rebuild |
| `KELD-CIIMG-004` | host plan would downgrade/remove/replace Essential or cannot resolve offline; rotate for the recorded runner or stop |
| `KELD-CIIMG-005` | visibility/linkage/attestation trust mismatch; repair the protected publication record |
| `KELD-CIIMG-006` | subject unavailable, unknown or denied; adopt current or the reviewed preferred rollback digest |
| `KELD-CIIMG-007` | unsupported host/Docker/compiler prerequisite; use the declared runner or fix the environment |
| `KELD-CIIMG-008` | published security source/index metadata is newer or changed from the lock; publish reviewed rotation within 72 hours |

### Reuse and rejected alternatives

- Preserve the router, selected Keld commands, host AppArmor sysctl, strict tests,
  GUI-smoke scripts and required-result evaluator. KEL-82 owns acquisition only.
- The OCI subject is a standard single-layer image containing `/bundle/repository/`
  (local apt repository), `/bundle/packages/`, and the lock-derived SPDX 2.3 JSON.
  Tar entries are byte-sorted with uid/gid/mtime zero, gzip omits name/time, and OCI
  config uses fixed platform/history timestamps. The digest-pinned resolver image owns
  all packer tool versions.
- Consumers use `runs-on: ubuntu-24.04`. They pull anonymously into an empty Docker
  config, extract to a new directory, verify bytes/attestations, then use an offline apt
  configuration whose only source is the extracted local repository. That recomposed
  repository is intentionally unsigned and is marked `Trusted: yes` only after the OCI
  subject, lock, index and every `.deb` hash pass; the outer verified subject is its
  trust owner and no Keld archive-signing key is invented. Network package sources are
  absent. The solver and install forbid downgrades/removals; roots
  must equal the lock, while already-installed dependencies may satisfy the locked
  Debian relations. The receipt records their actual versions and runner image.
- The Ubuntu `check` bundle step runs for every `rust=true` row, including workflow,
  router, evaluator, lock and verifier diffs. This costs a pull/install on non-GTK Rust
  closures but makes the consumer observable and removes the conditional blind spot.
- A job container was rejected: its different shell and nested Bubblewrap/userns
  boundary are unproven. Installing an exact full Noble closure over the runner was
  rejected because weekly host updates can require unsafe libc/system downgrades.
- SPDX is generated directly and deterministically from the verified package lock; no
  floating SBOM scanner is introduced. Publication creates separate SLSA provenance
  and SPDX (`https://spdx.dev/Document/v2.3`) attestations for the same subject.
- Consumer verification invokes `gh attestation verify` separately for both predicate
  types and supplies exact repo, signer workflow, source ref/digest, signer digest,
  OIDC issuer and `--deny-self-hosted-runners`; the receipts must contain the locked
  subject. An authenticated read may fetch attestations, but the image pull itself is
  separately proven anonymous.

### Registry bootstrap, lifecycle and freshness

T3 is two-phase. T3a review commits the deterministic expected candidate digest and a
main-push-only publisher. After merge, the protected workflow must reproduce that exact
digest or fail without publishing another subject. The package administrator then sets
visibility to public and confirms repository association. T3b is a second PR that
records the source/run/attestation/visibility/anonymous-pull receipts but keeps the
subject candidate.

Bootstrap qualification is explicit. A draft T4a consumer PR may select only the
locked candidate under a `pull_request`-only qualification mode; it cannot merge and
main consumers still reject candidates. Its exact head/run proves the Ubuntu check,
strict and GUI consumers. T3c then records that immutable evidence, fills
`qualified_at`, and promotes the candidate to current. The T4 PR rebases onto T3c,
removes qualification mode, verifies current-only consumption on its new exact head,
and only then may merge as T4b. No ordinary main/PR path accepts candidate state.

Main is the only supported ref in this pre-alpha repository. A release-branch policy
must add refs to `subjects.json` before creating them. The verifier checks `denied`
before any registry request. Rotation retains every previous digest until its individual
`retain_until`; a preferred rollback pointer may select one non-denied previous entry
without removing the others. Consumers select current by default; T5 may use only the
explicit preferred-rollback mode. Reviewed transition PRs may set/change/clear the
pointer and record that orthogonal change. Clearing it restores current selection;
rollback never moves a previous entry backward through the lifecycle. Denial is permanent. GitHub-hosted
attestations for a denied digest are deleted and verified absent as cleanup, but a
later attestation cannot override deny-before-network enforcement. Physical registry
deletion is best effort and never the trust oracle.

The single read-only freshness workflow owns all current `noble-security`
metadata acquisition. It runs daily and via `workflow_dispatch` before promotion,
with `contents: read` and explicit `packages: none`, `attestations: none`, and
`id-token: none`; it cannot call or dispatch the publisher. It authenticates the
InRelease → Packages/Sources hash chain, then compares only source/binary stanzas and
file hashes reachable from the locked source closure using Debian version ordering.
Unrelated index updates remain green. A relevant newer version or same-version
stanza/binary/file change emits stale with code 008. It downloads no package payloads.

Promotion consumes that workflow's immutable artifact and records its SHA-256 in
`freshness_receipt_sha256`; it does not fetch security indexes itself. T3c checks via
the GitHub API that the receipt came from the protected main freshness workflow at
the reviewed workflow hash/source commit, matching run ID/attempt and a successful
job. It checks exact lock hash and candidate digest, and requires passed status and
`0 <= promotion_time - checked_at <= 24h`. It rejects a future, expired, stale,
rejected, missing or mismatched receipt; rechecking means invoking the same freshness
workflow, never introducing another reader. The workflow's output includes the signed
metadata bytes and their hash chain for offline provenance verification by promotion.
The KEL coordinator uses
the receipt to open or update one `keld.linux-ci-rotation/v1` record:

```text
first_detected_at, deadline_at = first_detected_at + 72h, last_checked_at,
affected_sources[], response = null | { at, owner, blocker },
resolution = null | { at, replacement_digest }
```

Daily updates preserve `first_detected_at` and `deadline_at`. A response at or before
the deadline satisfies the response SLA, but blocker evidence leaves freshness red.
With no response, `now >= deadline_at` is overdue. Only a qualified replacement resolves
the record; a later distinct update starts a new record. No automated package
substitution or silent waiver is permitted.

## 5. Boundaries

- T2: `ci/linux-webkitgtk/` recipe, schemas, lock generator/verifier and tests only.
- T3a/T3b: protected publisher workflow, `subjects.json` candidate declaration,
  publication-field completion, visibility/linkage/attestation receipts and no
  candidate promotion.
- T4a/T3c/T4b: candidate-only draft qualification evidence, `subjects.json` promotion,
  current-only consumer integration and its exact hosted receipts.
- T4a/T4b: `.github/workflows/ci.yml` changes the `check` matrix label and GUI `runs-on` from
  `ubuntu-latest` to `ubuntu-24.04`; updates every Ubuntu comparison that owns fuzz,
  verifier/install, AppArmor preparation, package selection, nextest/Xvfb, KEL-167 and
  rustdoc; and changes the two dependency-acquisition steps. It also updates the owning
  router/evaluator/hygiene tests and `.agents/ci.md` with instruction-budget,
  semantic-eval and prompt-trace evidence. No stale `ubuntu-latest` may remain inside
  the final `check` or `linux-gui-smoke` blocks; unrelated generic jobs may retain it.
- T3a also introduces the single read-only freshness workflow needed by T3c.
- T5: rotation/retirement/denial subject transitions, daily freshness exercise,
  retention/cleanup logic and consumer/rotation documentation.
- Must not touch product crates, Cargo dependencies, macOS/Windows execution, MSRV
  placement, Bun/package-manager pins, wire protocol, permissions, release/update
  artifacts, or test deadlines/assertions.
- Regenerate `llms-full.txt` only if a changed source is on `tools/llms_docs.rs`'s
  explicit allowlist; neither this spec nor `.agents/ci.md` currently is.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

- [ ] **T1 — approve this contract:** human review accepts the resolved registry,
  trust, SBOM, host-install, lifecycle and freshness rules; then mark the spec approved.
- [ ] **T2 — deterministic source and host preflight:** add `ci/linux-webkitgtk/`,
  schemas, snapshot resolver, canonical lock/SPDX/OCI builder, verifier and negative
  controls. Prove two clean builds and host preflight locally. Do not publish or edit CI.
- [ ] **T3a — protected first publication:** after CI writers release, add the
  main-only publisher workflow, separate read-only freshness workflow and predicted
  candidate digest. Merge triggers
  an exact build, SLSA/SPDX attestations and private first publication; any digest
  mismatch stops.
- [ ] **T3b — public publication record:** package admin makes the subject public and repository
  linked. Verify API state, anonymous digest pull, both attestations and SBOM equality;
  record publication fields/receipts in a follow-up PR, but keep candidate state.
- [ ] **T4a — candidate qualification PR:** implement the offline consumers in a draft
  PR whose pull-request-only qualification mode accepts only the locked candidate.
  Run exact hosted Ubuntu check, strict and GUI rows; do not merge.
- [ ] **T3c — promote the qualified subject:** record the exact T4a head/run evidence,
  require the bound passing freshness receipt from T3a's workflow (dispatch it
  after candidate publication), fill `qualified_at`, move candidate to current and
  merge the subject-state PR.
- [ ] **T4b — current-only consumers:** rebase T4a on T3c; remove candidate mode; make
  the Ubuntu `check` verifier/install unconditional
  for `rust=true`; move its matrix value, all Ubuntu conditions and GUI runner to
  `ubuntu-24.04`; replace GUI live apt; update applicability/result admission and every
  owning CI/instruction test; preserve every existing Keld/strict/GUI command; rerun
  exact hosted checks against current state, then merge.
- [ ] **T5 — rotation, freshness and removal:** run one
  candidate/current/previous/retired cycle,
  one denied-but-still-pullable cycle, attestation deletion, 30-day/reference census
  enforcement, daily runs of the existing freshness workflow, 72-hour coordinator-record
  boundaries, unavailable/corrupt/skip/retry fallback mutations, preferred rollback
  set/use/clear, cold timing receipts and best-effort GHCR cleanup.

## 7. Test plan

| AC | Owner, test and independent mutation |
|---|---|
| 1 | CI census unit: package publisher=1, metadata-only freshness reader=1, offline consumers=2, MSRV/other acquisition=0; add a second package download or third install site and fail |
| 2 | Lock/schema unit: canonical parse plus exact roots, source/binary versions, source URI/snapshot_id/timestamp agreement/pocket/key/InRelease/Packages/Sources/closure/file hashes; independently alter order, duplicate keys, and the binary `(source_name, source_version, source_index_identity)` edge |
| 3 | Reproducibility integration: generate SPDX only from the named input projection, then manifest/payload/OCI; two clean builds compare every byte/descriptor; enable live source or mutate timestamp/tool/arch/file metadata |
| 4 | Host-preflight subprocess on two recorded `ubuntu-24.04` image revisions: sources disabled, no mutation sentinel; inject downgrade, removal, Essential replacement, missing deb and conflict; remove or spoof each compiler/linker command, target, version, probe and receipt field; assert no package mutation |
| 5 | Host-install integration: local-only repository network trace, exact-root receipt, `dpkg --audit` and offline dependency result; allow network or change one root/version and fail before compile sentinel |
| 6 | Existing Linux strict process tests plus executable package/hash receipt; remove sysctl/bwrap, mutate path, or run default nested container and require the named failure |
| 7 | Router/workflow/evaluator contract: every owned path runs both consumers as applicable and `CI required` observes result; assert the check matrix, every Ubuntu-specific condition and GUI runner use `ubuntu-24.04`; force gui=false for each bundle/workflow/router/evaluator input and fail; remove/invert each step/result edge and reject a stale consumer-side `ubuntu-latest` |
| 8 | Existing media interposer + `linux_gui_smoke.sh` title/control/cleanup process oracle; delete each existing command/oracle and fail |
| 9 | Attestation conformance: verify SLSA and SPDX separately with exact issuer/repo/workflow/ref/source/signer/subject, then hash the signer commit's workflow blob against `publisher_workflow_sha256`; one wrong-field mutation per predicate |
| 10 | Post-publish hosted check: package API visibility, repository association and empty-config anonymous digest pull are three separate assertions; private/unlinked/authenticated-only controls fail |
| 11 | Required-result failure matrix: tag, unknown/unavailable/corrupt/unlinked/unattested/denied/cached subject, candidate-publication mode used by a consumer, candidate outside T4a or with spoofed repo/base/PR/head/checkout/workflow/run/attempt/job, arbitrary/unpointed/expired rollback, network source, retry, `continue-on-error` and selected skip each fail before mutation/compile; T3c re-derives the passing run through GitHub |
| 12 | State-machine/transition-receipt unit plus Git/reference integration: unavailable candidate fields, null publication field promoted current, missing/duplicate consumer qualification receipt, mismatched GitHub run/check identity, two promotions inside 30 days retaining both previous entries, previous dropped without retired record/receipt, invalid transition, preferred rollback set/use/clear with the same hosted consumers, denied/expired/retired/unavailable pointer rejection, denied-before-pull even with a new valid attestation, non-main ref, reference remaining, early retirement, attestation cleanup and >5k/no-registry-delete paths |
| 13 | Single freshness workflow plus offline promotion verification: reject second metadata reader, stale-as-shared-result, malformed nested records, unknown fields, wrong lock/digest/workflow/run, missing signature bytes, future/expired receipt; authenticate whole current security indexes, compare only reachable source/binary stanzas and file hashes, keep an unrelated-index-update control green, and emit a stale receipt plus `KELD-CIIMG-008`; injected-clock records cover T+71:59:59, exact 72h with/without response, T+72:00:01, repeat without reset, blocker-stays-red, replacement resolution and later-new-update origin |

Tests use file/process/network conditions rather than sleeps. Local Docker proves bundle
and preflight behavior; only hosted Ubuntu proves runner identity, workflow permissions,
Bubblewrap/AppArmor and GUI execution. No local result is reported as hosted proof.

## 8. Review gates triggered

- unsafe: none
- public API: none
- permission model: none
- dependency addition: yes — independent supply-chain review of resolver image, Ubuntu
  sources/keyring/package closure, OCI subject, SBOM and attestation tools
- wire protocol: none
- CI/security: independent review for T3–T5, including publisher permissions and
  required-result applicability
- agent instructions: instruction review plus `just agent-context`, representative
  semantic/static mutations and prompt traces when `.agents/ci.md` changes in T4b

## 9. Perf impact

No application performance impact. Record cold OCI pull, verification, solver preflight
and local install separately from compilation/test time. Compare with current live-apt
receipts only after semantic equivalence. Do not hide a regression with cache, timeout,
retry or an application-performance claim.

## 10. Open questions

None. The formerly open registry, attestation, image-shape, retention and signer choices
are resolved above. T2 host compatibility and T4a/T4b hosted Bubblewrap are falsifiable
acceptance gates; a failure stops KEL-82 for spec revision and does not authorize a
fallback or widened privilege.
