# GH-1404: original draw.io argv compatibility candidate

This is a **draft source-only migration patch proposal**. It does not run in KELD or modify any packaged original, Host, or source owner checkout. The actual approved migrated `src/main/electron.js`, complete source project, signed graph and independent source/license review are missing in the [GH-1404 owner request](https://github.com/gyldlab/keld/issues/1404#issuecomment-6818891971). Do not merge or ship this tooling as proof of a repaired app.

The current signed bootstrap in [#1504](https://github.com/gyldlab/keld/pull/1504) authenticates Host Init and checks its selected original's physical identity before importing it. Real Bun arguments remain `[bun, bootstrap, original]`, and `process.defaultApp` is `true` in the installed role. Pristine draw.io `electron.js` forwards `argv.slice(2)` through `args.js`, treating the internal `original` path as a user diagram and conditionally calling `blessPath`.

`patch-original-main.mjs` produces a candidate **in-memory** edit to the exact legacy parser seam. It never chooses, opens or edits any source file itself. The migration must obtain a separately independently approved source SHA and hand-apply/review the two injected source locations:

- Import the fixed `@keld/kipc` Host launch facts, captured in the signed installed graph, next to the existing args import.
- Create a *separate parser-only array* only when the independently captured runtime-selection bit is **true** and the closed installed-original Host facts match schema 7, `local-development`, and the original-resource-policy contract. Missing/mismatched facts **throw `KELD-IPC-005` before `parseDrawioArgs`**; never reduce the two independent inputs to one boolean that silently falls through to upstream user-file parsing. The inverse mismatch (schema-7 original facts with runtime selection false) also refuses. These facts are decoded by KIPC and do **not** themselves authorize file access.
- The selected parser array's `slice(2)` is empty. The signed bootstrap admits **exactly three CLI arguments**; any additional path, including a real `.drawio`, is refused instead of becoming a filesystem grant. The original native Open, file association, and picker flows remain their separate OS-intent paths.

All branches preserve actual `process.argv`, `Bun.main`, `import.meta.main`, authenticated app link, Guard admission, ACK/Ready and the existing 5,000 ms I/O deadline. The non-installed upstream parser convention is retained with a copied array.

## Source-only checks

`node --test tools/gh1404-original-argv/argv-compat.test.mjs` runs the self-contained shape, refusal, immutable-argv and exact-hunk tests. Supplying independently pinned read-only inputs additionally runs the real draw.io 31.7.0 parser/source controls:

```sh
GH1404_PINNED_ORIGINAL_MAIN=/approved/source/src/main/electron.js \
GH1404_PINNED_ORIGINAL_ARGS=/approved/source/src/main/args.js \
node --test tools/gh1404-original-argv/argv-compat.test.mjs
```

The pinned pristine test oracle is `electron.js` SHA-256 `55505e690d37070344be172a68ad997ab48c687302dc5dee30a988d4f153770d` and `args.js` Git blob `e5ab92e4468fc80d888f953945b60a24e20c8b06`. The selected-facts positive uses KIPC's existing schema-7 fixture. Tests cover selected-facts missing/mismatch and unknown runtime selection with `defaultApp` true/false/undefined, zero or one CLI diagram, and refusal before the real pinned parser can consider a path for blessing. The worker16 fail-open regression was **RED: six new test cases failed on the previous implementation**, with absent/mismatched facts silently falling back to upstream parsing. After the explicit selection/facts guard, the new and existing cases passed. A migrated source will have a different SHA; the application owner must independently pin its revised content, reconcile this patch against existing migration modifications, review import closure, and validate the *actual* `args-obj`/file-blessing effects. A missing pinned original-source test input is reported as a skipped test, never a green original-source oracle.

Before any integration: original migration owner @0monish and an independent maintainer must decide exact source/policy/graph pins and whether to approve this bounded parser change. A genuine one-file `.drawio` open must be tested through its actual Host-admitted OS/file-picker route; passing a fourth CLI token is currently refused. Run a **newly source-built signed app** with defaultApp true/false/undefined controlled conditions, no incidental grants, owner-approved source and full original save/recovery/native rejection gates. No native or signed app acceptance is claimed here.
