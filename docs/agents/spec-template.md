# Spec template

Copy this into `docs/specs/<id>-<slug>.md` for any change bigger than a bug fix.
[`.agents/specs.md`](../../.agents/specs.md) owns the caps, the directory layout, IDs,
writing and citations. Keep every section; write "none" rather than deleting. A spec is
implementable when a fresh agent could build it without asking questions. Status is
draft, approved, implementing or done. Implementation may start only at `approved`
(human sign-off).

```markdown
# Spec: <name>

- Status: draft
- Tracker: GH-0
- Owner: <human>
- Updated: 2026-01-01
- Pin: 0000000000000000000000000000000000000000

## 1. Goal & non-goals
One paragraph: the problem and the observable outcome. Bullet the non-goals.

## 2. Spec refs
Governing `docs/architecture/*` sections. A deviation updates that doc in the same PR.

## 3. Acceptance criteria
### AC-1: <binary rule; a typed error states the fix>
- **Given:** <state>
- **When:** <action>
- **Then:** <observable result>
- **Negative control:**
  - **Single fault:** <one mutation>
  - **At-risk precondition:** <why the rule runs>
  - **Expected RED:** <what fails>

## 4. Design
- First-principles and reuse decision (architecture/public-contract/process/IPC/
  permission/lifecycle/performance work): ownership, trust, lifecycle, I/O, failure
  facts; options evaluated; the unmet requirement behind any rewrite; compatibility
  fallback or `not required`; a measured baseline for a performance claim. Else write
  `No boundary change`.
- Types and channels (real Rust signatures, `.k.ts` contracts); capabilities and
  manifest changes (spec 03); wire changes (spec 02: version bump + review gate);
  mac/win/linux differences. Write "none" where none.
- Runtime seam: execution owner, OS grants, crash domain, handle/callback lifetime,
  value/error/order semantics, config capture moment.
- Migration unit: callers, handlers, callbacks, generated contracts, persisted state;
  adapter owner/removal condition; permanent compat facade.

## 5. Boundaries
- Implement in: <crates/packages/files>
- Must not touch: <crates/files>

## 6. Tasks
Ordered; no placeholders.

### T-1: <vertical slice, about one PR>
- **Blocked by:** none

## 7. Test plan & evidence
Map each AC to a test. Note anti-flake concerns (timing, ports, platform-only paths).

### E-1: <measurement>
- **Run:** <command or link>
- **n:** <sample size>
- **Environment:** <OS, commit>
- **Limits:** <what it does not prove>

## 8. Review gates triggered
unsafe, public API, permission model, dependency, wire protocol (list or "none").

## 9. Perf impact
Budgets (architecture/01 §5) that could move, and the bench; or "none".

## 10. Open questions, blockers & unknowns
Q-n blocks a human decision; none remain at approval.

### Q-1: <decision>
<options and recommendation>

### B-1: <blocker>
- **Status:** open
- **Owner:** <who acts>
- **Impact:** <what stops>
- **Check:** <first observable completion check>

### U-1: <unknown>
<what is unknown and how to find it>
```
