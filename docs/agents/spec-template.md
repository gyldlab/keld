# Spec template

Copy this into `docs/specs/<kebab-name>.md` for any change bigger than a bug fix.
Keep every section; write "none" rather than deleting. A spec is implementable when a
fresh agent could build it without asking questions. Implementation may start only at
Status: approved (human sign-off).

```markdown
# Spec: <name>
Status: draft | approved | implementing | done
Linear: KEL-<n> · Owner: <human> · Updated: YYYY-MM-DD

## 1. Goal & non-goals
One paragraph: the problem and the observable outcome. Bullet the explicit non-goals.

## 2. Spec refs
Which `docs/architecture/*` sections govern this. If the design deviates, this spec
must say so and the same PR updates the architecture doc.

## 3. Acceptance criteria (binary, each becomes a test)
1. Given <state>, when <action>, then <observable result>.
2. Error case: given <invalid input>, when <action>, then <typed error, message states the fix>.
3. …

## 4. Design
- New/changed types & channels (Rust signatures, `.k.ts` contracts — sketch real code)
- Capabilities required; manifest changes (spec 03) — "none" if none
- Wire/protocol changes (spec 02) — "none" if none (else: version bump + review gate)
- Platform notes: mac / win / linux behavior differences

## 5. Boundaries
- Implement in: <crate(s)/package(s), files>
- Must not touch: <crates, files — e.g. workspace Cargo.toml, other crates' internals>

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)
- [ ] T1 …
- [ ] T2 …

## 7. Test plan
Map each acceptance criterion to a test (unit/integration/conformance/bench).
Note anti-flake concerns (timing, ports, platform-only paths).

## 8. Review gates triggered
unsafe? public API? permission model? dependency? wire protocol? (list or "none")

## 9. Perf impact
Which budgets (architecture/01 §5) could move; bench to run; or "none".

## 10. Open questions
Blocking decisions for a human. Empty when Status: approved.
```
