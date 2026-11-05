# Spec: static migration analysis and binding-aware scan rules

- Status: draft
- Tracker: GH-590
- Owner: Monish Khandelwal (@0monish; approval pending)
- Updated: 2026-11-05
- Pin: 0eb3ce203187f35c5276a4237189c906a5220d5a

## 1. Goal & non-goals

Define a bounded, read-only analyzer for future `keld migrate` implementation.
The analyzer inventories source uses without executing application code.
It separates compatibility, security, engine, and native-dependency findings.
It preserves unresolved inputs instead of reporting false support.

FACT: `migrate` remains reserved at `crates/keld-cli/src/verb.rs:38`.
This document adds no working command or compatibility result.

Non-goals:

- Implementing the analyzer, generator, installer, or recorder in this change.
- Changing #497 output ownership, #502 module resolution, or #593 launch admission.
- Minting permissions from source text or a static window name.
- Executing application code, configuration, package scripts, or dependency installation.
- Converting a static estimate into a KEL-74 product score.
- Pulling implementation ahead of the program's first-proof gates.

## 2. Spec refs

FACT: the following sources define the current boundary.

| Owner | Governing source | Consumed rule |
|---|---|---|
| CLI | `crates/keld-cli/src/verb.rs:74` | Reserved migrate status and remedy |
| Guard | `crates/keld-guard/src/jsonc.rs:1` | One comment stripper, not a general JSONC parser |
| CLI report | `crates/keld-cli/src/doctor.rs:68` | Existing `DoctorFinding` shape |
| Native evidence | `docs/architecture/04-electron-compat.md:179` | Exact addon/runtime/artifact qualification |
| Output contract | #1172 `docs/specs/gh497-first-proof-migration-output.md:77@b7816c4c2ba2a53c4800904601b7e2b5a703a34c` | Owned writes and explicit omissions |
| Source analysis | #1172 `docs/specs/gh497-first-proof-migration-output.md:85@b7816c4c2ba2a53c4800904601b7e2b5a703a34c` | No app, config, resolver, or install execution |
| Identity | #1172 `docs/specs/gh497-first-proof-migration-output.md:87@b7816c4c2ba2a53c4800904601b7e2b5a703a34c` | Dynamic windows/channels remain unresolved |
| Report | #1172 `docs/specs/gh497-first-proof-migration-output.md:97@b7816c4c2ba2a53c4800904601b7e2b5a703a34c` | Counts, shared diagnostics, no default product claim |
| Entity inventory | #1278 `docs/engineering/electron-entity-status.tsv:2@578bbac9ccb2daa48783054752d5a09eba5c28ef` | Entity inventory is not a member verdict |

