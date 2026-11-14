# GH-1184 X10/X11 scoped code handoff

This branch publishes the actual reviewed code delta for the X10/X11 Windows native regression scope, as a unified patch. It intentionally does not commit or republish the unrelated, currently uncommitted W1 foundation.

- Scope: 86 source paths (40 modified and 46 newly added).
- owned-source.patch: source changes only relative to the frozen W1 preimages.
- manifest.json: each owned path, exact preimage and corrected-content SHA-256.
- Foundation source digest: 7df119d6379cb2b8c47655da71bed9e9db0f5f9bd525085d670858175ba23095.
- Qualified final source digest: 5a880f571cba07a1d7050cbccb9644cfec0f6d24b91c3b3ea9003574ff4850c5.
- Native verification: all three navigation cases and all six final X10/X11 cases passed, with healthy follow-ups, independent raw review, and a deliberate guard-failure/restored replay.
- Full evidence and integration receipt: https://github.com/gyldlab/keld/issues/1184#issuecomment-6865140498

## Integration requirement

The W1 base is not committed as an approved standalone branch. The patch preimages differ from main and from the old W1 committed HEAD, so do not apply it to main directly or merge this carrier branch as production code. The W1 integration owner must first reconcile and publish its approved foundation, then apply these exact hunks against their verified preimages and qualify the resulting combined source. Do not stage unrelated W1 files.

This branch changes no product code by itself and is a pushable, reviewable code handoff, not a shipping approval. AC06, signed default Host/CLI, draw.io W01-W10 and final CI remain separate gates.
