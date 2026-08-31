# Third-party licenses for packed binaries

Engineering compliance notes for distributing Keld host/CLI binaries (and, later,
app installers from `keld-pack`). This is not legal advice. Counsel should review
the first external binary distribution.

Keld itself is declared `MIT OR Apache-2.0` in the workspace `Cargo.toml`.
`deny.toml` is an allow-list: new licenses are a dependency review gate, not a
drive-by `allow` edit.

## Current cargo-deny exception

`option-ext` 0.2.0 is MPL-2.0. It is allowed only as a **per-crate, version-pinned**
exception in `deny.toml`:

```toml
exceptions = [{ crate = "option-ext@0.2.0", allow = ["MPL-2.0"] }]
```

Do not add `MPL-2.0` to the global `allow` list. A global allow would cover any
future MPL crate that appears in the graph.

### Why the crate is in the tree

```
keld-wv → wry 0.56.1 → dirs 6.0.0 → dirs-sys 0.5.0 → option-ext 0.2.0
```

- crates.io: <https://crates.io/crates/option-ext/0.2.0> (latest, not yanked,
  published 2023-01-11, license `MPL-2.0`).
- Upstream still 0.2.0: <https://codeberg.org/soc/option-ext>
- `wry` 0.56.1 (and 0.56.0) depends on `dirs = "6"` unconditionally for Apple and
  Linux/BSD. Keld's `devtools` feature does not control that edge.
- `dirs-sys` 0.5.0 depends on `option-ext = "0.2.0"` unconditionally (Linux XDG
  parser uses `OptionExt::contains`).
- Keld does not vendor or modify `option-ext`. The crate is fetched from crates.io
  via `Cargo.lock`.

Removal is not the smallest safe change: replacing live wry/tao scaffolding is
separate architecture work; forking `dirs-sys` to drop one helper creates a
maintained fork solely to avoid a compatible license.

Treat every shipped target as containing the MPL component. Apple may dead-strip
the Linux-only `OptionExt` use; that is not a compliance basis.

### What MPL-2.0 requires of a Larger Work

Mozilla Public License 2.0 is **file-level** copyleft (MPL §§1.7, 1.10, 3.1–3.4;
FAQ Q8, Q11, Q12). Static linking into Keld does not relicense Keld as
GPL-style whole-work copyleft. The Larger Work may stay MIT OR Apache-2.0.

Executable distribution still requires:

1. Preserve copyright and MPL notices on covered files.
2. Identify the MPL-covered source (`option-ext` 0.2.0 as locked).
3. Make the **exact corresponding source** of those covered files available by
   reasonable, timely means (source archive with the release, or a durable URL
   to that exact version).
4. If Keld ever modifies an MPL-covered file, publish those modifications under
   MPL.

## Packaging checklist (first binary ship)

Required before any external `keld` / host binary or `keld-pack` installer:

- [ ] Third-party notice file in the package (or an equivalent About/credits
      surface) lists `option-ext` 0.2.0, MPL-2.0, and the crates.io / upstream
      URLs above.
- [ ] The MPL 2.0 license text is included or linked from that notice.
- [ ] Exact corresponding source for `option-ext` 0.2.0 is offered: a crate
      tarball matching `Cargo.lock` (checksum
      `04744f49eae99ab78e0d5c0b603ab218f515ea8cfe5a456d7629ad883a3b6e7d`) or a
      durable URL to that exact crates.io/upstream revision. A floating
      "latest" link is not enough.
- [ ] Confirm Keld still does not modify `option-ext` (no `[patch]`, no vendor
      copy). If that changes, the modified files ship under MPL and the source
      offer includes the diffs.
- [ ] `cargo deny check licenses` is green on the release commit.

`keld-pack` is still a skeleton. When it grows an installer pipeline, this
checklist becomes a release gate in that crate — do not invent a parallel
license-scanner stack.

## Residual cargo-deny findings

This exception addresses **licenses** only. Do not expand it to silence other
deny checks.

`cargo deny check` (cargo-deny 0.19.9, 2026-08-13) reports
`advisories ok, bans ok, licenses ok, sources ok`. Remaining output is warnings
only (unused license allowances; `multiple-versions` / wildcard-path at `warn`
by policy).

A prior failure was yanked `spin` 0.9.8 via `postcard` → `heapless`. The lockfile
now has `spin` 0.9.9 and advisories pass. If a yank/advisory returns, that is a
separate dependency decision (KEL-53) — do not allowlist yanked crates here.

## Review

Dependency/license policy is an independent review gate (`AGENTS.md`) under the standing
repository-owner delegation. Engineering review does not replace counsel before first external
distribution.
