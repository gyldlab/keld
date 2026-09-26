# Test layout and migration

Load for test placement/layout, oversized tests, discovery or acceptance refactors. [testing.md](testing.md) owns correctness and independent proof.

## Responsibilities and locality

- A scenario states its product contract and action/observation sequence.
  Keep launch, fault, recovery, quit and cleanup visible, not an `assert_everything`
  helper. Group by contract, not alphabet or length. Retain shipping-path seam proof.
- Fixtures construct projects, inputs, programs, servers and resources.
  Fixture checks validate prerequisites, never the product oracle. Extract substantial embedded programs into real nearby fixture files when
  useful. Keep tiny literals local and malformed input byte-exact.
- Observers report actual wire/error/status/filesystem/OS facts. They MUST NOT derive
  expected behavior from the implementation under test. Keep platform semantics local.
- Give every process, descriptor/HANDLE, socket, thread, channel, profile and temporary
  path an owner. Keep resource types with acquisition/finish/Drop.
  Observe and assert product cleanup before emergency harness cleanup; fallback cleanup
  MUST NOT erase a leak and then count as product success. Preserve unwind behavior.
- Scenarios depend on narrow fixture/observer modules, never the reverse. Use
  explicit imports and bounded visibility. No giant common harness, wildcard maze,
  empty mirrored hierarchy, wrapper registry or include-based pseudo-extraction.
- Start platform-local. Share only after multiple native migrations demonstrate the
  same semantics, owner, cleanup and failure behavior with named live consumers.
- For representative fixes, record the actual scenario/fixture/observer/production
  read set before and after. Fewer bytes alone do not prove better reasoning or speed.

## Extraction and discovery

Before moving code, record the baseline SHA and classify every
item: scenario, fixture, observer/support, helper entry, configuration or resource owner.
Map destinations/dependencies; check current main, open work and spec.

- Inventory compiled Cargo/libtest and nextest cases, exact names, ignore reasons and
  feature/cfg variants on the native OS. Source counts are not discovery proof.
- Preserve integration executable names and runner groups/filters/concurrency unless
  a separately justified contract change proves every consumer. Multiple source
  modules normally remain inside one existing integration target.
- Search subprocess `--exact` selectors, scripts, CI and operator references. Keep
  root helper entries when callers depend on their names. Map intentional
  scenario renames bijectively and update consumers in the same change.
- Re-list after each identity-changing move; compare omissions/additions/duplicates.
  Exercise exact helpers by an independent handshake/effect: exit zero can mean zero
  selected tests. A missing-selector negative control MUST fail the caller.
- Move one coherent family at a time; compile and run affected tests after each move.
  Preserve assertions, negative controls, deadlines, fixture bytes, environment capture,
  field/drop order and resource scopes.
- Separate moves, product fixes and deduplication; track defects under their owner. Never weaken a test or replace native behavior
  with a mock to make extraction pass. Run the testing/workflow gates on the final diff.
- Report actual native OS/session and unrun conditions. Roll back a move with its selector/import mapping; preserve newer
  regressions and evidence.

## Cohesion review triggers

Review an entry module over 100 physical lines **or** 4 KiB; review scenario/support
modules over 400 lines **or** 16 KiB. These are review triggers, not hard size limits.
Extract a real responsibility, or retain a reviewed exception naming owner, reason,
tracking issue and removal/review condition. A new regression MUST NEVER be rejected
because it crosses a trigger. Do not relocate a giant file into a giant support file
or fragment it for a counter. Automate demonstrated failures using compiled discovery
and exercised selectors.
