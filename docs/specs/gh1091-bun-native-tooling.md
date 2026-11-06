# Spec: Bun 1.4.4 and native JavaScript tooling

Status: implementing
Linear: KEL-177 · Public: GH-1091 (user-requested combined PR) · Owner: GYLDLAB · Updated: 2026-10-23

Approved by the repository owner in this session, 2026-10-23: continue with stable
Bun 1.4.4 native formatting; track native lint until it is available in stable.

## 1. Goal & non-goals

Use stable Bun 1.4.4 consistently for active repository runtime pins, CI and current
setup guidance. Give contributors and newly created Keld applications native Bun
formatting commands with a non-writing check suitable for CI. The user also requests
native linting; this remains blocked because the verified stable binary has no native
`bun lint`, and upstream documents lint as canary-only.

Non-goals: relabel historical receipts, replace TypeScript's strict checker, change
application runtime ownership or permissions, claim faster builds, add a third-party
formatter/linter, or silently upgrade the application runtime to canary.

## 2. Spec refs

No boundary change. Architecture 06 §1 owns the supervised, pinned Bun runtime;
architecture 07 owns developer/agent tooling. Existing KEL-77 differential tests keep
their oracles. Public claims remain independently owned by CONTRIBUTING.md.

## 3. Acceptance criteria (binary, each becomes a test)

1. Every active setup-bun step, packageManager field and strict runtime assertion
   selects 1.4.4; a stale 1.4.2, canary, latest or prefix-lookalike fails pin admission.
2. The existing package tests, strict TypeScript checks and Rust workspace gates pass
   with the selected stable binary. Each unrun platform is explicitly unverified.
3. Repository JavaScript/TypeScript formatting uses native `bun fmt --no-script`;
   its check variant returns nonzero for deliberately unformatted input without
   modifying it, formatting repairs it, and a second check leaves it unchanged.
4. `keld create` emits a dependency-free package with `format` and `format:check`
   scripts running the same native commands. A real generated app passes both;
   intentionally unformatted app code fails its check before formatting fixes it.
5. Generated JavaScript/TypeScript artifacts are updated only through their owning
   generators. Formatting a generated output and regenerating must agree; an
   incompatible generator is corrected at its owner before enabling the gate.
6. CI and `just ci` execute the formatter check for affected JavaScript inputs,
   including scaffold and generator inputs. Removing the command or its required
   admission dependency makes a negative control fail.
7. No successful lint check, native lint script or shipped-lint claim exists unless
   the selected, exact upstream tool version actually supports native lint. On the
   stable choice, the missing capability remains explicitly tracked on GH-1091.

## 4. Design

Reuse the upstream Bun binary and existing `just`/CI inventory, input-fingerprint
generator, package manifests and CLI template owner. Do not add a second formatter
implementation or a formatter runtime dependency. Keep `tsc --noEmit` and existing
semantic tests; formatting and linting are separate contracts from type checking.

Repository formatting calls native Bun directly with
`"**/*.{js,jsx,mjs,cjs,ts,tsx,mts,cts}"`, including tests, tooling and nonignored
untracked sources. Root `bunfig.toml` owns formatter configuration; its empty
`ignorePatterns` adds no exclusions beyond Bun's native ignore rules. The native
inventory contains no source requiring an additional exclusion. Existing CI tests
cover all eight extensions, native ignores and unchanged non-JS/TS files.
Bun also resolves path-scoped package, Prettier, oxfmt and EditorConfig settings.
The shared CI input contract admits their verified filenames, including Windows
case variants, to both local and hosted formatter checks and the generator's
TypeScript tests. A config-only change cannot bypass those checks merely because
the TypeScript bytes are unchanged. Root manifests retain the existing unknown
input fallback rather than narrowing shared build-graph changes.
Do not run an unrestricted formatter
over instruction Markdown, historical evidence, nested research or vendor files.
Use Bun defaults unless an existing project contract requires an explicit setting;
disable import sorting and other optional transforms by leaving their defaults off.
Generated files must reach the same formatting through their existing owner, never
through manual edits. Preserve intentional malformed parser fixtures as fixtures,
with a named path and regression oracle rather than a broad source exclusion.

`just fmt` normalizes Rust (including included files) before `just js-fmt`.
`just js-fmt` generates bindings, runs the native formatter, then regenerates to
restamp the final transport bytes and checks freshness. The generator formats only
owned outputs and the channel-table region, using each actual destination path
for Bun options; it preserves the manual transport body. It computes all outputs
before any write so a late computation failure leaves destinations unchanged.
Filesystem write errors are not a multi-file transaction. Read-only local/hosted
checks run explicit codegen freshness and the direct native formatter check.

