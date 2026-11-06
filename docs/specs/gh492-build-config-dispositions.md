# Spec: complete build-config disposition without executable evaluation
Status: draft
Linear: GH-492 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; docs only, do not merge.

## 1. Goal & non-goals

Define the future translator that inventories every input field of an Electron build configuration and produces an auditable
disposition against an approved Keld build schema. A disposition means mapped, explicit divergence (▲), or unsupported/manual
(✘). Completeness is measurable even when the destination schema or target is unavailable. The user's November 6 specification
expansion authorizes this draft; GH-492 remains parked for implementation.

Non-goals: implementing or enabling `keld build`; approving the destination schema; executing build hooks, signing or publishing an app; a
sandbox for JavaScript evaluation; interpreting executable configuration by default; reproducing Electron's ASAR archive or V8
fuses; changing the first migration proof's output; the update bridge in GH-583.

## 2. Spec refs

- [GH-492 tracker and predecessor decisions](https://github.com/gyldlab/keld/issues/492)
- [Build/runtime architecture and reserved build boundary](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/06-runtime-and-tooling.md)
- [Compatibility/configuration ownership](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/04-electron-compat.md)
- [Canonical ExpectedAppIdentity](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-pack/src/expected_identity.rs)
- [Reserved CLI verbs and KELD-CLI-045](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-cli/src/verb.rs)
- [KEL-254 installed Windows root binding](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/kel254-windows-installed-root-boot.md)
- [Proposed local macOS schema, independently owned PR1407](https://github.com/gyldlab/keld/blob/1e7a32db38e5f9b43614407e7b55086f46f5bdb9/docs/specs/macos-local-app-packaging.md)
- [draw.io Windows x64 config](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/electron-builder-win.json)
- [draw.io Windows arm64 config](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/electron-builder-win-arm64.json)
- [draw.io Linux/macOS config](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/electron-builder-linux-mac.json)
- [draw.io AppX config](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/electron-builder-appx.json)
- [draw.io Snap config](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/electron-builder-snap.json)
- [Electron44.4.5 nine FuseV1Options](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/tutorial/fuses.md)

Verified source and limits: All five pinned draw.io JSON files were read. They contain custom hooks, publisher-name arrays, nested file associations,
platform metadata, Linux targets, an arm64 portable target, per-machine NSIS and user-selectable installation roots. The main
CLI still reserves `build` as KELD-CLI-045. PR1407 proposes a narrow local macOS `keld.build.ts` literal schema; it is a draft
with a separate owner, not an approved general destination for this translation. The architecture sketch is not a parser. The
nine upstream fuse names below come from the pinned Electron tutorial. No configuration was evaluated and no app hook ran.

All proposed API, schema, ownership and permission changes below are future contracts.
This documentation PR changes no runtime boundary and approves none of those changes.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

The implementation cites the exact approved build-schema revision and admitted target cells. Without it, only an
inventory/report is allowed; no guessed executable config is emitted and `keld build` remains reserved.

Independent negative control: Replace the schema pin with the architecture sketch or unapproved PR1407 and attempt successful build-config emission; admission
must fail.

### AC2

Each of the five pinned configs has a report whose input field-instance count equals mapped plus ▲ plus ✘ counts. Every object
member, nested member and array-element path is accounted for exactly once; container rows name their children. Unknown fields
are retained as ✘ with their source location.

Independent negative control: Insert an unknown nested field and delete its report row; an independent tree walker finds the missing path and fails
completeness.

### AC3

JSON and YAML use a closed static-data parser with duplicate-key rejection, bounded size/depth and explicit handling of YAML
aliases/tags. Unsupported syntax produces a located error and no output config. Deterministic output ordering preserves
meaningful array/glob order.

Independent negative control: Use duplicate `appId`, an executable YAML tag, an alias expansion exceeding the bound or reordered exclusion globs; each fails
or has a documented unchanged semantic result.

### AC4

The report routes `appId` through Pack's canonical ExpectedAppIdentity owner. Files/globs and ASAR/unpack settings receive
plain-tree dispositions with exclusions and resources preserved; a signing container or ignored unpack hint never becomes a
second trust/parser path.

Independent negative control: Use a different app ID between generated metadata and compiled expected identity, or package a file excluded by the input; the
corresponding identity/tree comparison fails.

### AC5

Every requested target/architecture and installer mode has its own row. `nsis.perMachine` and
`allowToChangeInstallationDirectory` require approved installation authority and KEL-254 root binding. Portable, MSI, AppX, Snap
and unsupported architecture rows cannot be silently mapped to a direct-updater target.

Independent negative control: Label draw.io arm64 portable as supported Windows x64 PerUserDirect, or treat a user-chosen root as an unrestricted later launch
path; target admission rejects it.

### AC6

All custom JavaScript hooks, custom sign functions and Forge hooks are ✘ manual with exact source paths. Translation executes
zero application hook code, including beforePack, afterPack, afterSign and msiProjectCreated.

Independent negative control: A hook writes a sentinel outside the report directory. Run translation and require no sentinel while the hook row remains
explicit.

### AC7

Executable Forge/JS/TS configuration is rejected by the default path with an exported-JSON alternative. Any separately approved
developer-run evaluator requires an explicit invocation and warning before evaluation, records resulting data provenance, and
makes no sandbox claim; it does not run lifecycle hooks.

Independent negative control: Supply a JS config with import-time side effects to the default path, or omit the explicit evaluation flag; no marker or child
process may occur.

### AC8

The fuse report contains exactly the nine pinned FuseV1Options in section4.3, one row each, with an explanation of the Keld
analogue or absence. Configuration-specific fuse values may be unknown when hidden in an unexecuted hook; unknown is not
disabled.

Independent negative control: Delete WasmTrapHandlers, duplicate RunAsNode or infer false from an unread hook; exact-set and evidence checks fail.

### AC9

Signing/notarization, entitlements, publication, associations, metadata, installer flags and target tool settings are
individually mapped or explicitly manual. No translator output claims valid signing, notarization, distribution compatibility or
update reachability from configuration alone.

Independent negative control: Use a local ad-hoc macOS bundle as evidence of Developer ID distribution, or map a custom sign script to a successful signing
result; the report remains blocked/manual.

### AC10

A deterministic manifest records input commit/digest, parser version, approved schema pin, field dispositions and unresolved
prerequisites. Re-running on identical inputs yields identical semantic report/config, and any blocked field keeps the whole
generated build proposal non-executable until explicitly resolved.

Independent negative control: Change one input file after recording its digest or overwrite a pre-existing config without the approved replace workflow;
output is rejected or remains a separate proposal.

## 4. Design

### 4.1 Data and authority boundary

The input is source data; the output is a proposed configuration plus a disposition manifest. The translator owns only
interpretation/reporting, never build, signing or installer authority. Reuse the migration inventory/parser and diagnostics when
their approved contracts admit these formats. Pack remains the only canonical ExpectedAppIdentity serialization owner. The
approved packaging schema owns destination field names, target admission and artifact semantics. Do not create another config
schema inside this ticket.

Default processing: identify file and parser → parse bounded static data → enumerate source field instances independently →
compute dispositions against the pinned approved schema → compare exact coverage → write separate reviewable outputs. Missing
schema may produce an inventory only; it cannot cause guessed target fields.

### 4.2 Required source-field decisions

| Source family | Required disposition |
| --- | --- |
| appId | Canonical Pack identity; mismatch is a blocker. |
| files, asar, asar.unpack / unpack variants | Plain admitted tree; preserve excludes; explain dropping ASAR. |
| targets and architecture | One admitted or blocked row per target/architecture, including portable. |
| nsis.perMachine | Installation mode and updater authority selected by their owner. |
| allowToChangeInstallationDirectory | Installer selection bound to KEL-254 launch identity. |
| signing, notarize, entitlements | Declarative mapping only when schema admits it; scripts remain manual. |
| publish / channel | Build publication intent only; no runtime Host feed authority. |
| fileAssociations / extendInfo | Preserve nested platform meaning or list the exact unsupported member. |
| custom hooks / custom sign | Manual; record filename without importing or executing it. |
| unknown keys / unadmitted targets | Explicit ✘ with source path, reason and owning next decision. |

A report row has source file, JSON-pointer-like instance path, source location, source value category, disposition, target
schema path if mapped, rationale and blocking owner/reference. Containers and leaf instances use distinct row kinds so a summary
row cannot hide unknown descendants. Arrays preserve index and order; exclusions are not sorted as if commutative. A report may
display redacted sensitive values, but the field path and disposition remain visible.

### 4.3 Nine-fuse baseline

| FuseV1Options member | Proposed disposition basis |
| --- | --- |
| RunAsNode | ▲ Electron flag does not configure Bun's Host-owned launch. |
| EnableCookieEncryption | ▲ No fuse mapping; storage/session owner must state equivalent limits. |
| EnableNodeOptionsEnvironmentVariable | ▲ Existing owned role environment, not an Electron binary switch. |
| EnableNodeCliInspectArguments | ▲ Host launch/debug policy; no automatic debugger capability. |
| EnableEmbeddedAsarIntegrityValidation | ▲ Keld package integrity is a separate signed-tree contract. |
| OnlyLoadAppFromAsar | ▲ No ASAR loader; admitted plain-tree boot is separately verified. |
| LoadBrowserProcessSpecificV8Snapshot | ✘ Manual/unsupported V8-specific artifact. |
| GrantFileProtocolExtraPrivileges | ✘ No automatic file-origin privilege grant. |
| WasmTrapHandlers | ▲ Engine-specific behavior; no equivalent fuse asserted. |

These are proposed migration explanations, not claims of equivalent runtime protection. Record both the full reference set and
the requested value where statically known. draw.io's afterPack points to a fuse script; this translator must not execute it to
fill an unknown value. Changes to the upstream pinned reference set require a reviewed report revision.

### 4.4 Executable config and transactional output

Recommend exported JSON for Forge. A future developer-run evaluation is an ordinary execution of trusted project code with the
developer's authority; it is a separately approved CLI mode, not an automatic fallback and not containment. The warning precedes
evaluation and the report records it. Hook function bodies are not invoked as translation actions. Dynamic values that cannot be
represented in exported JSON stay manual.

Outputs are staged in an owned destination and published together after exact coverage succeeds. Invalid input, schema mismatch
or write failure preserves any pre-existing config. Report-only output must clearly distinguish blocked generation from
successful build-config emission. Never enable the reserved build verb as a side effect.

## 5. Boundaries

Files initially allowed: future translator/report modules under the migration/tooling owner, their focused fixtures and these
docs. Proposed public config/output shapes, error-code allocation and CLI flags need the packaging/tooling owners' exact
approval. Do not edit PR1407, KEL-254, runtime policy, update manifests, signing helpers, global schema/CI or existing migration
output without coordinated ownership. Parsing file size, nesting, aliases, glob expansion, report rows and total output bytes
require numeric caps before implementation; protocol frame limits do not supply parser budgets.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Pin the approved destination schema and exact five-config/fuse fixtures; independently enumerate source field paths and record
all target blockers. Resolve parser/size/output limits before code.

### T2

- [ ] Implement bounded static parsing and the exact-coverage report in the existing tooling seam. Add hook/executable-config sentinel
controls and deterministic outputs; leave the build verb reserved.

### T3

- [ ] Add approved mappings for canonical identity, tree/resources, target/install mode and platform metadata. Report every
unsupported/custom field; independently compare emitted tree and identity against the source.

### T4

- [ ] Review the five complete reports and all nine fuse rows, exercise malformed-input/write-failure/overwrite controls, and publish
exact receipts. Keep distribution, native install and build-schema approval gates separate.

## 7. Test plan

The independent oracle for completeness is a second simple source-tree enumerator, not the translator's own disposition list.
Golden fixtures cover all five immutable draw.io files and a minimal Forge-exported JSON object. Add nested-unknown,
duplicate-key, escaping/location, alias-budget, glob-order and hook-side-effect mutations. The original config corpus is read as
source; no hooks or signing tools execute. For future native packaging, each admitted mapping also requires the packaging
owner's artifact/installed-launch tests. This batch provides the specification, not those receipts.

Every acceptance criterion has an independently observable outcome and named negative control.
Record source commit, pinned reference, exact platform/runtime and raw receipt for every claimed result.
Documentation validation covers immutable source/readback, ten sections, controls and file limits.
Runtime, native and original-app tests are planned, not executed by this drafting session.
GitHub documentation CI cannot establish platform acceptance. Local `just ci`, hooks and machine
closeout were not run: the user restricted this work to GitHub. Official upstream GitHub sources
were read within that restriction; no Context7 query or external-machine evidence is claimed.

## 8. Review gates triggered

Public API/configuration and tooling review apply when output schema, CLI or report shape changes. Security review applies to
executable-config evaluation, identity/resource mapping and target/install-mode claims. Dependency/unsafe review is conditional
on implementation choices and is not pre-approved. The local macOS draft in PR1407 and first-proof migration work remain
separately owned. Documentation publication is authorized; the parked implementation remains blocked by an approved build schema
and the section10 choices.

Status remains draft. Implementation requires the section 10 decisions and approval of the exact contract.
Draft publication does not approve an upstream dependency or change a parked implementation schedule.

## 9. Perf impact

Static inventory cost should scale with input bytes and enumerated field count. No runtime app-startup effect is intended. Bound
YAML expansion, glob enumeration and output size; keep hooks and full builds out of translation. Measure representative config
size, parse/report time and peak memory after implementation before setting numeric budgets. No benchmark was run.

## 10. Open questions (block implementation)

1. Packaging owner: approve the exact destination schema, its relation to PR1407's local macOS subset, and a versioned
field-admission mechanism. No general schema is approved by this document.

2. Tooling owner: select static JSON/YAML parser, duplicate-key/alias policy, numeric budgets, output transaction and overwrite
workflow. Reuse existing tools where their contract fits.

3. Installation owners: settle each target, perMachine mode, portable/MSI/AppX/Snap behavior and user-chosen root mapping; do
not infer admission from a source target name.

4. Tooling/security owners: decide whether the explicit developer-run executable-config mode is needed at all. Exported JSON
remains the recommended path; approve warning and provenance before any evaluator.

5. Compatibility owner: accept the nine fuse dispositions and explicit runtime-divergence wording. Source-level settings are not
proof of equivalent protections.

Independent review must refute or fix concrete findings on the published revision. Platform gaps
remain explicit; no unexecuted acceptance test is reported as passed.
