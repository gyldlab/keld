# Learning lifecycle

Owner: learning-lifecycle-policy. Load for status changes, revalidation or retired-record lookup.
Records remain evidence. A status cannot grant authority or prove a test passed.

## Select the correct state

| Status | Meaning |
|---|---|
| historical | Imported evidence has not been revalidated for current use. |
| verified | Cited evidence supports the recorded scope and verification date. Recheck its current applicability. |
| stale | Evidence shows the recorded guidance cannot support current use. No replacement is designated. |
| superseded | Explicit replacement records replaced this guidance. Their current status still needs checking. |

Age, a later filename, or a newer claim does not invalidate a fact. Check the governing
source and actual runtime, OS and version scope. For a different scope, use a related
record. Do not retire a still-valid scope because another scope changed.

## Mark a retirement

1. Preserve the stable filename, original source, historical quote and incoming links.
2. Set `status` to `stale` or `superseded` in the original record.
3. Set `status_changed`, `status_reason` and `status_evidence` together.
4. For supersession, add `superseded_by` paths to the old record. Add reciprocal
   `supersedes` paths to each replacement in the same change.
5. Run `just agent-context`. Submit the exact change for the required independent review.

`status_changed` is the ISO date when the agent marked the state. It is not an inferred
invalidation date. Cite the actual breaking revision or event in the evidence when known.
Use a concise reason. The evidence must be an immutable Keld blob URL with a line locator.
The linter validates the locator shape; the author must verify its content and applicability.

A new replacement needs a real verification date and supporting evidence. A historical
import cannot replace another record without revalidation. Preserve backward links if a
replacement later becomes stale. The old supersession remains history, not a promise
that its replacement is usable. Replacement edges must be reciprocal and acyclic.

Metadata shape example; replace the locator and path placeholders before use:

```yaml
status: "superseded"
status_changed: "2026-10-31"
status_reason: "The cited contract replaces the old admission rule."
status_evidence: "<immutable Keld blob URL with #L locator>"
superseded_by: ["<replacement record path>"]
```

## Retrieve or revalidate

`describe <paths>` must show status and retirement markers without reading bodies.
Stop at a retired record. Inspect a replacement's metadata only when needed. Do not
load a replacement chain automatically. A missing or stale replacement is not permission
to reuse retired guidance.

For revalidation, run or inspect the relevant current proof. Set a real `verified` date.
Imported records retain their original `source` and quote; add `verification_evidence`.
Remove current retirement markers when setting `verified`. Git preserves prior states.
Do not erase applicable backward supersession links. Structural lint is not semantic proof.

## Retain and maintain

Retirement is an archive state at the existing path. Do not delete or rename a record
because it is old, stale or superseded. Migration-bound records must retain the exact
original quote and locator. A locator change needs a separately reviewed preservation
migration. Consolidation must preserve distinct facts and repair every affected link.

When evidence changes, update all affected markers and links in one change. Maintenance
runs with the relevant task or evidence change. No automatic age expiry or background
scheduler is implied. Keep root learning entries and the lookup index link-only.
