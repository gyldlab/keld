# Spec: Explicit empty-stylesheet admission for installed original content
Status: draft
Tracker: GH-553
Owner: Guard catalog and original-source composition
Updated: 2026-11-19
Pin: d98675c1a3b2eabb19e3000705e178f84fd169b4

## 1. Goal & non-goals

Support a verified application that creates an empty stylesheet and populates it
through CSSOM (the browser's stylesheet object API). Preserve the existing strict
`original-editor-offline` policy byte-for-byte. Add an explicit, separately named
choice; never silently relax a previously selected policy.

Do not allow `unsafe-inline`, `unsafe-eval`, arbitrary policy text, remote origins,
nonempty inline style text, inline style attributes, inline scripts or a new native
principal. Do not patch draw.io or its spinner. This is not proof of complete editor
styling, iframe/print behavior, public release readiness or another platform.

## 2. Spec refs

The predecessor is GH-553's `gh553-verified-csp-admission-d1.md`, immutable source
`53593f96c9d1aa9518758cc065ae0ab56ccb6d6c`. Its original H2 policy remains unchanged.
GH-1404's approved installed graph/source and runtime-facts contracts retain exact
Host/provider qualification, explicit Legacy, verified manifest, current document,
fixed original preload, resource confinement and authenticated process ownership.

The pristine original app reached a real installed WebKit document but stopped at
`m.insertRule`. An independent native control using the original `spin.min.js`
reproduced that exact exception: an empty style element had no sheet under H2.
Adding only the SHA-256 hash of empty UTF-8 text admitted the sheet and the original
spinner. Nonempty text, later text mutation, style attributes, inline scripts and
foreign stylesheet loading remained refused. Same-origin external CSSOM rule
insertion was already possible under the old policy. This is measured macOS
26.5.1 / WebKit 21624.2.5.11.4 behavior, not a universal CSSOM promise.

Receipt: `finish-20261106-1847/native-css-control-v2/result.json`, native run
`a81ee8b5f0fa4f9d8680800b25cea2f8`. The v1 compile failure is preserved. The corrected
executed source is `native-css-control-v2/main.swift`; a provenance correction
identifies its exact path separately from the original worker source.

## 3. Acceptance criteria

### AC-1: preserve the existing strict selector and H2 response

- **Given:** The existing strict selector and exact H2 policy.
- **When:** Guard resolves a selected window policy.
- **Then:** H2 remains byte-identical. Unknown selectors, malformed policy and
  missing or ambiguous original-window mapping fail closed. A mixed pair of
  old/new selected windows is ambiguous.
- **Negative control:**
  - **Single fault:** Change H2 or allow an ambiguous selector mapping.
  - **At-risk precondition:** The original strict policy is resolved.
  - **Expected RED:** Exact-header or ambiguity tests fail.

### AC-2: only an explicit manifest selector chooses H2E

- **Given:** The explicit manifest value `original-editor-offline-empty-style`.
- **When:** The installed original policy is resolved.
- **Then:** Only that explicit value selects H2E. App network grants and
  application arguments do not alter either response.
- **Negative control:**
  - **Single fault:** Infer H2E from a network grant or application argument.
  - **At-risk precondition:** The manifest does not contain the explicit selector.
  - **Expected RED:** The resolver selects a policy without the required declaration.

### AC-3: the header comes from the digest-verified Guard snapshot

- **Given:** A digest-verified Guard snapshot and its resolved window policy.
- **When:** Core builds the existing metadata record.
- **Then:** Core carries the actual selector, digest and window key.
  No caller string can install a response header.
- **Negative control:**
  - **Single fault:** Allow a caller-provided string to choose the header.
  - **At-risk precondition:** A caller attempts to override the resolved policy.
  - **Expected RED:** The independent provenance check rejects the override.

### AC-4: schema admission remains closed and metadata is not authority

- **Given:** The installed schema7 decoder and old developer schema5 decoder.
- **When:** Each decodes the selected policy.
- **Then:** Schema7 accepts the new closed selector as data. Schema5 retains only
  its prior strict selector. Missing, unknown or mixed data fail. Metadata grants
  no content or IPC access.
- **Negative control:**
  - **Single fault:** Accept an unknown or mixed selector, or treat metadata as a grant.
  - **At-risk precondition:** The decoder receives unsupported policy data.
  - **Expected RED:** Closed-shape or no-authority tests fail.

### AC-5: native three-arm controls distinguish H2, H2E and unsafe-inline

- **Given:** The original empty spinner stylesheet and admitted external JavaScript.
- **When:** Native H2, H2E and unsafe-inline diagnostic controls run.
- **Then:** H2E admits the actual empty spinner sheet and arbitrary rules inserted
  by already admitted external JavaScript. Those later rules are not hash-bound.
- **Negative control:**
  - **Single fault:** Replace the empty hash with unsafe-inline or admit nonempty text.
  - **At-risk precondition:** The original spinner attempts CSSOM insertion.
  - **Expected RED:** The three-arm policy and nonempty-style controls fail.

### AC-6: all unrelated inline and remote resources remain denied

- **Given:** The H2E policy and an installed original document.
- **When:** Nonempty inline text, appended text, style attributes, inline scripts,
  remote scripts, foreign stylesheet links/imports and remote CSSOM images are attempted.
- **Then:** The browser reports violations and forbidden request/native-handler
  counts remain zero. Same-origin positive controls remain healthy.
- **Negative control:**
  - **Single fault:** Permit one forbidden resource class.
  - **At-risk precondition:** The installed document attempts that resource.
  - **Expected RED:** The browser violation or zero-request oracle fails.

### AC-7: qualify the exact installed Host and signed application

- **Given:** The exact Host, providers and signed app with the explicit manifest.
- **When:** Original editor workflows and negative controls run on those bytes.
- **Then:** Acceptance is based on the complete installed workflow.
  A spinner-mechanism pass alone does not qualify the editor, print path or all styles.
- **Negative control:**
  - **Single fault:** Use a development/test Host or omit the installed workflow.
  - **At-risk precondition:** The release candidate is presented for acceptance.
  - **Expected RED:** The installed-artifact provenance gate refuses qualification.

## 4. Design

The sole Guard catalog adds `OriginalEditorOfflineEmptyStyle`, serialized as
`original-editor-offline-empty-style`. Its exact header H2E is:

```text
default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self'; img-src 'self' data:; style-src 'self' 'sha256-47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU='; font-src 'self' data:; media-src 'self'; object-src 'none'; base-uri 'none'; frame-src 'none'; form-action 'none'
```

Use the measured `style-src` form, not an untested `style-src-elem` substitution.
There is no `unsafe-hashes` token, so the hash does not authorize style attributes.

This explicitly expands which stylesheet objects can be created. The empty hash
constrains their initial text, **not subsequent CSSOM rule text**. Existing admitted
same-origin JS can already populate an external same-origin stylesheet. HTML text
alone gains no arbitrary nonempty stylesheet, script or network destination. Every
resource request remains subject to the unchanged directives and native graph/root
checks. Legacy main-process ambient authority is not described as containment.

Extend the existing unique-key resolver through one private matching helper. The
new installed-original resolver accepts exactly these two catalog variants and
requires one matching key overall; never try one profile then silently fall back
after an ambiguity. Old callers selecting an exact profile keep their behavior.
Expose the actual immutable catalog selector from `ResolvedWindowCsp`, not its raw
constructor. Installed facts derive the selector from that resolved value; both
Guardian expectation and canonical JS structural validation retain the selected
value. No new boot version, KIPC field, trust service or resource parser is needed.

Rejected: unsafe-inline (admits arbitrary text); changing old H2 in place (changes
existing selected semantics); editing the original app/spinner (not framework
compatibility); intercepting DOM constructors (another implicit policy owner);
loosening source hashes or native resource checks (unrelated authority bypass).

## 5. Boundaries

Guard `window_csp.rs`, `verified_manifest.rs` and their existing tests own policy.
Core installed source/facts carries the actual resolved selector. The canonical
KIPC launch-facts codec and generator own JS metadata validation; consumers reuse
the decoded record. The native handler already consumes `ResolvedWindowCsp` and
must not gain raw-header overrides. The original app's declared manifest changes
only its selected catalog name; source, preload, channels and other grants do not.

## 6. Tasks

### T-1: approve the exact permission-model design

- **Blocked by:** none
- [ ] Obtain exact independent design approval and owning tracker record.

### T-2: implement failure-first parser and catalog controls

- **Blocked by:** T-1
- [ ] Add parser/catalog/mixed-ambiguity and schema7 controls.
- [ ] Implement the small owner-scoped change and refresh generated transport.

### T-3: review the source and generated transport

- **Blocked by:** T-2
- [ ] Complete exact-source and generated-transport review.

### T-4: qualify the installed original application

- **Blocked by:** T-3
- [ ] Use qualified fresh providers/Host, explicit manifest and signed original app.
- [ ] Run real workflows, denied-resource controls and cleanup controls.

## 7. Test plan & evidence

Keep old exact-header assertions. New selector must reject before implementation.
Its selected header must differ from H2 by exactly one hash token. Mutating that
token must fail spinner admission; replacing the empty hash with unsafe-inline
must fail the nonempty negative. Test zero, old-only, new-only, two-old, two-new and mixed mappings. A valid
signature cannot substitute for manifest provenance or actual resource denial.
Use the same canonical schema7 literal vectors in Rust/JS metadata tests; the
decoder's acceptance is not native authority. Preserve every native failed run.

## 8. Review gates triggered

Permission model and public configuration: independent exact-source security and
architecture review. No new unsafe operation, dependency or wire channel is planned.
Do not score this as full GH-553/473 completion or dismiss existing review requests.

## 9. Perf impact

One cold catalog selection and one constant header; no new process, queue, runtime,
DOM interception or per-frame allocation. No performance improvement is claimed.

## 10. Open questions, blockers & unknowns

### B-1: independent permission-model review is pending

- **Status:** open
- **Owner:** Guard catalog and original-source composition
- **Impact:** Implementation must not start until exact permission review approves
  the selected H2E semantics and no-authority boundary.
- **Check:** Obtain independent exact-source security and architecture review.

### B-2: native and installed acceptance is pending

- **Status:** open
- **Owner:** GH-1404 native acceptance owner
- **Impact:** Spinner success alone does not qualify the editor, print path or all styles.
- **Check:** Complete native extended resource-denial controls, installed editor
  workflow, exact-source review, repository gates and second-Mac acceptance.
  New failures in other style constructors must be traced, not met with a broader
  inline allowance.

Primary semantic references: W3C CSP3 Working Draft 2026-10-27, sections 6.1.13 and
6.7.3.3; WebKit `InlineStyleSheetOwner.cpp` and `ContentSecurityPolicy.cpp`, commit
`7d1fa40fdbd3c998f5b84bb71dd58220d18e7820`. CSSOM behavior is qualified by actual
native results because the draft's CSSOM explanation is incomplete. Retrieved
2026-11-06. The build's paid signing, notarization and public trust remain out of scope.
