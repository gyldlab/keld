## Summary

<!-- What changed and why. Conventional title: `feat(ipc):`, `fix(wv/macos):`, `docs(research):`. -->

## Spec refs

<!-- `docs/architecture/0N-*.md` sections, Linear KEL-n. Write "none" for a pure bug fix. -->

## Review gates

Human sign-off is required for any that apply (`AGENTS.md`). Check all that this PR touches, or check **none**.

- [ ] `unsafe` (new or changed)
- [ ] Public API (new or changed)
- [ ] Permission model
- [ ] Dependency addition
- [ ] Wire protocol (kipc frames, manifest schema, update feed)
- [ ] none

## Tests

<!-- Paste real gate output. Never write "should work". -->

```
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace --profile ci
```

## Platforms

<!-- macOS / Windows / Linux — which were actually run. Say so if a path was not verified. -->

## Perf impact

<!-- Architecture 01 §5 budgets, or "none". >5% regression needs a waiver. -->