Generated applications expose `format` invoking `bun fmt --no-script` over the
JS/TS family glob and `format:check = bun run format --check`, forwarding the
read-only flag to that same command. This scope matches repository JS/TS formatting;
HTML/JSON/Markdown are available through direct native Bun commands. Arbitrary
admitted project-name lengths remain valid without adding a runtime formatter
subprocess to `keld create` or formatter-ignore directives. These commands run in the application
checkout with the contributor's normal tooling authority, not in the sandboxed
application process. Native commands must not recursively call identically named
package scripts. CLI/template tests execute both commands in a temporary app and
inspect exit codes, bytes and a second successful check.

New Rust types/channels, capabilities, manifest grants and wire changes: none.
Runtime seam: none. Tool processes are cold developer/CI subprocesses; existing
supervised application lifetime and host handles remain owned by their current
owners. Platform notes: prove Windows locally; require actual Linux/macOS CI for
their corresponding tests. No inference from one OS to another.

Migration unit: active Bun CI pins, strict runtime expectations, packageManager
metadata, setup docs, JS formatting recipes/CI admission, scaffold scripts and any
formatter-incompatible generated source owner. Historical evidence stays historical.
Compatibility fallback: existing semantic tests and type checker remain required;
there is no false-green fallback for unavailable native lint.

## 5. Boundaries

Implement in `.github/workflows/ci.yml`, existing `tools/ci_*` owners, `justfile`,
`packages/@keld/*/package.json`, CLI template/create tests and relevant current setup
documentation. Correct a generator only if its output violates the selected formatter
contract, after loading its governing spec and nearest crate rules.

Must not touch Cargo dependency selection, unsafe production paths, permissions,
wire parsing, historical compatibility receipts or unrelated public readiness work.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

The user explicitly requests these slices in the same PR as the public-claims fix.

- [ ] T1: update active stable pins and current docs; prove existing pin rejection
  controls accept only 1.4.4 and execute existing semantic gates with that binary.
- [ ] T2, approved: integrate native formatting into the repository
  and generated apps, with failure-first check/mutation tests and CI admission.
- [ ] T3, deferred on GH-1091: when native lint is available in an official stable
  release, verify its actual command contract and integrate it through a reviewed
  update. Keep the current runtime/tooling stable; do not install canary for lint.

## 7. Test plan

AC1: existing CI-hygiene tests plus stale-pin/canary/latest/prefix negative controls.
AC2: frozen package installs, strict package typechecks/tests, `just ci`, format,
warning-denied workspace clippy and full workspace nextest; hosted OS-specific gates.
AC3–5: real native formatter subprocesses, exact file hashes before/after check,
idempotent output and real generated app tests; regenerate using current generators.
AC6: existing CI router/hygiene suites with command-removal negative control.
AC7: exact stable binary `bun lint --help` exit1 and `Script not found "lint"`;
recheck upstream capability before any future claim. Tests use isolated temporary
directories, awaited subprocess exit, no sleep synchronization or external network.

## 8. Review gates triggered

Dependency (Bun runtime bump); independent architecture/dependency review of exact
diff. Unsafe, public Rust/TS API, permission model and wire protocol: none.
CI/shared input owners also require independent review under existing policy.

## 9. Perf impact

None claimed. Formatting adds a developer/CI check; no runtime performance change
is claimed from the language or upstream benchmark numbers.

## 10. Open questions

None for the approved formatter implementation. Native lint is explicitly deferred
until an official stable release supports it, tracked on GH-1091 and KEL-177. Its
future update must verify release provenance, actual native execution and CI/app
acceptance before changing the current stable tooling contract.

Upstream evidence (retrieved 2026-10-23):
[release](https://github.com/oven-sh/bun/releases/tag/bun-v1.4.4),
[formatter release notes](https://bun.sh/blog/bun-v1.4.4),
[lint availability](https://bun.sh/docs/lint).
Local checksum-verified Windows binary: `1.4.4+38afee6c8`; `bun fmt --help` exits0,
`bun lint --help` exits1 with `error: Script not found "lint"`.
The immutable [1.4.4 lint command source](https://github.com/oven-sh/bun/blob/bun-v1.4.4/src/runtime/cli/lint_command.rs)
sets `IS_ENABLED` to `bun_core::Environment::IS_CANARY`; the command is disabled in
stable builds, rather than hidden behind a supported runtime flag.
