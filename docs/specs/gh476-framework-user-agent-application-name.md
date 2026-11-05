# Framework application name in the macOS WebView user agent
Status: approved for bounded implementation; native and independent review gates remain open
Linear: GH476/GH589 (#517 tracker) · Owner: repository owner through standing delegation · Updated: 2026-11-01

The user explicitly requires reusable framework implementation. This decision
supersedes the unadopted draw.io-specific suffix candidate. Approval is the user's
standing technical-owner delegation; it does not constitute runtime acceptance.

## 1. Goal & non-goals

Allow an application to supply optional, bounded user-agent application metadata
through its literal `keld.config.ts`. Apply it through the common macOS WebView
configuration factory, preserving the actual engine user agent and any existing
application-name prefix. Any application can use it, without an original preload,
admitted draw.io source, app-name comparison, or special resource path.

No full user-agent override, runtime setter, Electron/Chromium version fabrication,
page-global shim, renderer rewrite, permission, CSP change, or broader platform
support is introduced. A reference application may configure its own public
branding/compatibility text; framework code must contain no draw.io token.

## 2. Spec refs

Architecture01/05 backend ownership; architecture03 default-deny and immutable
admission; KEL96 closed boot descriptor; GH476 native capability ownership.
GH473 source confinement and GH452 role facts remain separate contracts.

## 3. Acceptance criteria

1. Absent metadata leaves previous config/descriptor bytes and native defaults
   unchanged. Explicit null, duplicate, nonliteral, empty, control/non-ASCII,
   leading/trailing-space, or greater-than-256-byte values refuse before resources.
2. A value contains 1..256 visible ASCII bytes with internal spaces permitted.
   One opaque WV value type owns validation; config and boot consumers reuse it.
3. The optional closed field `applicationNameForUserAgent` is supported by the
   existing boot families1..5. The producer omits absence. An older Host rejects
   the new field rather than ignoring it. Existing graph/policy/Host pairing is
   mandatory wherever already required; metadata cannot confer eligibility.
4. Producer and Host reject present metadata on unsupported non-macOS backends
   before child/window acquisition. Defaults remain usable on those platforms.
5. On macOS, default and two differently configured first-party applications
   demonstrate actual native UA values. Preserve the default engine portion and
   pre-existing application-name prefix; only the requested suffix differs.
6. Legacy/renderer and managed/facade paths use the same factory. No source/image,
   app-name, graph-member or draw.io predicate selects the capability.
7. Metadata changes no principal, source, document, profile, grant or channel
   decision. Missing policy and wrong-window/role denials remain effective.

## 4. Design

No handle/crash/principal boundary change. Core owns one cold config read and
retained validated descriptor; it supplies immutable data before engine creation.
WV exposes a documented opaque `ApplicationNameForUserAgent` value with one
validator and borrowed string access. Existing constructors select `None`; a
cold constructor accepting the optional value reuses the same implementation.
`MacProfileOwner` retains it alongside its existing store selection. It cannot
be changed after startup and is not delivered as a new Bun role fact.

The existing `configuration_for_view` is the sole native consumer for both view
creation paths. When metadata is present, it reads the existing configuration
application name and appends the requested suffix using public
`WKWebViewConfiguration.applicationNameForUserAgent` operations before WebView
creation. Preserve existing prefix bytes. Nil/default configuration remains
untouched when metadata is absent. Native exception or unsuccessful assignment
returns a typed error; no ignored value or fallback is success.

Reuse the root literal parser, strict boot DTOs, common native factory, existing
profile/store ownership and Wry configuration handoff. Reject `setCustomUserAgent`
because a full override discards actual engine information. Reject app-specific
suffixes and page globals. No kipc channel, opcode, functional message, principal,
transport or parser is added. This is an explicit optional cold-format extension;
strict older consumers refuse it and final boot-contract review is required.

Atoms: literal syntax/value bounds; retained cold provenance; single immutable
engine value; pre-WK native copy/lifetime; actual UA readback; authority
independence; platform refusal. Each has its own test. A parser pass cannot prove
native behavior; a UA string cannot prove identity or permission.

## 5. Boundaries

Core hello/config, boot DTO/selection and macOS engine construction; CLI boot
producer; WV engine metadata type/constructors and P profile configuration owner;
their tests and exact P-only unsafe-operation instruction amendment.
Do not change original webapp/preload, runtime facts, SDK facade, CSP, permissions,
dependency pins, graph providers, Wry delegates or other native operations.

## 6. Tasks

- [ ] Add conformance/regression tests before generic producer/consumer code.
- [ ] Implement the single validator and closed optional cold carrier.
- [ ] Amend only P's exact two native selectors within unchanged instruction caps.
- [ ] Implement shared pre-WK configuration with unchanged default constructors.
- [ ] Execute default/A/B/prefix and rejection tests, then independent final gates.
- [ ] Configure the reference app separately and run the real workflow.

## 7. Test plan

Literal/DTO tests include maximum/max+1, whitespace/control/non-ASCII, escaped
duplicate key, unknown field, null, nonliteral and old descriptor invariance.
Exercise the actual CLI staged descriptor and Host cold consumer. An old-Host
fixture must refuse presence. Test non-macOS admission as platform-unverified
until the actual applicable target runs; do not claim cross-OS acceptance.

Use real macOS first-party pages with no original source/preload to observe
default and distinct A/B UAs. Include pre-existing prefix preservation and both
creation paths. Remove the native assignment and alter the cold carrier as
separate fault controls; each must fail its owning observation. Preserve profile,
wrong-window/role and policy-denial controls. Compilation is not native proof.

## 8. Review gates triggered

Unsafe; public API; closed boot-format/architecture contract. No permission-model,
dependency addition, or kipc wire change. Exact owner instruction, independent
security/architecture evidence and physical native checks remain required.

## 9. Perf impact

One bounded cold value per engine and native string copying during configuration;
no steady-state IPC work, extra thread, queue or performance claim.

## 10. Open questions

None for the bounded contract. Actual native behavior, old-consumer refusal,
cross-platform checks and independent final review are implementation gates.
