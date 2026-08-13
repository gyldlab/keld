# Keld Tooling Audit — Senior Engineer Review

> Audit date: July 2026. Baseline: pre-alpha 11-crate workspace, no `packages/` yet.
> Competitor comparison: `docs/research/08-competitor-source-audit.md`.
> Context7 refresh: `docs/research/09-tooling-context7-audit.md`.

## Executive summary

Keld's tooling was **thin at audit start** (workspace `Cargo.toml` only). This audit
landed a production-grade Rust CI baseline: pinned toolchain, workspace lints, fmt/clippy/
deny configs, 3-OS matrix, MSRV gate, and `justfile` local parity. The foundation now
matches or exceeds Tauri/WRY discipline; gaps vs Deno's generated-CI sophistication remain
P1/P2.

**Applied in this audit cycle.** Config files listed in §Changes applied.

**Deferred (with rationale).** cargo-llvm-cov (no coverage baseline yet); cargo-machete (tiny dep tree);
nursery clippy (pedantic not fully exercised); TS/Biome CI (no `packages/`); release
automation (nothing to ship); cargo-vet (deny.toml covers advisories/licenses for now).

---

## Findings — workspace

| Area | Before | After | Verdict |
|------|--------|-------|---------|
| Resolver / edition | `3` / `2024` | unchanged | ✅ Already modern |
| MSRV | `1.93` in Cargo.toml | unchanged | ✅ Matches pin |
| Workspace lints | pedantic warn only | + `unsafe_code deny`, unwrap/expect/panic/todo lints | ✅ AGENTS.md aligned |
| Release profile | thin LTO, strip, panic=abort | unchanged | ✅ Good for size budget |
| Per-crate metadata | version/edition inherited | unchanged | ⚠️ Add descriptions when publishing |
| `packages/` TS | empty | unchanged | P1 when scaffolded |

---

## Findings — config files

| File | Status | Notes |
|------|--------|-------|
| `rust-toolchain.toml` | **Added** | Pin `1.93.0` + rustfmt + clippy |
| `rustfmt.toml` | **Added** | `style_edition = "2024"`, stable-only |
| `clippy.toml` | **Added** | Test exemptions for unwrap/expect/panic |
| `deny.toml` | **Added/extended** | Licenses, advisories, bans, sources; + platform targets |
| `.editorconfig` | **Added** | LF, indent rules; defers Rust to rustfmt |
| `justfile` | **Added/extended** | `just ci` mirrors CI gates incl. deny |
| `.github/workflows/ci.yml` | **Added/extended** | fmt, 3-OS clippy+test, doc, MSRV, deny |
| `.github/dependabot.yml` | **Added** | github-actions weekly; cargo manual per AGENTS.md |

---

## Findings — CI vs competitors

| Gate | Keld | Tauri | Deno | WRY/TAO |
|------|------|-------|------|---------|
| clippy `-D warnings` | ✅ | ✅ | ✅ | partial/none |
| 3-OS matrix | ✅ | ✅ (5 targets) | ✅ (generated) | ✅ |
| rust-cache | ✅ | varies | varies | partial |
| cargo-deny | ✅ | vet (soft) | advisories | audit job |
| MSRV job | ✅ | in matrix | pinned toolchain | MSRV field |
| Miri | — | — | — | Tao only |
| nextest | ✅ (`.config/nextest.toml`, CI profile) | some projects | — | — |

---

## Changes applied

### Phase A (prior worker, verified green)

- `Cargo.toml`: workspace lints (`unsafe_code`, unwrap/expect/panic/todo/dbg)
- `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml`, `deny.toml`, `.editorconfig`
- `justfile`, `.github/workflows/ci.yml`, `.github/dependabot.yml`
- `.gitignore`: `/competitors/` for research clones

### Phase B (coordinator run — P0 from Context7 audit)

- `deny.toml`: `[graph].targets` for linux-gnu, macOS, windows-msvc; `unmaintained = "warn"`
- `.github/workflows/ci.yml`: `rust-cache` on fmt + deny jobs; `cache-on-failure: true` on matrix
- `justfile`: `ci` recipe includes `deny`

### Phase C (alignment audit — nextest landed)

- `.config/nextest.toml`: CI profile with retry policy
- `.github/workflows/ci.yml`: `cargo nextest run --workspace --profile ci` on 3-OS matrix
- `justfile`: `test` recipe matches CI nextest profile
- `AGENTS.md` verification gate updated to nextest

---

## Recommendations — adopt later

| Priority | Item | Rationale |
|----------|------|-----------|
| **P1** | `cargo clippy --all-features` | Match deny.toml `all-features = true` |
| **P1** | Miri job (nightly, ubuntu) | Tao precedent for `keld-wv` unsafe backends |
| **P1** | Biome for `packages/*` | Single Rust-class toolchain for TS |
| **P1** | Generated CI from `tools/ci.ts` | Deno pattern; prevents workflow drift |
| **P2** | cargo-llvm-cov + codecov | Non-blocking until baseline exists |
| **P2** | cargo-machete | Unused dep detection as tree grows |
| **P2** | cargo-vet alongside deny | Tauri supply-chain depth |
| **P2** | Release workflow | When first binary ships |
| **P2** | `[profile.dist]` fat LTO | Final shipping builds only |

---

## Verification

Run before merge (AGENTS.md gate):

```bash
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo nextest run --workspace --profile ci
```

Optional locally: `just ci`, `cargo deny check` (requires cargo-deny installed).

---

## Open questions

1. **Licensing:** workspace declares MIT OR Apache-2.0; README/ROADMAP updated to match.
2. **MSRV vs pin:** both `1.93` today; document bump policy when pin advances.
3. **Strip + crash triage:** release `strip = "symbols"` may complicate host crash reports — revisit with `keld-update` rollback UX.