FACT: [#590](https://github.com/gyldlab/keld/issues/590) owns these scan rules.
[#498](https://github.com/gyldlab/keld/issues/498) owns the first analyzer slice.
[#592](https://github.com/gyldlab/keld/issues/592) owns later blocker classes.
[#591](https://github.com/gyldlab/keld/issues/591) retains reversible generation.
KEL-127 retains sequencing.
The proposed analyzer preserves the architecture's target direction.
#497 retains the pending architecture errata and report contract.

## 3. Acceptance criteria

### AC-1: analysis executes no project input

- **Given:** a project contains config and import traps that create an external fixture marker.
- **When:** CLI or MCP analysis examines every admitted input.
- **Then:** no trap runs, no child starts, and the project digest stays unchanged.
- **Negative control:**
  - **Single fault:** evaluate the fixture's JavaScript config in the loader.
  - **At-risk precondition:** discovery reaches that config and its sentinel is writable.
  - **Expected RED:** the independent sentinel observer records execution.

### AC-2: input identity and bounds remain explicit

- **Given:** files include an outside-root link, ambiguous spelling, changed bytes, and a limit-crossing input.
- **When:** discovery freezes the scan input set.
- **Then:** the report names each excluded or unstable input and refuses complete-scan qualification.
- **Negative control:**
  - **Single fault:** omit the outside-root exclusion from the result.
  - **At-risk precondition:** an enumerated source points outside the admitted root.
  - **Expected RED:** the frozen-input oracle contains a missing disposition.

### AC-3: import identity follows bindings

- **Given:** fixtures contain renamed Electron imports, shadowed names, comments, strings, and unrelated objects.
- **When:** the scanner inventories calls.
- **Then:** its source-span list exactly matches the hand-resolved binding oracle.
- **Negative control:**
  - **Single fault:** match receiver text without resolving the lexical binding.
  - **At-risk precondition:** an inner parameter shadows a genuine imported receiver.
  - **Expected RED:** the oracle detects an extra Electron call site.

### AC-4: preload exports retain their provenance

- **Given:** a preload exposes a literal bridge through local re-exports and one cyclic module edge.
- **When:** analysis traces the renderer source call to that bridge.
- **Then:** the report records the originating IPC operation and each inspected edge once.
- **Negative control:**
  - **Single fault:** stop following a renamed static re-export.
  - **At-risk precondition:** the renderer uses only that exported name.
  - **Expected RED:** the expected bridge call site is absent.

### AC-5: uncertain syntax never becomes a negative finding

- **Given:** inputs contain parser errors, dynamic imports, computed members, and unsupported configuration syntax.
- **When:** analysis cannot prove a binding or value.
- **Then:** it emits the applicable unclassified or manual finding with its source location and next action.
- **Negative control:**
  - **Single fault:** classify a recovered parse as complete despite its diagnostic.
  - **At-risk precondition:** the recovery crosses an Electron call expression.
  - **Expected RED:** the fixture receives an unsupported complete-scan verdict.

### AC-6: one parser owns JSONC comments

- **Given:** JSONC contains comments, comment-like strings, and trailing commas.
- **When:** configuration analysis runs.
- **Then:** the shared guard stripper preserves strings and positions.
  Unsupported trailing commas remain manual unless the shared owner separately approves support.
- **Negative control:**
  - **Single fault:** remove comment-like bytes from a quoted URL.
  - **At-risk precondition:** the URL contains the same characters as a real line comment.
  - **Expected RED:** the independently expected string value changes.

### AC-7: classification requires matching evidence

- **Given:** a member row has a different OS, runtime, semantic case, or source digest.
- **When:** classification queries the approved matrix.
- **Then:** the call site stays unclassified for that unmatched dimension.
- **Negative control:**
  - **Single fault:** accept a row using only the member name.
  - **At-risk precondition:** a same-name row exists for a different platform.
  - **Expected RED:** the report claims supported behavior without a matching row.

### AC-8: blocker classes stay independent

- **Given:** one project exercises every D-5 rule and several rules overlap.
- **When:** findings are rendered.
- **Then:** compatibility, security, engine, and native-dependency sections retain their separate findings.
- **Negative control:**
  - **Single fault:** discard security findings after finding a compatibility blocker.
  - **At-risk precondition:** one call has both unsupported semantics and unsafe options.
  - **Expected RED:** the security section lacks its expected finding.

### AC-9: static IPC observations mint no grants

- **Given:** a literal sender has only a method-incompatible registration and another sender is computed.
- **When:** analysis records channel candidates.
- **Then:** neither produces a grant proposal.
  The report retains both source locations and the missing or incompatible registration.
- **Negative control:**
  - **Single fault:** join callers and registrations by channel name alone.
  - **At-risk precondition:** `send('danger')` meets only `handle('danger')`.
  - **Expected RED:** the candidate list contains an unauthorized match.

### AC-10: reports preserve honesty and deterministic order

- **Given:** the same frozen inputs and evidence source run through CLI and MCP.
- **When:** both adapters render analysis.
- **Then:** normalized findings, classifications, exclusions, and counts match exactly.
  Both say `static estimate, not a compatibility score` and print no percentage.
- **Negative control:**
  - **Single fault:** count an unclassified call as supported.
  - **At-risk precondition:** the input has one unresolved binding and one proven member.
  - **Expected RED:** the exact count oracle differs.

## 4. Design

### D-1: decision atoms and ownership

The following table is the proposed working model.
Each atom has a separate falsifier.
An unresolved atom cannot borrow another atom's result.

| Atom | Owner and boundary | Input to output | Failure and falsifier | Dependency |
|---|---|---|---|---|
| Input freeze | CLI read-only discovery | Root and bytes to census | Hidden omission, AC-2 | None |
| No execution | CLI input readers | Bytes to data | Config side effect, AC-1 | Input freeze |
| Syntax identity | Approved parser and CLI scanner | Syntax to source spans | Shadow false positive, AC-3 | Frozen bytes |
| Bridge graph | CLI static traversal | Local export edges to provenance | Lost rename/cycle, AC-4 | Syntax identity |
| Evidence | Compatibility owners | Exact member rows to status | Wrong-platform promotion, AC-7 | Frozen matrix |
| Report | #497 and CLI adapters | Findings to shared output | Lost class or count, AC-8/AC-10 | Earlier atoms |

No runtime process, principal, handle, permission, or crash owner changes.
Analysis does not launch Bun or Electron.
Analysis creates no persistent application state.

### D-2: bounded discovery and frozen bytes

The proposed discovery scope is the selected project root.
Enumerate regular source and data files in deterministic relative-path order.
Record the exclusions before scanning.
Do not follow symlinks, junctions, device nodes, or other reparse targets.
Report such inputs as manual when they could affect analysis.

Read `package.json`, admitted lockfiles, `tsconfig.json`, and `jsconfig.json` as data.
Discover relevant inherited config references without executing a resolver.
An outside-root or package-resolved `extends` needs explicit input admission.
Record unresolved inheritance and wildcard mappings.
Do not claim #497 runtime qualification from these observations.

Initial proposed limits are 16,384 files and 128 MiB of source bytes.
Each input is at most 8 MiB.
The import graph holds at most 65,536 edges.
Traversal records at most 256 hops per provenance chain.
Findings stop at 65,536 entries with an explicit incomplete-analysis failure.
These are admission limits, not measured performance claims.
Test zero, exact limit, and limit plus one for each bound.

Capture each accepted file through a retained no-follow handle into the bounded input set.
Reject a path component or file kind that cannot satisfy that platform's no-follow policy.
Record path, identity, metadata, digest, byte count, source kind, and capture interval.
Parse and report only those retained bytes.
Repeat metadata and byte-digest validation through the handle and revalidate its named identity.
Any observed instability or unreadable input makes the scan incomplete.
These checks provide per-file observations, not an atomic whole-project snapshot.
Unobserved changes or bytes that change and return cannot be excluded by repeated checks.
Record that limit and the full scan interval in the report. Never label retained evidence as current live state.

Exclude `.git`, generated Keld stages, output folders, and dependency trees by an explicit census rule.
Record every excluded directory and why.
Do not claim analysis of excluded dependencies.
A discovered import into an excluded tree remains an unresolved dependency edge.
The analyzer does not install or recursively execute packages.

### D-3: parser and binding decision

FACT: the existing JSONC helper replaces comments while retaining positions.
It explicitly lacks trailing-comma support at `crates/keld-guard/src/jsonc.rs:7`.
Expose that implementation through its owning crate.
Do not copy it into the CLI.

Use strict JSON after comment removal for the initial JSONC subset.
Report unsupported syntax as `manual (unparsed)`.
The loader must not repair malformed text silently.
YAML remains `manual (unparsed)` in the initial slice.
Adding YAML support requires a separate parser dependency decision.

Proposed JavaScript choice: a matching, pinned Oxc parser and semantic-analysis release.
This choice remains blocked by B-2 and Q-1.
A lexer or regular expression cannot resolve shadowing and export identities reliably.
The small existing literal-config scanner is not a JavaScript binding engine.

FACT: Oxc describes "Build complete symbol tables with binding information".
Source: [Oxc semantic README](https://github.com/oxc-project/oxc/blob/5c25203d70f31b8e5f26fbebb48ccced400e60ec/crates/oxc_semantic/README.md), retrieved 2026-11-05.
That description motivates evaluation.
It does not qualify a dependency version or prove Keld behavior.

Parse supported JavaScript and TypeScript syntax without evaluation.
Reject recovered AST regions that have syntax or semantic diagnostics.
Recognize static ESM imports and literal CommonJS requires.
Follow immutable local aliases using resolved bindings.
Treat mutation, reflection, computed lookup, and ambiguous exports as unclassified.
Track main, preload, and renderer contexts separately.
Do not infer a runtime principal from those static contexts.

Follow only uniquely resolved local source edges.
An unresolved extension, conditional export, alias, or package mapping stays explicit.
The scanner is not a second runtime module resolver.
Use a visited set keyed by file, binding, and source context.
Consume graph bounds before adding edges.

### D-4: one classification source

FACT: #1278 labels its rows as entity inventory, not member verdicts.
Its presence cannot classify every method of `BrowserWindow` as supported.

Proposed canonical form: one compatibility-owned, committed UTF-8 TSV.
Proposed path: `crates/keld-compat/fixtures/electron-api-classification.tsv`.
The compatibility owners must approve that location under B-3.
Migrate consumes it directly and creates no private copy.
Any existing authoritative member table must move or generate this file through its single owner.
Do not establish two manually maintained tables.

Each row identifies a member, context, platform, semantic case, runtime, status, owner, and immutable evidence.
Use only supported, behavior-note, unsupported, and unclassified classifications.
A supported row needs matching conformance evidence.
A source export or approved spec alone cannot supply that result.
A behavior note names the exact divergence.
Missing evidence yields unclassified.

Limit this file to 16,384 rows and 4 MiB.
Record actual row count and byte digest at implementation.
Do not hard-code the historical matrix count as a completeness test.
Validate unique row keys, closed statuses, owner references, and evidence references.
The evidence owners retain KEL-74 schemas and qualification decisions.

### D-5: scan-rule and fixture contract

The rule table is proposed future behavior.
Each named fixture needs committed input and an independently authored expected result.
Expected lists must not come from the scanner under test.

| Rule and fixture | Input | Required disposition | Class |
|---|---|---|---|
| `import-alias-shadow` | Aliased import plus shadow parameter | Count only resolved Electron receiver | Compatibility |
| `preload-reexport-cycle` | Literal bridge, renamed exports, cycle | Record provenance or bounded unresolved edge | Compatibility |
| `computed-channel` | Computed send or registration | Unenumerable, zero grant proposal | Security |
| `method-channel-mismatch` | Send with handle-only registration | No method-compatible candidate | Security |
| `webprefs-defaults` | Missing and explicit five security fields | Compare exact pinned defaults separately | Security |
| `webview-false` | `webviewTag:false` and attachment prevention | No positive webview-use claim | Security |
| `unsafe-webprefs` | Disabled isolation or web security | Blocker, never an allowed generated setting | Security |
| `engine-scheme` | Custom scheme without proven privilege registration | Engine blocker and source locations | Engine |
| `addon-version-mismatch` | Native module with unmatched binary/runtime | Unqualified, never package-name support | Native-dependency |
| `spawn-variable-shell` | Variable program or `shell:true` | Never infer a grant from the expression | Security |
| `external-url-dynamic` | Dynamic `shell.openExternal` URL | Unresolved destination, manual decision | Security |
| `updater-provider-literal` | Closed literal provider object | Build-time mappable observation, no feed authority | Compatibility |
| `updater-feed-runtime` | Runtime-derived update feed | Never translate into privileged updater input | Security |
| `builder-yaml` | YAML config without an approved parser | Manual, unparsed, no inferred clean result | Compatibility |
| `forge-eval-trap` | Config calls or hooks with side effects | Inspect syntax only, never invoke | Compatibility |
| `jsonc-preserve` | Comments, quoted URLs, trailing commas | Shared stripping or explicit manual result | Compatibility |
| `config-inheritance` | External extends or Electron wildcard mapping | Unresolved qualification input, no overriding write | Compatibility |

The five security fields are `nodeIntegration`, `contextIsolation`, `sandbox`, `webviewTag`, and `webSecurity`.
#498 first records their pinned Electron defaults.
Do not substitute remembered defaults or another Electron version.

An updater literal is only a classification.
#497 still excludes generated build configuration and feed grants.
Native evidence matches the entire architecture 04 section 5 tuple.
A missing artifact digest does not become a match.

### D-6: adapters and authority

Proposed internal seam:

```rust
fn analyze(inputs: &FrozenInputs, evidence: &ClassificationSource)
    -> Result<Analysis, AnalysisError>;
```

These names describe future CLI-private types.
They add no package API or wire contract.
Both CLI and MCP consume one analysis result.
Reuse `DoctorFinding` and `KeldErrorObject` under #497.
Allocate any new stable diagnostic codes through the existing registry owner.

Snapshot caller options before discovery.
No hidden environment or cwd change may alter later classification.
Store no scan cache across changed input digests.
No temporary adapter or compatibility facade replacement is needed.
Manifest, channels, and runtime protocol changes: none.

## 5. Boundaries

Implement later in `crates/keld-cli` and the approved compatibility data owner.
Expose JSONC stripping only in `crates/keld-guard`.
Place tests with those owners under the test-layout policy.
Use #498 and #592 for implementation slices.

Do not edit #497, #502, #593, runtime grants, the corpus scorer, or live launch paths here.
Do not add a second report serializer, manifest parser, module resolver, or classification table.
Shared Cargo dependency changes require their designated writer and review.

## 6. Tasks

### T-1: freeze independent inputs and parser acceptance

- **Blocked by:** B-1, B-2, Q-1
- Add source fixtures and hand-resolved expectations before scanner behavior.
- Expose the shared JSONC helper without changing manifest acceptance.
- Verify AC-1, AC-2, AC-3, AC-5, and AC-6.
- B-2 supplies the earlier parser-only qualification. These are integrated scanner controls.

### T-2: deliver the #498 read-only vertical

- **Blocked by:** T-1, B-3
- Connect discovery, static binding scan, security defaults, and shared report rendering.
- Add CLI and MCP consumers together.
- Keep unsupported classes explicit.
- Verify AC-1, AC-2, AC-3, AC-5, AC-6, AC-7, and AC-10.

### T-3: deliver the #592 provenance and blocker vertical

- **Blocked by:** T-2
- Add bounded bridge tracing and the remaining D-5 rules.
- Consume existing addon evidence instead of a new package-name database.
- Verify AC-4, AC-7, AC-8, AC-9, and all retained criteria.

### T-4: qualify platform-neutral claims

- **Blocked by:** T-3
- Run the same frozen fixtures on each claimed OS.
- Compare complete normalized reports and resource failures.
- Produce B-4's closeout evidence before widening claims.

## 7. Test plan & evidence

| Criteria | Planned independent test | Required observation |
|---|---|---|
| AC-1 | Subprocess with config execution traps | External sentinel, child observation, unchanged project |
| AC-2 | Frozen input corpus and limit boundary set | Exact census and explicit incomplete disposition |
| AC-3/AC-4 | Hand-resolved source-span and bridge graph corpus | Exact locations, contexts, and provenance |
| AC-5/AC-6 | Malformed/config parser fixtures | No clean verdict from unparsed input |
| AC-7 | Cross-platform evidence mismatch fixtures | No unsupported status promotion |
| AC-8/AC-9 | D-5 overlapping findings and IPC matching | Separate classes and zero unsafe candidates |
| AC-10 | CLI/MCP differential over identical bytes | Equal reports and exact counts |

Use isolated temporary resources.
Use observable barriers for input replacement.
Run each single-fault control independently.
Do not reuse the mutated scanner to generate expected output.
FACT: no runtime, parser, or native test ran for this document.

### E-1: current source and tracker inspection

- **Run:** GitHub GET of the cited files, #590/#498/#592, and PR #1172/#1278.
- **n:** one pinned source inspection per cited file and one live issue snapshot.
- **Environment:** cloud source review on 2026-11-05 at the header commit.
- **Limits:** source inspection proves neither implemented migration nor successful fixture execution.

### E-2: dependency discovery receipt

- **Run:** Context7 `/oxc-project/oxc` query for Rust parsing, lexical bindings, and non-executing analysis.
- **n:** one discovery query and one immutable primary README confirmation.
- **Environment:** retrieved 2026-11-05, Oxc source `5c25203d70f31b8e5f26fbebb48ccced400e60ec`.
- **Limits:** no dependency release, license audit, parser benchmark, or Keld integration is qualified.

## 8. Review gates triggered

- Public API: CLI and MCP findings under #497.
- Dependency: proposed Oxc family and any later YAML parser.
- Permission model: verify analysis creates no grant or launch authority.
- Unsafe: none proposed.
- Wire protocol: none proposed.

These are future implementation gates.
A passing document check does not discharge them.

## 9. Perf impact

The analyzer is cold tooling.
The proposed limits bound admitted inputs, graph work, and retained findings.
No performance improvement is claimed.
Measure wall time and peak retained memory on the frozen fixture set.
Keep the simple read-only design unless measurements justify another mechanism.

## 10. Open questions, blockers & unknowns

### Q-1: select the JavaScript parser

Approve a pinned Oxc parser/semantic pair after B-2.
Alternatively, keep JavaScript analysis manual and do not claim binding-aware coverage.
Recommendation: evaluate Oxc because bindings are required, and a custom lexer would create another language implementation.

### B-1: output-contract and scheduling approval

- **Status:** open
- **Owner:** Monish Khandelwal and the #497/#496 owners.
- **Impact:** runtime implementation and output changes cannot start from this draft.
- **Check:** record exact-content approval and the first-proof scheduling decision on the owning issues.

### B-2: parser qualification

- **Status:** open
- **Owner:** CLI owner and the designated dependency reviewer.
- **Impact:** parsed JavaScript claims and T-1 remain blocked.
- **Check:** approve pinned versions, license, toolchain, and a parser-only experiment for binding identity and malformed input.
  The experiment precedes scanner implementation and records resource limits.
  T-1 subsequently proves AC-3/AC-5 through the integrated scanner. That later proof is not B-2's prerequisite.

### B-3: canonical member classification ownership

- **Status:** open
- **Owner:** #391 family owners and the KEL-74 compatibility evidence owner.
- **Impact:** supported/behavior-note classifications and T-2 remain blocked.
- **Check:** approve the single TSV producer/location and validate its first evidence-backed rows.

### B-4: actual platform qualification

- **Status:** open
- **Owner:** #498/#592 implementers and the respective OS operators.
- **Impact:** no macOS, Windows, or Linux runtime pass is established by this spec.
- **Check:** T-4 publishes exact-head CLI/MCP fixture results with each named control.
  This blocks platform claims, not execution of T-4.

### U-1: whole-application input coverage

UNKNOWN: the minimum complete source set for each migrated application.
Resolve this from frozen app sources and independently authored call-site lists.
An omitted dependency or unsupported configuration remains a reported gap.

### U-2: parser resource behavior

UNKNOWN: worst-case parser allocation and cancellation behavior for admitted hostile inputs.
Measure it during B-2.
Input-byte bounds alone do not prove a hard heap or runtime bound.

