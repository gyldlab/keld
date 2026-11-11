# Spec: Explicit empty-stylesheet admission for installed original content
Status: proposed (exact independent permission review required before implementation)
Tracker: GH-553 / GH-1404 · Owner: Guard catalog and original-source composition · Updated: 2026-11-06

## 1. Goal and non-goals

Support a verified application that creates an empty stylesheet and populates it
through CSSOM (the browser's stylesheet object API). Preserve the existing strict
`original-editor-offline` policy byte-for-byte. Add an explicit, separately named
choice; never silently relax a previously selected policy.

Do not allow `unsafe-inline`, `unsafe-eval`, arbitrary policy text, remote origins,
nonempty inline style text, inline style attributes, inline scripts or a new native
principal. Do not patch draw.io or its spinner. This is not proof of complete editor
styling, iframe/print behavior, public release readiness or another platform.

## 2. Existing contracts and evidence

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

1. The existing strict selector and exact H2 response are unchanged. Unknown
   selectors, malformed policy and missing or ambiguous original-window mapping
   still fail closed. A mixed pair of old/new selected windows is ambiguous.
2. Only explicit manifest `original-editor-offline-empty-style` selects H2E below.
   Neither app network grants nor application arguments alter either response.
3. `ResolvedWindowCsp` still comes only from the digest-verified Guard snapshot.
   Core carries its actual selector, digest and window key into the existing
   metadata record. No caller string can install a response header.
4. The installed schema7 decoder accepts the new closed selector as data; the old
   developer schema5 retains only its prior strict selector. Missing, unknown or
   mixed data fail. Metadata itself does not grant content or IPC access.
5. Real native three-arm tests distinguish H2, H2E and an unsafe-inline diagnostic
   comparator. H2E admits the actual original empty spinner sheet and arbitrary
   rules inserted by already admitted external JS; it does not hash those rules.
6. Nonempty inline text, text appended after empty admission, inline attributes,
   inline scripts, remote script, foreign stylesheet links, foreign CSSOM imports
   and CSSOM remote-image requests remain denied. Exercise the newly admitted
   empty sheet in the H2E arm. Require a browser violation and zero forbidden
   requests/native-handler calls, with healthy same-origin controls.
7. Rebuild the exact Host, providers and signed app with the new explicit manifest.
   Run original editor workflows and negative controls on those bytes. A spinner
   mechanism pass alone cannot qualify the editor, its print path or all styles.

## 4. Design and rejected alternatives

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

## 5. Implementation boundaries

Guard `window_csp.rs`, `verified_manifest.rs` and their existing tests own policy.
Core installed source/facts carries the actual resolved selector. The canonical
KIPC launch-facts codec and generator own JS metadata validation; consumers reuse
the decoded record. The native handler already consumes `ResolvedWindowCsp` and
must not gain raw-header overrides. The original app's declared manifest changes
only its selected catalog name; source, preload, channels and other grants do not.

## 6. Ordered tasks

- [ ] Exact independent design approval and owning tracker record.
- [ ] Failure-first parser/catalog/mixed-ambiguity and schema7 controls.
- [ ] Small owner implementation, generated transport refresh and source review.
- [ ] Qualified fresh providers/Host, explicit manifest, signed original app and
  real workflow, including the required denied-resource and cleanup controls.

## 7. Failure-first validation

Keep old exact-header assertions. New selector must reject before implementation.
Its selected header must differ from H2 by exactly one hash token. Mutating that
token must fail spinner admission; replacing the empty hash with unsafe-inline
must fail the nonempty negative. Test zero, old-only, new-only, two-old, two-new and mixed mappings. A valid
signature cannot substitute for manifest provenance or actual resource denial.
Use the same canonical schema7 literal vectors in Rust/JS metadata tests; the
decoder's acceptance is not native authority. Preserve every native failed run.

## 8. Review gates

Permission model and public configuration: independent exact-source security and
architecture review. No new unsafe operation, dependency or wire channel is planned.
Do not score this as full GH-553/473 completion or dismiss existing review requests.

## 9. Performance

One cold catalog selection and one constant header; no new process, queue, runtime,
DOM interception or per-frame allocation. No performance improvement is claimed.

## 10. Remaining evidence and sources

The native extended resource-denial controls, complete installed editor, final
exact-source review, repository gates and second-Mac acceptance remain separate.
New failures in other style constructors must be traced and reviewed, not met with
an automatic broader inline allowance.

Primary semantic references: W3C CSP3 Working Draft 2026-10-27, sections 6.1.13 and
6.7.3.3; WebKit `InlineStyleSheetOwner.cpp` and `ContentSecurityPolicy.cpp`, commit
`7d1fa40fdbd3c998f5b84bb71dd58220d18e7820`. CSSOM behavior is qualified by actual
native results because the draft's CSSOM explanation is incomplete. Retrieved
2026-11-06. The build's paid signing, notarization and public trust remain out of scope.
