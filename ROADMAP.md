# Keld public roadmap

Keld is pre-alpha. The generated [Current/Target/Evidence ledger](docs/engineering/product-status.md)
owns implementation status; the [architecture](docs/architecture/01-overview.md) and
approved issue specs own the target contracts. This page presents their dependencies
and exit criteria, without creating another completion ledger or promising dates.

Follow [public GitHub issues](https://github.com/gyldlab/keld/issues) for scope,
progress, and decisions. The [open-source foundation work](https://github.com/gyldlab/keld/issues/167)
tracks public documentation, governance, security checks, genuine testing, and release
readiness. Maintainers connect public issues to internal planning; private tracker
access is not needed to follow or contribute to the project.

## What is available

The current slice includes scaffolding and diagnostics, a native webview window with
a supervised Bun process, authenticated app-link communication, permission evaluation,
and a limited Electron lifecycle facade. The [quick-start](docs/onboarding/README.md)
runs that slice. Native services, the wider compatibility surface, migration, packaging,
and updates remain incomplete; consult the ledger for each component's exact scope.

## Prerequisites and exit criteria

| Order | Work | Evidence needed to advance |
|---|---|---|
| Foundation | Make the current slice reproducible and its limitations public; establish contributor, security, and review procedures | A stranger can build and run the documented demo, report a result publicly, and trace each product claim to the ledger and its evidence |
| Guarded application slice | Complete the public lifecycle/window contract and remaining runtime admission for the declared platforms | Host-owned windows survive the specified child failures; fresh generations invalidate stale authority; the documented app runs on every platform claimed |
| Bounded native and compatibility plane | Complete required IPC ordering/backpressure/cancellation, generated permissions, renderer bridge, native brokers, and common Electron behavior | Approved contracts have failure-path and negative-control evidence; a declared representative application corpus exercises `migrate` and `dev`, with gaps visible |
| Distribution and recovery | Build installable artifacts, qualify signing and package channels, implement signed updates and rollback | Clean-machine installation and interrupted/corrupt-update scenarios pass on each claimed OS/channel; shipped security profiles have real containment evidence |
| Compatibility depth | Expand the measured app corpus and only the named roles, addon support, or engine options that it requires | A committed denominator, pinned artifacts, operation-level results, and explicit failures/waivers support each published compatibility claim |

The ledger's [phase view](docs/engineering/product-status.md#phases) shows how much
of each area exists today. Work can advance independently when its prerequisites are
satisfied; a partial implementation does not mark an entire phase complete.

## A release follows evidence

The source version is currently development metadata, not proof of an alpha release.
An alpha/dev-preview release should name its supported runnable slice and known limits,
include reproducible validation, and link a real tag and release notes. Automated
release publishing, checksums, SBOM/provenance, and signed or attested artifacts belong
with the reviewed artifact pipeline when binaries begin shipping. The
[changelog](CHANGELOG.md) records the current unreleased baseline.

## VS Code and other demanding applications

VS Code migration is future work. It depends on the framework's migration command,
process contracts, native services, extension behavior, and installable artifacts.
It is a separately measured stress workload, with its input artifact and permitted
adaptations declared. An opened editor window cannot stand in for a migrated desktop
application, and showcase results do not change the general product denominator.

## Research that earns implementation

Material unknowns need a bounded question, competing hypotheses, primary evidence,
and the smallest discriminating experiment before implementation. Current examples
include Bun/Electron semantic differences, platform containment and engine behavior,
compatibility corpus coverage, and end-to-end performance attribution. Existing
[compatibility evidence](docs/engineering/compat-scoreboard.md) and
[benchmark evidence](docs/engineering/budget-scoreboard.md) identify their limitations.
New research does not by itself create a roadmap obligation.

Keld protects four core ideas: a prebuilt host, supervised Bun roles, kipc, and
generated host-enforced default-deny. It does not need a new browser engine, mandatory
shared memory, or a VS Code-specific core to demonstrate those contracts. See the
[architecture's non-goals](docs/architecture/01-overview.md#6-what-keld-is-not-v1-non-goals).
