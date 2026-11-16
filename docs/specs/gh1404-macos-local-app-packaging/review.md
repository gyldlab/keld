# macOS release evidence and gaps
Part of: [index](index.md)
Owns: Performance evidence and native blockers

## 9. Perf impact
Packaging/signing/DMG work is cold tooling. Runtime bundle and graph validation adds
startup I/O; measure actual installed startup and memory for the same inputs before
making claims. Stream large file hashes/copies, reuse existing bounded parsers and
inventory limits, and avoid allocating file contents wholesale. No language rewrite,
new async runtime or performance budget waiver is proposed.

## 10. Open questions, blockers & unknowns
**Recorded approval:** the user authorized this exact local-development contract:
ad-hoc self-consistency without publisher authenticity; an ephemeral browser profile;
existing explicit Legacy authority; no Strict/updater/publisher-profile privileges;
the existing Host as launcher; exact bundled Bun; provenance-scoped cleanup; and a
separately qualified original-content package caller. The approval provenance is
recorded above; implementation review and native acceptance remain separate gates.

**Integration evidence needed:** owners of #499/#502/#553/#452/#570 must supply the
exact committed source/provider and applicable qualification handoff required by
T1/T2. Current bounded test integration is real evidence but is not installed release
approval. #589 records the resulting end-to-end consumer evidence.

**Execution uncertainties:** preserved pinned-Bun operation in the installed layout, resource
replacement controls, original draw.io packaged-path behavior and required descendant
cleanup are tests, not assumed passes. Preserve a named blocker and owner if a
required predicate fails. Stronger same-user replacement protection would require
an independently approved trust anchor and cannot be promised by this artifact.

**Future only:** Windows/Linux installer format, installed trust details, signing,
update ownership and supported native cells remain in #1405/#1406. They do not delay
macOS implementation once its own approval and source dependencies are satisfied.

### B-1: Full installed macOS acceptance is not yet qualified
- **Status:** open
- **Owner:** GH-1404 release and native acceptance owners
- **Impact:** The complete signed local app, DMG, original workflows and cleanup cannot be called finished.
- **Check:** Install and relocate signed artifacts and pass every applicable AC with recorded native evidence.
