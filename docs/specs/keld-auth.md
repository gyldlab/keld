# Spec: keld-auth (OS-broker authentication capability)

Status: approved
Linear: KEL-89 · Owner: GYLDLAB · Updated: 2026-09-09

**This file does not testify to its own review status.** Whether the merge and
review gates are satisfied for the exact bytes you are reading is recorded on
PR #195 and KEL-89, and nowhere here — a file cannot be evidence about a review
of itself, and two earlier revisions of this header that tried to be were
themselves review findings. Treat it as not merge-ready unless that external
record says otherwise for this revision.

Promoted from the private research note
`docs/research/library/auth-distribution/228-keld-auth-spec-draft-v2.md`
(`0monish/keld-research` commit `a7d177e`, content commit `ad7e493`, git blob
`9eab65c0bb81f2bd8b926f450045284ea988280d`, raw-bytes sha256
`27fca6cbd4b0c6529658344e86e2aa01768ba5871ced885df6faba18dce5bd57`, 164,909
bytes), §4.1–§4.12. Superseded pins this file carried earlier on the same
branch: `73f62ec`/`5fd87d5b…`, `30c706d`/`f8f53fa7…`, `60d2a24`/`63905ba9…`,
`109d9649`/`f9510eaa…`, `e8177dd`/`0c2bba1f…`, `e9191c4`/`72949e50…`, `8edddc5`/`c12dcf4d…`, `9d574c3`/`ed4676d0…`,
`bd7728e`/`52e68122…`. The note pins Keld at
`c6e14f13…` for its 2026-09-01 facts, `576aaca2…` for the Linux
re-verification, and `bf78a10d…` for the URL-glob matcher evidence it executed.
That evidence is **historical**: KEL-208 landed as PR #203 (`fa1bfd7`), which is
an ancestor of this PR's base. Executed against the shipped guard at that base,
the three rows whose grants name **no destination** now deny, and the
origin-rooted `https://api.myapp.com/**` row is **unchanged and still Allows** —
§4 enumerates them individually. An earlier revision of this header said every
Allow row now denies; that was inferred rather than run, and was false.
AC9b's predicate is unchanged and still narrower than the shipped rule, because
#203 supplies no control-character rejection and AC13(a)'s sentinel depends on
that half. The note's own `revision:` field owns the
errata history; it is not repeated here.

Decision records, all on KEL-89, all revocable, each written by the executing
agent under the owner's recorded delegation:
`linear-comment:f2178f64-6f30-4431-8906-271b7e7ce458` (D1–D10),
`linear-comment:9498d983-5c21-4979-bd4e-5fc6c582d601` (D11),
`linear-comment:973acac2-2461-46f6-886e-6a2ebeb77b4b` (D12–D16),
`linear-comment:413725f1-3b9e-449c-816a-08f72c96e4eb` (D17), and
`linear-comment:68410a82-52db-438a-a6ac-17420010aeaf` (D18).
**Owner ratifications from the owner's own Linear identity**
(`linear-user:49ccfebb-c3fb-40a3-abb2-a3bf92e83cb1`, `Amisha Ramani`):
`linear-comment:38dab27a-4a32-4791-bb41-7189c646d7d6` (2026-09-09T11:53Z, D11
"and the standing delegation it extends") and
`linear-comment:e27751b8-dd3d-4fec-ab62-81e0d2546186` (2026-09-09T16:25Z,
D12–D16). D17 and D18 were recorded after both, under that same ratified
delegation.

Approval of the promotion is a separate record from the decisions.
**Approved blob: `9eab65c0bb81f2bd8b926f450045284ea988280d`** — stated here so it
can be checked against the pin above rather than taken on trust — approved in
`linear-comment:be72ba9e-30ff-4ae8-a808-99cce405a2e3`. It supersedes
`linear-comment:3568b4fa-2259-405b-b057-865f526c8510` (blob `52e68122…`,
withdrawn in `linear-comment:9a5f80ba-acaa-42ba-93cd-d21916c71294` for a false
claim of executed evidence),
`linear-comment:b1377181-e5f8-4bf7-891b-42d97c73f710` (blob `ed4676d0…`),
`linear-comment:3759af34-755a-4218-8d00-bc848fe4e2f4` (blob `c12dcf4d…`),
`linear-comment:40e40b76-f952-48af-bb1b-8009d642dc72` (blob `72949e50…`) and
`linear-comment:938da4f8-5790-442d-b50f-06dec97ff296` (blob `0c2bba1f…`, withdrawn
in `linear-comment:f1baac66-e72b-4f71-a480-a217e0cbae65` after a review found a
false claim in it), plus the older `bf6cc36b…` / `99ba1903…`. Three successive
revisions of this header cited an approval record for a blob the file no longer
carried, which is why the blob is now named in the sentence that approves it. `approver_identity`:
`linear-user:d7f711ff-c049-4ddd-a1f3-9f41b461b5c1` (`GYLDLAB`) by delegation.
`approval_mode`: `explicit-user-delegation-recorded-by-executing-agent` — the
promotion approval is agent-recorded under a delegation the owner ratified from
their own identity, and is **not** an owner-identity attestation of this blob.
The owner has not attested this blob. **Independent-human review of this file
is not claimed**; §6 T1's "review this note" and §8's "owner sign-off or the
recorded delegated-approval mode" are discharged by that delegated record.
Permission-model and Public-API gate evidence lives on PR #195 and KEL-89; each
implement PR re-reviews its own diff (§8).

Promotion transforms — mechanical only; no normative body text was reworded.
Note §4.1–§4.9 and §4.12 became `## 1.`–`## 10.`, with the §3, §6, §8 and §9
headings retitled to `docs/agents/spec-template.md`'s exact wording; §4.10 and
§4.11 are carried verbatim as §4's last two subsections; **internal** `§4.N`
references became the section that carries them (`§4.10`/`§4.11` → `§4`,
`§4.12` → `§10`, the rest → `§N`, which is why `§4` can appear twice in one
rendered list); research-relative links became cited paths with the label
preserved; the draft status line became this header; T1 is checked because this
landing is T1.

A `§4.N` belonging to a citation of **another** note — `241 §4.2`,
`[241](./241-…md) §4.2`, the chain `241 §4.3/§4.4`, and one instance wrapped
across a line break — names that note's subsection and is **not** renumbered.
The transform used by this branch's earlier promotions lacked that exclusion
and silently rewrote ten such references on nine lines, pointing the `[fact]`
provenance for the WAM GO verdict, the `entra-common` profile row and
`KELD-AUTH-014` at note 241's scope statement and pin ledger instead of its
findings. Every span the rule protects here was audited to be a genuine
note-241 citation.

How the transform is validated, and what that does **not** prove. Regenerating
an earlier promoted file from its own blob under the earlier rule reproduces it
byte for byte — which proves the rule was recovered, not that it was right, and
it was not: it reproduced the citation defect above, which a line-oriented
residual metric cannot see, because the metric is parameterised by the rule
under test. Three controls run instead of one: **recovery** (old rule
reproduces the old file byte for byte), **correctness** (the corrected rule's
delta is enumerated and every changed site audited), and **mutation** (flipping
one word of one promoted line makes the checker report `RESIDUAL: 1` and name
that line, so the metric is not vacuous). On the corrected rule this body has
`RESIDUAL: 0` against blob `9eab65c0…` over 1,506 non-heading lines, with no
promoted line lacking a note source.

Bare `§N` inside note §4 that pointed at one of the *note's* own top-level
sections was disambiguated in the note itself. A mechanical checker now enforces
that class, because a later hand pass re-introduced one after the first sweep
removed them all. Every remaining `§N` is preceded by a document, note or
standard citation (`02-ipc.md` §2, `226` §9, `241` §7, `note 228` §3/§5/§6/§7/§9,
`kel102-host-guard-enforcement.md` §4, RFC 8252 §8.12, RFC 6749 §3.3, OpenID
Connect Core 1.0 §3.1.2.1) and means that document's section.

## 1. Goal & non-goals

Give a Keld app a host-owned, default-deny authentication capability: a supervised
Bun child with an explicit `auth.*` grant can start an interactive OAuth/OIDC
public-client flow and later obtain a short-lived access token, while the host owns
every OS broker handle, redirect listener, PKCE verifier, and refresh token.
Observable outcome: on grant, `auth.begin` runs the OS broker or an RFC
8252-compliant external-user-agent fallback; on success the child receives at most
`{ account_id, granted_scopes, access_token, expires_at, token_type }` (the union
across the two reply shapes) — never a refresh token.

Non-goals (v0):

- Not a fifth unique — composition of prebuilt host, supervised Bun with zero
  ambient authority, kipc, and generated host-enforced default-deny (root
  `AGENTS.md` four uniques; 226 F-checked).
- Not required for Phase 2; MUST NOT block window/kipc/crate-map work.
- No embedded-webview IdP login surface (Google server-enforced ban; RFC 8252 §8.12).
- No OS-containment claim on any OS: `unverified` is the live state of macOS,
  Windows, and Linux (`docs/specs/kel78-strict-profile-sandbox.md:198`), Windows
  LPAC has zero production callers, and Job/guardian reaping is supervisor
  cleanup, not containment. (Rewritten per 226 K1 — the old "T2–T4 not
  implemented" premise is dead; the non-claim survives on these grounds.)
- No device-compliance Conditional Access claim: unsupported on macOS/Linux;
  Windows conditional on the WAM spike (D6, 226 F11).
- No WAM for third-party IdPs; no invented Linux auth portal.
- Deferred, names reserved (D4; 226 F32): `auth.accounts`, `auth.signout`,
  `AuthEvent::AccountChanged` — out of the v0 surface; names reserved in §4
  (field sketch recorded for `AccountChanged` only; non-normative) so the future
  API cannot drift.
- Future/destination tiers recorded, not promised: opaque host-attached
  credential handle (D2, future tier); macOS Enterprise-SSO broker-aware routing
  (D6, destination tier); packaging emitters in keld-pack (KEL-19);
  **per-account authorization for `auth.token`** (D11 destination tier — returns
  when durable account identity exists, behind the `secrets` fast-follow and
  alongside D4's reserved `auth.accounts`/`auth.signout`). Durable
  refresh custody via a `secrets` module is the named **fast-follow tier** (D5),
  scheduled behind its own spec rather than merely aspirational.

## 2. Spec refs

- Approved Keld specs @ `c6e14f1`: `docs/specs/kel96-no-flag-host-boot.md` (host
  owns event loop/windows/app-link; diagnostics unprivileged),
  `kel102-host-guard-enforcement.md` (verified snapshot; `evaluate` +
  `dispatch_privileged` only; T2→T3→T4→T5 order; V0AppLink→AppProcess),
  `kel101-windows-named-pipe-dacl.md` (Windows app-link transport),
  `kel75-principalized-bun-child-roles.md`, `kel78-strict-profile-sandbox.md`.
- Architecture: `02-ipc.md` §§1-2, `03-security.md` §§1-3,
  `05-webview-and-native.md` §1/§3 (native modules incl. `deeplink`, `secrets`
  destinations), `06-runtime-and-tooling.md` §3, and `07-agent-experience.md` §2,
  which owns the framework-wide error contract that §4's `KELD-AUTH-*` table
  applies — `crates/keld-ipc/src/call_error.rs:34-38` cites the same section for
  the code-first, imperative-fix message shape. §4 specifies which conditions
  get codes; `07-agent-experience.md` §2 and its registry own everything else
about them.
- Research: 226 (`docs/research/library/auth-distribution/226-keld-auth-decision-refresh.md`) (fact matrix, trust graph,
  findings ledger), 170 (`docs/research/library/auth-distribution/170-keld-auth-spec-draft.md`) (superseded draft),
  192 (`docs/research/library/auth-distribution/192-auth-walls-contradiction-refresh.md`) (Q2/Q3/Q5 stand; **Q1 rejected**
  — 226 §9), 167/168/169/175/176.
- If promoted, the same PR MUST update the architecture module/capability tables
  (ground-truth rule, root `AGENTS.md`).

## 3. Acceptance criteria (binary, each becomes a test)

1. **AC1 (deny before side effects).** Given a manifest with no `app.auth.begin`
   grant, when the AppProcess child CALLs `auth.begin` through the KEL-102/T3
   admission path (`TrustedCaller::V0AppLink → Principal::AppProcess`,
   `dispatch_privileged` before handler), then the reply is `DenyReason::NotGranted`
   with `KELD-GUARD*` fix text and no broker session, scheme side effect, or
   loopback listener was started. (Sequencing: testable end-to-end only after
   KEL-102/T3 lands — declared predecessor edge.)
2. **AC2 (refresh never on wire).** Given a flow that yielded a refresh token,
   when the child CALLs `auth.token`, the REPLY encoding contains no
   refresh-token field; refresh material remains host-custodied.
3. **AC3 (no embedded UA).** A request for an in-webview/embedded-user-agent
   authorization presentation is rejected with a typed `KELD-AUTH*` error naming
   the fix (broker/external user agent); no `keld-wv` navigation is a supported
   IdP login surface.
4. **AC4 (PKCE).** Every non-WAM interactive code-flow rung sends PKCE S256.
5. **AC5a (loopback default form).** The default loopback rung binds
   `http://127.0.0.1:{port}` with a dynamic port (bind 0) and uses that literal in
   the authorization request; it MUST NOT emit a `localhost` redirect by default.
   `[::1]` is optional and per-IdP gated (Entra: documented unsupported).
   Per-IdP profile exception (D7): a provider profile MAY select the `localhost`
   form where the IdP's documented port-flexibility guarantee is scoped to
   `localhost` (Entra), and this selection is a recorded field of that provider
   profile (§4 — a host-side table in v0), not an inline special case at the
   call site.
6. **AC5b (exact-literal mode).** An opt-in per-provider registration mode uses
   one exact registered redirect URI with a fixed port; in this mode bind-port-0
   is a typed configuration error (`KELD-AUTH-003`), not a fallback. The mode is
   selected by `fixedRedirect` in that profile's `keld.config.ts` `auth` entry
   (D14) — the operator who performed the registration is the only party that
   knows a fixed redirect was registered. `KELD-AUTH-003` fires at **request
   construction**, when `fixedRedirect` is set and there is no fixed port to use;
   once a socket has been attempted the code is `KELD-AUTH-004` instead (D18(b)).
   Errata pass 8 aligned this sentence with the code table and §7's tiebreak row,
   which pass 7 left it contradicting. Operator documentation MUST
   record that Entra's portal refuses http loopback URIs and requires a
   `replyUrlsWithType` manifest edit.
7. **AC6 (WAM ownership).** On the Windows WAM rung (conditional on the T2 spike
   go, D1), the host supplies its top-level `HWND` to
   `IWebAuthenticationCoreManagerInterop::RequestTokenForWindowAsync` (or
   documented sibling) and retains the COM/WinRT async state; the child never
   receives the handle.
8. **AC7 (ASWebAuthenticationSession ownership).** On the macOS rung the host
   retains the session and the presentation-context provider (weakly-held by the
   API — host holds both strongly) and returns its AppKit `NSWindow` on the main
   thread; the child owns neither.
9. **AC7b (device-compliance deny).** Given macOS and a flow that requires Entra
   device-compliance Conditional Access, the host returns a typed `KELD-AUTH*`
   error naming Intune Company Portal / Enterprise SSO as the missing broker; it
   does not silently attempt the ASWebAuthenticationSession chain for that policy
   class. (D6: v0 non-claim; WPJ certificate is broker-exclusive per Microsoft.)
10. **AC8 (Linux honest path).** On Linux the host claims no portal auth broker;
    the interactive path is external user agent (portal `OpenURI` or an
    equivalent reviewed host launch) plus host-owned loopback completion. Custom
    scheme is opt-in only where the IdP documents support — for Google targets
    the scheme rung is skipped: the primary page carries the callout "Important: Custom URI schemes are
    no longer supported due to the risk of app impersonation." (verbatim incl. callout label; fetched
    2026-08-31; the page separately carries an Android/Chrome-scoped sentence —
    recorded inconsistency, no desktop-affirmative text exists; 226 §5 G1).
11. **AC9 (scope isolation).** A `begin` for issuer Q under a grant scoped to
    issuer P ≠ Q denies with `DenyReason::OutOfScope` and fix text naming the
    manifest pointer.
12. **AC9b (auth grants are well-formed issuer literals).** A manifest whose
    `app.auth.*` grant list contains any entry containing `**` (a bare `"**"`
    included) **or any Unicode control character** (`U+0000`–`U+001F`,
    `U+007F`, `U+0080`–`U+009F`) is rejected at manifest load with a typed
    error. The control-character half is D13 (2026-09-09) and it is what makes
    AC13(a)'s sentinel unauthorable: `{"app":{"auth":{"token":["\u0000unresolved"]}}}`
    parsed and Allowed every unresolved id before it, reproduced by execution.
    It is the same rule on the same load path with the same `ManifestError`
    variant — a widened predicate, not a second surface. Rationale (D3): the shared matcher applies
    prefix-glob semantics to a `/**` suffix, so
    `"https://**"` would otherwise allow every https issuer (226 F34);
    auth grants are exact string literals — issuer URLs for **both**
    `auth.begin` and `auth.token` (D11 errata, 2026-09-09; the original
    `acct:<id>` form was unauthorable before login — §4 D11).
13. **AC10 (crash boundary).** If the host process terminates mid-flow, the PKCE
    verifier, OAuth `state`, and pending loopback listener are gone with the
    process (ordinary OS teardown — OS-agnostic; not a containment or
    supervisor-cleanup claim; 226 F22 kept this unchanged).
14. **AC11 (single-flight).** Per `account_id` **and identical requested scope
    set**, at most one refresh/token exchange is in flight; a concurrent
    `auth.token` for the same account and the same scope set awaits the
    in-flight result, while a distinct scope set is its own exchange — a
    coalesced reply never carries a token minted for a different scope set.
    **The key is canonical, and D17 (errata pass 6) defines it**, because
    "identical scope set" was previously undefined and AC11 is in T3's task
    list. The single-flight key is `(account_id, canonical_scopes)` where
    `canonical_scopes` is the requested `scopes` as a **set**: duplicates
    collapsed, order ignored, values compared **case-sensitively** byte for
    byte, and an absent `scopes` field equal to an explicitly empty list. Order
    and duplicates are dropped on the authority of RFC 6749 §3.3 — "their order
    does not matter, and each string adds an additional access range to the
    requested scope" — which is also why case is *not* folded: the same
    sentence makes the strings case-sensitive. That RFC defines no equality
    relation of its own, so this rule is Keld's, stated rather than assumed.
    The guarantee is deliberately one-directional: coalescing two requests that
    are **not** canonically equal is a defect, coalescing fewer than it could
    is merely a missed optimisation, so an implementation that is stricter than
    this rule still satisfies AC11 and one that is looser does not.
    A failed or consumed exchange is never automatically
    retried. (226 F4 narrowed form, 226 §11 item 5; motivated by rotation-enabled configs — Auth0
    third-party-native default-on, Okta operator-enabled with 0-60s grace.)
15. **AC12 (cancel semantics).** `canceledLogin`-class results map to a
    non-terminal typed `KELD-AUTH*` error whose fix text offers user-initiated
    retry and MAY include a defect-neutral default-browser hint (spec-author addition beyond D9); the host never auto-retries, and the error
    text asserts no Apple defect (the Sequoia third-party-default-browser
    anomaly remains an unconfirmed [lead]; D9).
16. **AC13 (token scope is the declared issuer; account id is a bound runtime
    selector).** D11 errata (2026-09-09). The matched string is the session's
    **declared issuer literal** as defined in §4 "Issuer literal" — the
    provider profile's configured value, never the IdP-returned `iss` claim.
    Four binary parts, in this order — (a)–(d) below, matching §7's rows
    13a–13d (errata pass 6 renamed the third from (b′) to (c); pass 7 corrects
    the count, which had said three since the note carried only three):
    (a) **Resolution (side-effect free, never itself a reply).** The host
    resolves `account_id` against the auth session registry (§4). This is a
    read of host state; it starts no broker session, listener or network
    exchange. If the id resolves to a **live** session (§4 defines live), the
    resource for step (b) is that session's declared issuer literal. If it does
    **not** resolve, the resource for step (b) is the reserved sentinel
    `\u{0}unresolved`. **D13 (2026-09-09) is what makes that sentinel
    unauthorable**: AC9b's predicate now rejects any control character in an
    `app.auth.*` entry, not only `**`. Before D13 the manifest text
    `{"app":{"auth":{"token":["\u0000unresolved"]}}}` parsed and Allowed every
    unresolved id — reproduced by execution against the shipped guard, and the
    reason the earlier "a value the manifest can never contain" claim was
    false. **AC9b is a load-time rejection, not a type invariant** (errata
    pass 7; **mechanism corrected in pass 8**): the bypass exists because
    `PermissionsManifest` derives `Deserialize` **publicly** while AC9b's check
    lives in `parse_manifest_at`, which a direct `serde` deserialize never
    enters. It is **not** about `deny_unknown_fields`, as pass 7 claimed: that
    attribute constrains a struct's own field names and cannot reach inside
    `app.auth.token`, so adding it changes nothing — reproduced by execution on
    a replica. An implementer who "restores the type invariant" that way has
    restored nothing, and code that deserializes straight into the type still
    bypasses the check — §7's 9b negative control depends on exactly that
    bypass. AC13(d) is the defence that does not depend on the load path.
    Resolution never produces a reply of its own.
    (b) **Scope — the one and only decision.** The host calls
    `dispatch_privileged(.., "auth.token", <resource from (a)>, ..)` exactly
    once. Which denial arises is the shipped guard's, not this spec's, and
    errata pass 7 states it exactly because §7 row 13b depends on hitting the
    right arm: `evaluate` returns **`NotGranted`** when the grant node is
    absent, is not an array, is an **empty** array, or holds any non-string
    element — all before scope matching
    (the four pre-scope-match `deny_not_granted` arms in `evaluate`,
    `crates/keld-guard/src/lib.rs`, verified by execution). **Cited by item, not
    by line number** (errata pass 11): pass 7 cited `513-519`, pass 8 corrected
    it to `517-525`, and PR #203 then moved the arms to `535-543` by inserting
    above them. A line pin into a file other PRs edit goes stale on someone
    else's schedule, so this note no longer keeps one here — and
    **`OutOfScope`** only for a present, non-empty, all-string list that does
    not contain the resource. A fixture written as
    `{"app":{"auth":{"token":[]}}}` to mean "granted but not at P" therefore
    exercises the *ungranted* arm; row 13b must use a non-empty list naming a
    different issuer literal. A grant containing the declared issuer
    literal — authored before any login, with no wildcard and no control
    character — Allows a real post-login `auth.token`, including when the IdP's
    returned `iss` property **differs from the declared issuer literal**
    (241's raw bundle, `run-04-operator-tenant-success.log:309`, records exactly
    that divergence for an Entra `common` profile; §4 "Issuer literal" carries
    the precise citation).
    (c) **Deny rendering — the resource is never echoed (D12, 2026-09-09).**
    On `Decision::Deny` the broker does **not** forward the `DenyReason`
    verbatim. It builds
    `CallError { code: reason.code(), message: <a constant for that code> }`,
    where the constant **begins with that code** — `CallError`'s `Display` and
    `02-ipc.md` §2 both assume `message` leads with it — then names the
    capability and the manifest key to edit, and contains **no** requested
    resource, no scope list and no account id. Exactly two constants exist,
    for `KELD-GUARD001` and `KELD-GUARD002`; `evaluate`'s other reachable
    variant, `NotAppProcess`/`KELD-GUARD006`, cannot occur here because v0 has a
    single `AppProcess` principal, and if T5 makes it reachable it needs its own
    constant before `auth.token` ships under role generations. This is
    the whole fix for the disclosure defect, and it is required: `DenyReason`'s
    own `Display`/`fix()` interpolate `requested` and `scope`
    (`crates/keld-guard/src/lib.rs`, `DenyReason::fix` and `Display`), and
    `impl From<&DenyReason> for CallError` copies `reason.to_string()` to the child
    (`crates/keld-ipc/src/call_error.rs`), so any design that forwards the guard's
    rendering leaks the very string this criterion protects.
    **Why the sentinel, and why (c) is the half that actually closes it.**
    The sentinel gives one guard call for both cases, so nothing is Allowed
    except through it. (c) then makes the two replies indistinguishable: an
    **ungranted** caller sees `KELD-GUARD001` with a constant message whatever
    id it sends, and a caller granted issuer P sees `KELD-GUARD002` with a
    constant message both for a live session at issuer Q ≠ P and for an id that
    resolves to nothing. Within a grant level the reply is byte-identical, so
    `auth.token` is not an account-existence oracle, and — the defect (c) also
    fixes — a caller granted P can no longer learn *which* issuer an
    out-of-scope live session belongs to. The sentinel alone never achieved
    this; three earlier revisions asserted it did.
    The enforced predicate remains *liveness in this host session*: ids are
    host-minted and opaque (§4), and v0 has exactly one `AppProcess`
    principal (§4 Principal), so fabricated, guessed, previous-session and
    cross-principal ids are one indistinguishable failure class. Per-principal
    separation activates with KEL-102/T5 role generations and is **not** claimed
    here (§4). Stated plainly because KEL-75 named roles are a shipped
    concept and the consequence is operational (errata pass 7): every supervised
    child mapping to `Principal::AppProcess` shares **one** auth session
    registry, so a compat-role child can tokenize a session the main child
    established. That is a property of v0's single principal, not of the
    registry, and it changes with role generations.
    **Boundaries this does and does not move.** Unchanged **by AC13**:
    `keld-guard`'s API — AC13 adds none, so §5's boundary holds for this
    criterion. (AC9b/D13 is the exception and is not AC13's: it adds a
    `ManifestError` variant, a breaking change to a public
    non-`#[non_exhaustive]` enum, gated in §8 and listed in §5's
    implement-in set. Errata pass 7 qualifies a sentence that previously read as
    a claim about the whole spec.) Also unchanged: the postcard `CallError`
    encoding, so its "MUST NOT invent a per-channel `Err` encoding" rule is
    preserved; and the code **vocabulary**, which stays `keld-guard`'s
    `KELD-GUARD001` / `KELD-GUARD002` rather than minting an auth code for a
    policy denial. Peers match on `code` and already "MUST NOT parse
    `message`", so no peer contract breaks.
    D12 moves **two** boundaries, not one — an earlier draft claimed one and
    named the weaker of them:
    1. `call_error.rs`'s prose that `message` is the error's full `Display`
       text. For `auth.token` denials it is a constant instead.
    2. **The heavier one.** `impl From<&DenyReason> for CallError` documents
       itself as "the single owner of the guard-denial → wire mapping … no
       broker may re-derive this mapping". D12 has the auth broker do exactly
       that. This is a one-rule-one-owner question (root `AGENTS.md`), so it is
       not enough to note it: the `auth.token` code→constant rendering MUST live
       in **one** function in the auth module, referenced by every deny path
       there, and MUST NOT be copied per call site. If a second capability ever
       needs the same treatment, the rendering moves into `keld-ipc` beside
       `From<&DenyReason>` rather than being duplicated.
    A **third** boundary moves, and it belongs to (d) rather than to D12. An
    earlier draft of this paragraph listed the code as "unchanged, still
    `DenyReason::code()`"; that is false on (d)'s path, where the guard said
    Allow and there is no `DenyReason` at all, so the broker **synthesizes** a
    `KELD-GUARD002` reply. `call_error.rs` says `code` is "owned by the crate
    that produced the failure", and the crate that produced *this* failure is
    `keld-native`. The reply must still read `KELD-GUARD002` — byte-identity
    with the granted-level denial is the entire point of (d) — so the boundary
    is moved deliberately, and it is bounded three ways: the synthesis lives in
    the **same single** auth-module rendering function as (c), so there is one
    owner and not two; `docs/engineering/keld-error-codes.md` keeps
    `KELD-GUARD002` registered under crate `keld-guard`, because the decision
    *vocabulary* is still the guard's — and errata pass 10 corrects the reason
    given for that: the registry's `crate:` field **does** name a crate, so
    saying it "maps code → docs rather than code → emitting crate" was false.
    What is true is narrower and is the whole of the slack: the CI gate
    (`crates/keld-cli/tests/error_registry.rs`) never compares that field against
    the tree a code is emitted from, so the registry entry stays accurate as
    documentation while the emitting crate differs. T3 MUST record that
    divergence where the code is emitted rather than leave it to the registry; and the host-side report of the fault names
    `keld-native` as its origin, so the attribution the wire cannot carry is
    not lost where it is actionable.
    All three are listed in §8's Public API gate — which is true as of errata
    pass 7 and was **not** true before it, when §8 named only the first — and
    the promote PR updates
    `02-ipc.md` §2 and `07-agent-experience.md` §2 in the same commit (note 228
    §9) — §2 cites those sections as the contract's owner.
    **What this costs, without dressing it up (corrected after review).** An
    earlier draft said the full `DenyReason` "still goes to the host-side audit
    trail". It does not, and no such trail exists: `audit` is an **ignored**
    top-level manifest key (`crates/keld-guard/src/lib.rs`),
    `dispatch_privileged` returns `Err(reason)` and logs nothing, and
    `product-status.tsv` records for `crate.keld-guard` that "audit logging
    remain[s] target behavior". So in v0 the precise deny reason is **destroyed**
    rather than relocated: the child gets a constant and nobody keeps the
    detail. That is the real price of closing the oracle, it is accepted here,
    and operator-side diagnosability is a **named dependency** on audit logging
    (`03-security.md` §2's `audit.log` destination), not a v0 property. Until
    that lands, an operator debugging an `auth.token` denial reproduces it
    against the manifest by hand.
    (d) **Allow with no resolved session is a host-internal invariant
    violation** (D13, defence in depth). If (b) Allows while (a) resolved
    nothing — which D13's load-time rejection is designed to make unreachable —
    the host **never** mints a token. There is no `DenyReason` to render — the
    guard said Allow — so the host **synthesizes the granted-level denial**:
    `CallError { code: "KELD-GUARD002", message: <the same constant as (c)> }`.
    That is the correct peer, because a caller reaching this path *was* granted,
    so the case it must be indistinguishable from is the granted-level
    out-of-scope denial, **not** the ungranted one (those two differ by `code`
    by design, and always will). It deliberately gets **no** distinct
    child-visible code — not `KELD-AUTH-015`, not any other: a code whose
    condition is "your id did not resolve" would disclose precisely what (c)
    conceals, which is the rule §4's code table retains. (`KELD-AUTH-015`
    belongs to D14's missing-`clientId` condition on `auth.begin` and to
    nothing else; an earlier draft of D13 also attached it here, which would
    have reopened the oracle this criterion exists to close.)
    The fault is reported **host-side only**, on the host's own error path: T3
    MUST surface it there rather than swallowing it, and v0 has no audit sink
    to make that record durable (the cost paragraph above), so nothing outside
    the running host retains it until audit logging lands.
17. **AC14 (`auth.begin` ordering, symmetric with AC13).** Given an
    `auth.begin` CALL naming `provider`: (a) the host resolves `provider`
    against the provider-profile table (§4) to its declared issuer literal —
    a side-effect-free lookup that starts no broker, no scheme handler and no
    listener; an unknown `provider` is a typed pre-Allow `KELD-AUTH*` error.
    (b) The single authorization decision is
    `dispatch_privileged(.., "auth.begin", <declared issuer>, ..)`. No OS side
    effect of any kind occurs before that call returns Allow (AC1 covers the
    no-grant case; this criterion covers the ordering).
    (c) **Request-field validation is post-Allow and still pre-side-effect**
    (errata pass 4). Immediately after Allow and before any rung work begins —
    before the WAM provider lookup, before the loopback bind, before any browser
    launch — the host validates the request surface and refuses without any OS
    side effect. Two checks live in this slot: child-supplied fields
    (`login_hint`, §4 Request surface) refuse with `KELD-AUTH-010`, and the
    **host-side profile configuration** (D14: the resolved profile has no
    `clientId` in `keld.config.ts`) refuses with `KELD-AUTH-015`. Both are
    post-Allow precisely so that neither is a pre-Allow discriminator: an
    ungranted caller never learns whether its fields were well-formed **or**
    whether this host has that profile configured — it sees `NotGranted` either
    way, byte-identically.

## 4. Design

**Reuse and ownership (first principles).** The auth broker is a `keld-native`
module behind `keld_ipc::guard_dispatch::dispatch_privileged` — under KEL-102-D5
the sole privileged path for registered native channels. `MediaPolicy` is
KEL-102-D5's **named destination** for the webview-media caller, not a shipped
type: no `.rs` or `.ts` file in the workspace contains it (errata pass 10
corrects earlier revisions, which described it as an existing second caller).
Today the only production caller of `dispatch_privileged` is `keld-native`'s
`fs` module, and webview media calls `evaluate` **directly**
(`crates/keld-wv/src/media.rs`) — that direct-evaluate call is the remnant
KEL-102/T4 cleans up, and it is not a pattern to copy. Grant evaluation needs **zero
guard code**: `keld-guard`'s private `grant_node` and public `evaluate` are
capability-generic, so
`app.auth.begin` grants evaluate today; what the guard load path must ADD is
AC9b's rejection of malformed auth grant entries — `**` **and**, per D13,
control characters (errata pass 6: earlier revisions said "wildcard rejection",
which has been only half the predicate since D13). Custom-scheme registration belongs to the `deeplink`
module destination and durable secrets to the `secrets` module destination
(`05-webview-and-native.md` §3) — this spec extends those owners and forks
neither. Deny payloads reuse `keld_ipc::CallError`/`write_call_error` (single
owner). Rejected alternatives: a parallel auth dispatcher (bypasses D5), a Bun
helper owning listeners/refresh (competitor punt pattern, 169), reversing the
Google rung order (192 Q1 rejected — 226 §9), a guard "evaluate arm" (no such
concept — 226 F16).

**Principal.** The auth principal is KEL-102's `TrustedCaller::V0AppLink →
Principal::AppProcess` mapping (`kel102-host-guard-enforcement.md` §4 D4/T3; not yet landed in code). KEL-75
`RolePrincipal` is not wired to `keld_guard::Principal` and is not the auth
identity. No KEL-97 dependency: a single-app-link principal needs only the
KEL-102/T3 disjunct (226 F26); KEL-102/T5 later consumes KEL-97 for role
generations, never the reverse (kel102 D6).

**Policy snapshot.** The permissions snapshot is immutable per host session with
no reload (kel102 AC8): changing an `auth.*` grant requires session teardown +
link/principal revocation. The spec MUST NOT assume live re-grant.

**Host-broker chain (canonical order: OS broker → host-registered custom scheme →
loopback+PKCE external browser; each rung names ownership and unavailability).**

Windows:

| Rung | When | Ownership | Unavailable/inapplicable |
| --- | --- | --- | --- |
| 1. WAM (**T2 spike GO, 2026-09-09 — see §4 addendum, D1**) | Entra/MSA issuer, WAM available | Host HWND into interop; host-owned COM apartment (`!Send`/`!Sync`); child: nothing | WAM missing, or non-WAM issuer (e.g. Google) → rung 3 (rung 2 requires registration that v0 packaging cannot emit). Single-tenant authority-URL format and Entra-joined/PRT behavior remain untested (241 §7) |
| 2. Custom scheme | IdP documents desktop scheme support AND registration exists | Host is handler; correlates host-minted `state`; registration is unauthenticated last-writer on Windows — delivery ≠ ownership | v0: effectively unavailable (no MSIX/`uap:Protocol` emitter exists; keld-pack is a name-only enum) → rung 3. Google: skipped by policy (AC8 quote) |
| 3. Loopback + PKCE | Default | Host binds per AC5a/AC5b, launches system browser, correlates callback | Bind/launch failure → typed `KELD-AUTH*` error with fix |

macOS:

| Rung | When | Ownership | Unavailable/inapplicable |
| --- | --- | --- | --- |
| 0. Enterprise SSO broker | **Never claimed in v0** (D6). Device-compliance CA → AC7b typed deny | n/a | Destination tier: broker-aware routing on MDM-managed devices, own spec + managed-hardware proof |
| 1. ASWebAuthenticationSession | Default interactive path | Host retains session + provider; `NSWindow` anchor on main thread; in-process custom-scheme initializer callback needs no Info.plist (168:74) and does not transit OS scheme dispatch (226 K11/F12, per 176:75; the narrow defensible form of the old "hijack-proof" claim) | API/start failure → rung 3. `prefersEphemeralWebBrowserSession` is an ignorable preference — never an isolation claim. Third-party default browsers without `CallbackURLMatchingIsSupported` route to Safari |
| 2. Custom scheme (app-launch form) | Registered via `CFBundleURLTypes` when relaunch-by-URL is required | Host receives URL via bundle registration | v0: no `.app` bundle/plist emitter exists → not reachable; Google: skipped by policy (AC8 quote); use rung 1's in-process form or rung 3 |
| 3. Loopback + PKCE | Fallback | As Windows rung 3 | As Windows rung 3 |

Linux:

| Rung | When | Ownership | Unavailable/inapplicable |
| --- | --- | --- | --- |
| 1. OS auth broker | **Never claimed** — no `org.freedesktop.portal` auth/identity/OAuth interface exists | n/a | Document gap; do not invent a portal |
| 2. Custom scheme | Opt-in only where IdP-allowed and `.desktop` registration exists | Host owns registration + correlation | v0: no `.desktop` emitter; Google: skipped by policy |
| 3. Loopback + PKCE + external browser | Default honest path | Host owns portal parent-window id, `OpenURI` request (interface v3, returns a Request handle), loopback listener | Portal/session bus missing → typed error naming the reviewed launch policy |

**Linux availability (D10; premise refreshed 2026-09-09).** At this note's
original pin (`c6e14f1`) Keld's no-flag host failed closed on Linux and
KEL-96/T4 was open. **That edge has since landed**: `c41ec5e`
(`feat(linux): wire no-flag host sessions`, #145, 2026-09-04) checks
`kel96-no-flag-host-boot.md` T4; `KELD-CORE-034` is now gated
`#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]`
(`crates/keld-core/src/app_session.rs`), i.e. it fires only outside
mac/linux/windows, and `crates/keld-host/tests/no_flag_linux.rs` exists.
`product-status.tsv` records `surface.no-flag-linux` = **partial** (Ubuntu/Debian
x86_64 Wayland implemented; X11, non-Debian and release packaging unverified).
(`617ba41`, #146, is KEL-78/T4 strict isolation — a different slice.)
D10's decision is unchanged: Linux e2e auth acceptance stays out of v0 scope,
now on the narrower ground that the landed Linux surface is itself `partial`
rather than absent. The component-level Linux ACs (4, 5a, 5b, 10-as-harness)
are written now; e2e activation is a scope decision at implement time, no
longer a blocked dependency edge.

**Types & channels (v0 surface — D2/D4).**

```text
AuthRequest::Begin { provider, scopes?, login_hint? }
//   provider: the provider-profile key that yields the profile's declared
//   issuer literal (§4 "Issuer literal"); that literal, not this key, is the
//   guard scope resource for both auth.begin and auth.token.
//   scopes: Option<Vec<String>> — OAuth scope values for this request.
//   NOT manifest-constrained in v0 (§4 "Request surface"; §4 non-claim).
//   login_hint: Option<String> — OIDC login_hint, child-supplied and
//   host-validated (§4 "Request surface"); advisory to the IdP, never an
//   authorization input.
AuthRequest::Token { account_id, scopes? }
//   account_id: host-minted, opaque, host-session-scoped runtime selector
//   (§4 "Auth session registry"), NOT an authorization scope (D11/AC13).

AuthResponse::Begin { account_id, granted_scopes }
AuthResponse::Token { access_token, expires_at, token_type }  // no refresh_token field exists

// Deferred, names reserved (non-goals in v0; field sketches non-normative):
//   AuthRequest::Accounts / AuthResponse::Accounts
//   AuthRequest::SignOut / AuthResponse::SignOut
//   AuthEvent::AccountChanged { account_id, change }
```

**Wire naming (normative, single source).** The field names above are the wire
names: `snake_case`, exactly as written in this block, in both directions. No
camelCase alias exists and no rename layer is introduced; any prose elsewhere in
this note that spells a field differently is prose, not the contract. AC2's
"REPLY encoding contains no refresh-token field" and its test row assert against
these names under the existing postcard encoding (positional, so field order is
the contract and casing is not a wire property).

**Request surface (normative; errata pass 4, 2026-09-09).** Three request fields
were named in the types block with no semantics. This paragraph is their single
owner.

*`scopes` — not manifest-constrained in v0, stated rather than left implicit.*
The manifest constrains the **issuer only**. An app holding `app.auth.begin`
(or `app.auth.token`) at issuer P MAY request **any** OAuth scope set at P; the
host neither filters nor rewrites it. This is a real widening and it is disclosed
in §4 rather than hidden: an app granted an issuer for a sign-in flow can also request a broad data scope
at that same issuer — for a provider offering both, which is the usual case.

**This is a recorded position, not a derivation.** An earlier draft of this
paragraph argued it "follows from decisions already taken" by citing D3 and D8.
Both legs are withdrawn. D3 fixes the *guard scope resource* vocabulary — the
string the manifest matches — which is a different object from the OAuth
`scopes` request parameter that merely shares the word "scope"; citing it was an
equivocation. And a constraint would **not** need a manifest-schema change: the
provider-profile table is host-side and closed, so a per-profile declared scope
set is available today with zero schema change and zero D8 conflict. So
"unconstrained" is the status quo the note has always described (AC11 keys
single-flight on the requested scope set) — it was **chosen, not entailed**.
**D15 (2026-09-09) takes that choice explicitly**: requested scopes stay
unconstrained in v0. The rejected alternative is the one now known to be cheap —
a per-profile maximum scope set in the host-side table, needing no schema change
— and it is rejected on the same ground D11 used to give up the cardinality cap:
no non-arbitrary maximum exists for a general-purpose framework, and a wrong
maximum silently breaks legitimate apps, which is a worse failure than a
disclosed widening. The operator has already authorized the issuer. The
constraint returns with the operator-extensible profile source, where the
**operator** declares the maximum instead of Keld guessing it for them.

*`AuthRequest::Token.scopes` and its relation to `granted_scopes` (D17, errata
pass 6).* Pass 4 defined `scopes` on `Begin` and left it undefined on `Token`,
where it also keys AC11's single-flight — so an implementer had to invent the
relation. It is now stated. `Token.scopes` is **passed to the IdP's token
endpoint unchanged**: the host does not filter it, does not rewrite it, and
does **not** compare it against the `granted_scopes` the establishing `begin`
returned. A `Token` request naming scopes broader than the session's
`granted_scopes` is therefore not refused by Keld; it is refused, narrowed or
honoured by the IdP, and an IdP refusal surfaces as `KELD-AUTH-013` (exchange
failed) carrying the provider's own error. Three reasons, in order of weight:
the host holding no scope policy is exactly D15's recorded position, and a
`Token`-only check would be a scope constraint smuggled in one capability
lower; `granted_scopes` is the IdP's answer to a past request, not an authority
Keld is entitled to re-enforce, and treating it as a ceiling would break the
legitimate case where an IdP grants incrementally; and a Keld-side refusal
would be a **pre-exchange** discriminator over host-held session state, the
class of leak AC13(c) and AC14(c) exist to remove. The corresponding cost is
recorded as a non-claim in §4 rather than hidden: `AuthResponse::Token`
carries no granted-scope echo in v0, so a child that requests a broad set and
receives a narrower token cannot tell from the reply. Adding that echo is a
public-API change with its own gate; it is named as a destination, not taken
here.

**Exactly one boundary is enforced: the issuer** (AC9, AC13). An earlier draft
also offered the IdP's consent screen as a second "enforced boundary". That is
withdrawn, because this note's own evidence refutes it: 241 §4.2 did log a real
consent dialog on hardware, but 241 §4.3 logged
`GetTokenSilentlyWithWebAccountAsync` returning a token in **728 ms with no UI
at all** once consent was on file — and `AuthRequest::Token { account_id,
scopes? }` carries its own scope set, so the capability the widening matters
most for is exactly the one that mints tokens without showing anyone anything.
A screen the host neither controls, observes, nor can require is a
provider-side mitigation, not a boundary this spec enforces.

*`session_policy` — removed from the v0 surface.* It appeared exactly once, in
the types block, with no type, no values and no semantics. Its only plausible v0
meaning is a per-request browser-session preference, and that meaning is already
a non-claim twice over: §4 states "No ephemeral-session isolation", and the
macOS rung table records `prefersEphemeralWebBrowserSession` as "an ignorable
preference — never an isolation claim". Shipping a request field the host cannot
honor as policy is the "exists only to look complete" case root `AGENTS.md`
YAGNI forbids, and a field that *looks* like an isolation control while
guaranteeing nothing is worse than its absence. Unlike D4's deferred names, this
name is **not reserved**: reserving a name whose semantics nobody has agreed
would re-create the same ambiguity later. If a session-behavior control is ever
needed, it arrives with its own decision, its own AC and its own gate.

*`login_hint` — kept and defined, because two passages depend on it.* It is the
OpenID Connect Core 1.0 §3.1.2.1 authorization-request parameter: a hint about the
login identifier the end user might use, whose "use … is left to the OP's
discretion". It is retained rather than deleted because the D11 trade ledger and
§4's non-claim both rest on its existence — it is one of the two reasons this
spec does **not** claim that every reachable account required a human to complete
a broker dialog. (Untested PRT-backed silent SSO independently keeps that
non-claim necessary, so the field is not the *only* reason for it; it is one of
two, and deleting it would leave the ledger citing a field that no longer
exists.)

It is **child-supplied**, so the host validates it. Ordering, and why it is not
the second pre-Allow refusal: validation runs **immediately after**
`dispatch_privileged` returns Allow (AC14(b)) and **before any rung work
begins** — before the WAM provider lookup, before the loopback bind, before any
browser launch — so it still precedes every OS side effect, while an ungranted
caller learns nothing at all: it sees `NotGranted` whether its hint was
well-formed or not. (An earlier draft said "before the authorization request is
constructed", which is too late — AC5a requires the loopback bind *before* the
request is built, and the bind is itself an OS side effect with its own
`KELD-AUTH-004`.) That keeps `auth.begin`'s unknown-`provider` refusal the
*only* pre-Allow typed error, as §4 Errors states. The host MUST:

- reject a `login_hint` longer than **256 bytes** of UTF-8 (`KELD-AUTH-010`).
  This bound is a spec-author choice with no external source — OIDC Core places
  no length or character constraint on the parameter — and is recorded as ours
  rather than dressed up as a standard;
- reject any Unicode control character — C0 (`U+0000`–`U+001F`), `U+007F`, and
  C1 (`U+0080`–`U+009F`) — while **accepting non-ASCII otherwise**
  (`KELD-AUTH-010`). An earlier draft rejected every byte outside printable
  US-ASCII; that is withdrawn. It bought nothing, because the injection control
  is the percent-encoding in the next bullet and not the character class, while
  it rejected legitimate internationalized identifiers (RFC 6531 addresses,
  non-Latin UPNs). Control characters are excluded because they are never part
  of a login identifier;
- percent-encode the accepted value as a single `login_hint` query-parameter
  value on the loopback and custom-scheme rungs — **never** string-concatenate
  it into a URL. On Windows rung 1 the WAM request-property name for a login
  hint is **not pinned by this spec**: 241 records none and no primary source
  was consulted, so T5 MUST confirm it against current Microsoft documentation
  before implementing (note 228 §7 falsifier). Until then rung 1 MAY drop the hint rather
  than guess a property name;
- treat it as **advisory only**: it is never an input to any authorization
  decision, never widens a grant, and MUST NOT be used to suppress the account
  picker or the consent dialog. Whether the IdP honors it at all is the IdP's
  choice;
- treat the value as end-user PII: it is never logged and never echoed back in
  error text. `KELD-AUTH-010`'s fix text names the violated constraint (length or
  character class) and MUST NOT quote the offending value.

Channel id assigned at implement time in the existing registry; control-plane
encoding unchanged (`02-ipc.md` §2). No frame-layout/`FrameKind` change — the
wire gate stays `none` unless implementation alters `02-ipc.md` §2 constants (kel102 bars
frame changes; the KEL-101 named-pipe transport change was its own reviewed
boundary and is transparent to channel payloads).

**Issuer literal and provider profiles (D11 errata, 2026-09-09 — normative).**
The auth scope resource, for **both** `auth.begin` and `auth.token`, is a
provider profile's **declared issuer URL**: one canonical string per profile.
`AuthRequest::Begin { provider, .. }`'s `provider` field is the profile key
that yields it. It is **not** the IdP-returned `iss` claim and not a rung
transport parameter: each rung maps the declared issuer to its own transport
(the WAM rung to a `(WebAccountProvider id, authority)` pair; loopback rungs to
the authorization endpoint), and that mapping is a property of the profile, not
of the grant. Evidence that the distinction is load-bearing rather than
pedantic: in 241 (`docs/research/library/auth-distribution/241-wam-operator-run.md`) a profile configured for
`https://login.microsoftonline.com/common` produced a response whose `iss`
property was `https://sts.windows.net/4da63538-…/` — an operator could never
have authored that in advance. **Citation corrected in errata pass 4:** the fact
lives in 241's raw bundle
(`artifacts/runs/wam-operator-run-2026-09-09/evidence/run-04-operator-tenant-success.log:309`),
not in 241 §4.2 as earlier revisions claimed, and it is a `WebTokenResponse`
property rather than a decoded token claim. The tenant it names is the
operator's **own registered tenant, echoed back** (241 §4.2
`Properties["TenantId"]`), which is why row 13a real's observable is stated as
**inequality against the declared literal** rather than the earlier "different
tenant" wording, which `common` cannot satisfy either way.

**In v0 the profile table is host-side and closed**: a compiled-in table in the
`keld-native` auth module mapping profile key → declared issuer literal + per-rung
transport parameters + the D7 redirect-form selection. It is **not** manifest
data, so this spec still authorizes **no manifest-schema change** (§4
gate-deferral clause) and the only auth strings an operator authors are the
issuer literals inside `app.auth.begin` / `app.auth.token` grants. An
operator-extensible profile source — one that lets an operator **add or edit
profiles**, with its own schema, collision rules and load-time validation — is a
**named destination tier**, gated on its own spec and schema review, not v0.
D14's `keld.config.ts` `auth` section is **not** that: it supplies
per-application *credentials* for keys the closed table already defines, and can
neither add a profile nor change an issuer literal. A key there that names no
tabled profile is **ignored with a load-time warning**, never a new profile —
that is the line between D14 and the deferred tier. This is what makes
the grant authorable before any login: the operator reads the declared issuer
for a profile from documentation and writes that literal.

**v0 provider-profile table — contents (normative; errata pass 4, 2026-09-09).**
**One row ships in v0** (`entra-common`) — the only profile with a real
hardware run behind it. A `google` row was drafted and **dropped by D16**: no
decision authorized it, it carries no Keld run, and the gap asked for one
concrete row. It survives below purely as a worked illustration of the
declared-issuer rule, **not** as a v0 profile; AC8's Google skip logic is
unaffected, because it constrains rungs rather than profiles. Profile keys are lowercase ASCII kebab-case, unique, and the
set is **closed**: adding, renaming or removing a key is a spec change — though
not for the reason an earlier draft gave. A *rename* breaks no manifest, because
grants name issuer literals and never profile keys; it breaks the child's
`provider` string, which fails loudly as `KELD-AUTH-002`. What a table edit can
change *silently* is the authorization surface: editing a row's declared issuer
literal changes what every existing `app.auth.*` grant matches, and adding a row
widens the set of issuers an app can reach using grants already written. That is
why the table is a permission-model gate item (§8), not merely documentation.

| Profile key | Declared issuer literal — *the guard scope resource* | Win rung 1 (WAM) transport | Rung 3 authorization endpoint | Rung 3 token endpoint | D7 redirect form | Rung 2 (custom scheme) |
| --- | --- | --- | --- | --- | --- | --- |
| `entra-common` | `https://login.microsoftonline.com/common` | `WebAccountProvider` id `https://login.microsoft.com`, `authority` `common` | `https://login.microsoftonline.com/common/oauth2/v2.0/authorize` | `https://login.microsoftonline.com/common/oauth2/v2.0/token` | `localhost` — the documented **D7 Entra exception** to AC5a's default | unavailable in v0 (no MSIX/`uap:Protocol` emitter) |

Provenance, per cell class:

- `entra-common`'s WAM pair is `[fact]` from a real run:
  241 (`docs/research/library/auth-distribution/241-wam-operator-run.md`) §4.2 logged
  `FindAccountProviderWithAuthorityAsync("https://login.microsoft.com", "common")`
  and a `ResponseStatus = 0 (Success)` token from an unpackaged exe.
- The tabled authorization endpoint is `[fact]` read from the IdP's own live OIDC
  discovery documents on 2026-09-09 (note 228 §3 ledger rows; note 228 §7
  carries the falsifier).
- The redirect-form column is **decided**, not observed: it is D7 applied per
  profile, which is exactly what AC5a means by "a recorded field of that
  provider profile … not an inline special case at the call site".
- The token endpoint is a separate column because it is **not derivable** in
  general: Google's sits on a different host (`oauth2.googleapis.com`) from both
  its issuer and its authorization endpoint, so a rule that derived it would be
  wrong for the first profile added after D16, even though `entra-common`'s
  happens to share a host. `auth.token`'s refresh exchange needs it, so a row
  without it is not implementable.
- Rung coverage: rung 3 is OS-independent — the same two endpoints serve
  Windows, macOS and Linux — and macOS rung 1
  (`ASWebAuthenticationSession`) drives the same authorization endpoint. No
  per-OS transport parameter is therefore missing **except** one open question
  recorded in §10: whether AC8's custom-scheme skip bars macOS rung 1's
  in-process callback, or only OS scheme dispatch. D16 answers it for the v0
  table — there is no Google profile to route — and it returns with the first
  non-Entra profile, which is a spec change, not an implementation choice.
- AC5b's exact-literal registration mode has **no column** because it has no
  selector in the v0 surface. D14 gives it one: it is a per-profile property of
  the `keld.config.ts` auth section alongside `clientId`, set by the operator
  who performed the registration, since only they know whether they registered
  a fixed redirect. AC5b owns the firing condition.
- `entra-common`'s rung 3 is **not** end-to-end verified by any Keld run. The
  only auth flow ever executed on hardware is its **rung 1** (241). Rung 3 is a
  written contract awaiting T3.

**Which string is the declared issuer literal.** The literal is the profile's
**configured authority** — the string the operator can read from documentation
and author before any login. It is a rule about the *profile*, not a claim
about the IdP, and the contrast below shows it holding in both directions:

- For `entra-common` the literal is `https://login.microsoftonline.com/common`,
  which is neither the `iss` the IdP returns nor the `issuer` its own discovery
  document advertises. 241 §4.2 records the WAM-returned account carrying
  `Properties["Authority"] = "https://login.microsoftonline.com/common"` — the
  configured value, echoed back. The divergent `iss` is **not** in 241's prose:
  it is in that run's raw bundle,
  `artifacts/runs/wam-operator-run-2026-09-09/evidence/run-04-operator-tenant-success.log:309`
  (repeated at `:386`), as
  `ResponseData[0].Properties["iss"] = "https://sts.windows.net/4da63538-…/"` —
  a `WebTokenResponse` **property-bag entry**, not a decoded token claim (241
  §4.2 records the token as `<withheld> … jwt-shaped=true`, never decoded).
  Independently, that endpoint's
  discovery document declares `issuer` as the **templated**
  `https://login.microsoftonline.com/{tenantid}/v2.0`. A template is not an
  authorable literal, and a per-tenant `iss` is not knowable in advance, which
  is precisely why D11 fixes the scope resource to the configured authority.
- **Worked illustration, not a v0 profile (D16).** Were a `google` profile
  added, its literal would be `https://accounts.google.com`, which **does**
  equal that IdP's fixed `issuer` (live discovery, note 228 §3). The rule is unchanged —
  take the profile's configured value — so it holds whether that value diverges
  from `iss` (Entra) or coincides with it (Google). The contrast is why the
  rule is stated as "the profile's configured value" rather than "not the
  `iss`": the latter would be wrong for half the providers.

**Not profile-table fields — and where the client id does live (D14,
2026-09-09).** The OAuth **client id** (241's registration is
`ffd5a764-256d-44d5-b646-cc684ac82e5e`), the tenant id, and requested scopes are
per-application operator configuration, not rows of this compiled-in table,
which carries only what is identical for every Keld app talking to that
provider. A compiled-in client id would make every Keld app share one
registration.

Their home is **`keld.config.ts`**, under an `auth` section keyed by profile
key: `auth: { "entra-common": { clientId: "…" } }`, with an optional
`fixedRedirect: "…"` beside `clientId` where the operator registered a fixed
redirect. (Errata pass 7: earlier revisions wrote `fixedRedirect?:` inside the
value literal, which is TypeScript *type* syntax and does not parse if copied.)
Rationale: it is per-application, which is what the value is; it is
operator-authored, so the child cannot assert its own registration; an OAuth
public-client id is **not a secret** (that is what PKCE is for), so plaintext
config is correct; and `keld.config.ts` is already one of the four sanctioned
config names (root `AGENTS.md`). It is **not** the permissions manifest, so D8's
"no manifest-schema change" clause is untouched.

**How it reaches the host — corrected after independent review.** An earlier
draft of this paragraph said `keld.config.ts` is "parsed by `keld-core` and
reached by the host through the boot path". Pass 5 called that "false and the
whole of D14's buildability" and replaced it with "the host's only input is the
compiled descriptor". **Errata pass 7 withdraws that replacement too: it is
also false**, and an independent review found it by execution rather than by
reading.

What is actually shipped: `crates/keld-cli/src/boot.rs` does read the config in
the **CLI**, extracts only `name`/`entry`/`renderer`, and does not stage
`keld.config.ts`. But `keld-core` **publicly exports** `read_config_title`,
`read_config_renderer` and `read_config_entry`
(`crates/keld-core/src/hello.rs:134-155`, re-exported at
`crates/keld-core/src/lib.rs:18-19`), each of which does
`fs::read_to_string(project_root.join("keld.config.ts"))`, and
`crates/keld-host/src/main.rs:72` calls `keld_core::resolve_hello_title(&args,
cwd)`, which falls through to `read_config_title`. **The shipped host binary
therefore reads `keld.config.ts` from its current working directory today.**
The pass-5 sentence survived only on a technicality — the string literal lives
in `keld-core` rather than in `keld-host`'s or `keld-native`'s own `src` — and
pass 6 narrowed the wording in a way that made the technicality tidier without
making the surrounding claim true.

**What this changes, and what it does not.** It does **not** revive the
rejected alternative. That host-side reader resolves against the *current
working directory*, which is a developer convenience for `--hello` and is not
an app-identity-bound path: it is not the permissions digest, it is not
integrity-checked, and a host started in a different directory reads a
different file or none. Routing an **OAuth client id** through it would make an
operator-authored credential depend on the host's cwd, which is exactly the
property a boot descriptor exists to remove. So D14's descriptor route stands —
but on the honest ground that the existing reader is *unsuitable*, not on the
false ground that no reader exists. `BootDocument` remains the closed contract
that must change, and it is **private**: a `cfg`-gated struct
(`crates/keld-core/src/app_session.rs:396`), `#[serde(deny_unknown_fields)]`
over `{schema, name, entry, renderer, permissions}` with `schema == 1`, and
`ParsedBoot` (`:413-418`) discards every field but
`name`/`entry`/`renderer`/`permissions_digest`, so carrying `auth` to
`keld-native` needs a **new public `keld-core` surface** as well as the field —
both gated in §8.

The value therefore travels: operator writes `keld.config.ts` -> the boot
compiler extracts the `auth` section -> it is emitted into `keld.boot.json` ->
`BootDocument` carries it to the host -> the `keld-native` auth module reads it.
Four consequences the spec must own, and now does:

- `BootDocument` gains an **optional** `auth` field (`#[serde(default)]`), so
  descriptors without it still parse and `schema` stays `1`. Absent or empty
  means no profile is configured, which makes every `auth.begin` refuse.
- **v0 reaches this only under `keld dev`.** The sole producer of
  `keld.boot.json` in the workspace is `keld_cli::boot::stage_dev_boot`, whose
  own doc calls it a "non-release owner-private" stage, and no verb produces a
  release descriptor: `build` exists in `keld-cli`'s verb table only as a
  reserved name that refuses with `KELD-CLI-045` pointing at KEL-19, and there
  is no `pack` verb at all (keld-pack is a name-only enum, §4 packaging).
  Errata pass 6 corrects an earlier "there is no `build` … verb", which was
  wrong about the verb while right about the consequence. So auth is dev-path
  only until a release descriptor producer exists — a named dependency, not a
  claim this spec makes. Note 228 §5's acceptance matrix records it as such.
- **`keld-core` and the boot compiler are implement-in targets** (§5), not
  just `keld-native`/`keld-ipc`. The boot descriptor is a host-input contract
  adjacent to the permissions digest, so this rides the Public API gate
  alongside the `keld.config.ts` section itself and the new public `keld-core`
  surface named above (§8) — **three** contracts, not one (errata pass 8: pass 7
  added the third to §8 and left this sentence saying two).
- T3 needs an extractor, and **its shape is constrained here so that no
  dependency is required** (D8's clause authorizes none, and §8 gates none):
  `keld-core`'s existing `quoted_config_field` idiom is a per-line scraper and
  returns nothing for a nested key, so the `auth` section is specified as **at
  most two levels deep, object literals only, string values only** — that is
  parseable by extending the existing scraper with brace-depth tracking, in the
  same hand-rolled style, with no TypeScript parser. If an implementer concludes
  a real parser is needed, that is a **dependency addition with its own gate**
  (§8) and a spec change, not an implementation detail.

**A missing or unknown `clientId` is a post-Allow refusal, not a pre-Allow one**
(corrected after review). Resolving it before the guard would let an ungranted
caller sending a *known* provider distinguish "this host has a client id
configured" from "it does not" — reintroducing exactly the pre-Allow
discriminator AC14(c) closes for `login_hint`. So the profile-configuration
check runs with the other request-surface validation, after Allow and before any
rung work, and raises `KELD-AUTH-015`. An ungranted caller sees `NotGranted`
either way.

**What this unblocks.** `AuthRequest::Begin { provider: "entra-common", .. }`
resolves (AC14(a)) to `https://login.microsoftonline.com/common`, and that is the
string `dispatch_privileged` matches against `app.auth.begin`, the string a live
session records for AC13(b), and the literal an operator writes in the manifest.
§7's rows 13a CI / 13a real name it directly.

**Auth session registry (D11 errata — new state, one owner).** The
`keld-native` auth module owns a per-host-session registry mapping
`account_id` → session. `account_id` is **host-minted, opaque, unguessable and
unique within the host session**; it is never app-asserted and carries no
durable meaning. A session is **live** from a successful `begin` until the
earliest of: host process exit (AC10 — the registry dies with the process, like
the verifier/state/listener), or a terminal refresh failure (`invalid_grant`)
that evicts it. Session lifetime is therefore bounded by the host process
(D5's "host restart ⇒ re-authenticate"); there is no revocation surface until
D4's reserved `auth.signout` ships. No durable storage is introduced.

**Capabilities & manifest (D3).**

| Capability | Scope resource | Child effect |
| --- | --- | --- |
| `auth.begin` | issuer URL, exact string literal | Start interactive flow → `{ account_id, granted_scopes }` |
| `auth.token` | issuer URL, exact string literal (same vocabulary as `auth.begin`; D11 errata) | Short-lived `{ access_token, expires_at, token_type }` for an `account_id` that resolves to a live session at a granted issuer (AC13) |

Grants live under `app.auth.*` (`json_pointer_for`: `auth.begin` →
`/app/auth/begin`). A top-level `auth:` manifest key is silently ignored — not
by any reserved-key rule, because the parser has no such concept, but because
it ignores **every** unknown top-level key (`crates/keld-guard/src/lib.rs`:
"Unknown top-level keys (`$schema`, `windows`, `audit`) are ignored";
`03-security.md` §1). Grants belong under `app` only. (Errata pass 6: earlier
revisions called this "documented reserved-key behavior", which named a
mechanism that does not exist.) Malformed entries rejected per AC9b.
Exact-match semantics: issuer strings compare by byte equality after
`evaluate`'s own `..`-segment rejection, which runs on the resource before any
scope is matched, so a grant and a resource that are byte-equal but contain a
`..` segment still deny; no v0 declared issuer literal contains one.
Trailing-slash/case normalization is a deliberate non-feature — the grant must
match what the host sends to the IdP.

**The shared URL-glob defect AC9b sits on top of (errata pass 4, 2026-09-09).**
AC9b's rationale cites a defect in the shared matcher. That defect is **not**
auth-specific, and this note previously added an auth-scoped rejection to the
*shared* load path without saying so — which root `AGENTS.md` engineering
principles 3 and 5 forbid. It is recorded here, with the general fix evaluated.

*Verified, not asserted, and describing the matcher **as it was at the pinned
SHA below**.* `keld-guard`'s private `path_in_scope` stripped a `/**` suffix and
then required the next byte after the prefix to be `/`. (Errata pass 11 drops a
`:573` line pin that PR #203 moved to `:646`, and marks the tense: since #203
the same function first rejects a prefix that names no destination.) In a URL the `//` after the scheme makes
the empty authority look like a path segment, so `"https://**"` strips to prefix
`"https:/"` and every `https://…` URL satisfies the test. Executed against the
real shipped `keld_guard::evaluate` at Keld `origin/main`
`bf78a10db350ba58830efc60a80eeb67fc996ebd`, from an out-of-tree harness that
touched no product file:

| Capability | Grant | Resource | Decision |
| --- | --- | --- | --- |
| `net.connect` | `https://**` | `https://evil.example.com` | **Allow** |
| `shell.open` | `https://**` | `https://anything.example/x` | **Allow** |
| `net.connect` | `wss://**` | `wss://anything.example/s` | **Allow** |
| `net.connect` | `**` | `https://evil.example.com` | Deny — a bare `**` is inert (exact match only) |
| `net.connect` | `https://api.myapp.com/**` | `https://api.myapp.com/v1` | Allow — correct |
| `net.connect` | `https://api.myapp.com/**` | `https://api.myapp.com.evil.net/v1` | Deny — correct, no suffix confusion |

So the defect was **capability-generic and scheme-generic** (not https-specific)
and **bounded**: a host-qualified `/**` grant was correctly anchored, and only a
glob at or above the authority collapsed. **It is fixed.** KEL-208 landed as Keld
PR #203 (`fa1bfd7`, 2026-09-09). **Exactly which rows changed, executed against
`keld_guard::evaluate` at the post-#203 tree rather than inferred** (errata
pass 11 — pass 10 wrote "every row above marked **Allow** now denies", which is
false and was never run):

- rows 1–3, whose grants name **no destination** (`https://**`, `https://**`,
  `wss://**`), now **Deny `KELD-GUARD002`**;
- row 4 (`**`) denied before and still denies;
- **row 5 is unchanged and still Allows.** `https://api.myapp.com/**` →
  `https://api.myapp.com/v1` is an origin-rooted prefix grant, so #203's
  destination rule does not fire on it — and must not, which is what
  `03-security.md` §2 states in #203's own words: "`"https://api.myapp.com/**"`
  covers that origin's subtree and still denies the longer sibling";
- row 6 denied before and still denies.

So #203 removed the *authority-spanning* grants and left the *origin-rooted*
one working. The table is kept as executed evidence **at its pinned SHA
`bf78a10`**, because AC9b's rationale is a statement about the matcher AC9b was
written against; a reader must not read it as current behaviour. A second, distinct
defect surfaced in the same check and is fixed by the same PR:
`docs/architecture/03-security.md` §2's own example
`"shell": { "open": ["https://*"] }` was **inert** — a single `*` is not a
wildcard in the v0 matcher — so a published, copy-pasteable example granted
nothing, in a section headed "Wildcards allowed but linted loudly"; that example
now reads `["https://docs.myapp.com/**"]`.

*General root-cause fix, evaluated.* Three shapes, with what each breaks:

- **G1 — capability-generic load-time rejection of authority-spanning globs.**
  Reject any `**` pattern whose stripped prefix ends at or before the authority
  (concretely `<scheme>://**`) at manifest load, for every capability. Smallest,
  fail-closed, no matcher change, no new concept. **Breaks** any manifest that
  intends a scheme-wide `net.connect`/`shell.open` grant — a legitimate shape for
  a link-opening or browser-shaped app — so it is a policy decision with an
  operator-visible break, not a silent bug fix. **#203 took exactly that break**
  (errata pass 11): `03-security.md` §2 now reads "**No glob grants 'any host'
  for a multi-character scheme** — name the origins", enforced at match time
  rather than at load. So the policy question this bullet weighed is **answered**,
  and the G1/G2/G3 evaluation below is retained as the record of how it was
  reached, not as an open choice.
- **G2 — make the matcher URL-aware** through a per-capability resource-kind
  table, so a glob can never span an authority. Breaks nothing at authoring time,
  but `evaluate` takes only `operation: &str` and `path: &str` and has no notion
  of resource kind; adding one is a new shared abstraction and an architecture
  change to `03-security.md` §2.
- **G3 — forbid the `/**` suffix everywhere.** Rejected: it breaks `$APPDATA/**`,
  the documented, shipped and tested filesystem idiom.

*Why this note keeps the auth-only scope anyway — the named unmet requirement.*
Auth grants name **identity providers**, which are enumerable by construction: an
operator knows their IdPs, and no legitimate "every issuer" auth grant exists.
That is D3's deliberately "stricter-than-matcher convention", and it is decidable
from auth evidence alone. `net.connect` and `shell.open` are the opposite case —
a scheme-wide grant is a real shape — so the general rule turns on evidence this
spec does not hold, and choosing G1 over G2 changes the **published manifest
contract for capabilities KEL-89 does not own**, which §5 explicitly forbids
these implement PRs from touching. Taking it here would also put two concerns in
one PR. AC9b is therefore stricter-by-design, not a fix, and this note does not
present it as one.

*Owner and follow-up (corrected in errata pass 6; status updated in pass 10).*
The owner was **KEL-208** ("Guard: `/**` suffix stripping crosses the URL
authority boundary"), and it is **Done**: the root-cause fix landed as Keld
**PR #203**, merge commit `fa1bfd7`, 2026-09-09. It
carries the executed evidence above, the evaluated options, the inert
`https://*` example and the acceptance list, and its own description states the
division explicitly: AC9b rejects `**` at load "for `app.auth.*` only … This
issue is that missing owner: the root-cause fix in the shared matcher, covering
every capability." **KEL-209** was filed eight seconds later against the same
`path_in_scope` line and is its duplicate in substance — but **not in Linear**:
as of errata pass 7 its `duplicateOf` is null, it is `Todo` and unassigned, and
KEL-208 does not relate to it. It is therefore **pickable**, and whoever picks
it races PR #203. Closing it as a duplicate is an owner action this note cannot
take and does not assume. Pass 4 cited it as the owner, so the constraint below
was attached to the issue that will not land the fix.
One constraint travels with the real owner: whichever option lands, **AC9b MUST
be re-derived as a special case of the general rule** rather than surviving as a
parallel auth-only policy — one rule, one owner. **That constraint is now due,
not pending** (errata pass 10): KEL-208 has landed, `net.connect` and
`shell.open` no longer carry the defect, and the executed table above is a record
of behaviour **at its pinned SHA `bf78a10`**, not of current behaviour. **What survives as the real delta** — and therefore what AC9b is still for — is
narrower than the paragraph above it once implied: #203 refuses grants that name
no destination, and AC9b additionally refuses **any** `**` in an `app.auth.*`
entry and **any Unicode control character**. The control-character half is not
in #203 at all, and AC13(a)'s reserved sentinel depends on it, so AC9b cannot
simply be deleted in favour of the shipped rule. What T3
owes as a result: AC9b MUST be re-derived as a special case of the shipped rule
rather than restated as an independent auth-only predicate, and its rationale
MUST cite the shipped matcher rather than the defect it was written against.
AC9b's *predicate* is unchanged and remains stricter by design — an exact-literal
rule with a control-character rejection is narrower than any glob rule the
matcher adopted, and the sentinel of AC13(a) still depends on the
control-character half, which PR #203 does not provide.

**Token custody.** 170's custody table carries forward unchanged plus one row:
the policy snapshot is immutable per session (grant change ⇒ teardown). v0
refresh custody is host-memory-only with the explicit non-claim "host restart ⇒
re-authenticate" (D5); durable custody is the `secrets`-module **fast-follow
tier** (D5) with its own spec. Host crash drops verifier/state/listener (AC10); Bun child
crash never drops host-custodied material.

**Packaging (v0 packaging-free — 226 F13/F14).** keld-pack is a name-only enum
with zero emitters and zero dependents; no MSIX/`uap:Protocol`, `Info.plist`,
entitlement, or `.desktop` artifact can be emitted today. Every registration hook
in 170 §"keld-pack manifest hooks" becomes a documented destination row gated on KEL-19. The only
near-term observation surface is an optional `keld doctor` scheme check on
Windows (registry read via already-shipped `windows-sys` + `Win32_System_Registry`;
zero new deps); macOS/Linux doctor checks are named dependency-gate items, not v0.

**Backward compatibility (owner directive).** Auth is additive and default-deny:
an app whose manifest contains no `app.auth.*` keys sees identical behavior
before and after this capability ships. Three pre-existing-key cases are decided
policy, not accidents (owner-delegated; each has a falsifier in the test plan):
(a) a manifest already carrying a `**` entry under `app.auth.*` — inert junk
today — fails load with AC9b's typed error once this ships; that is a deliberate
fail-closed break, and the error's fix text names the exact key to correct.
(b) A pre-existing exact-literal `app.auth.*` grant activates when the auth
channel ships: a manifest grant is the operator-authored authorization artifact,
so honoring it is enforcement, not drift — default-deny governs absence, never
presence. (c) A pre-existing `acct:`-prefixed entry under `app.auth.token` —
the vocabulary D11 replaced — matches no declared issuer literal and therefore
denies `OutOfScope` at call time rather than failing load: a deliberate
fail-closed runtime deny, with a test row (compat (c)). Migrating apps may keep their hand-rolled loopback flows indefinitely
(nothing intercepts them); `@keld/electron` today exposes no protocol/auth
surface (Tier Two), and a future compat shim can map onto `auth.*` without
breaking either path. Deferred names (D4) keep later adoption extension, not
migration.

**Errors.** All privilege entry via `dispatch_privileged`; denials carry the
guard's `KELD-GUARD*` code. `auth.token` has **no** pre-Allow error class:
AC13(a)'s unresolved ids route through the guard on the reserved sentinel and
surface as a denial, not as a typed error. **For `auth.token` the denial's
`message` is a constant (D12/AC13(c)), not `DenyReason`'s `Display`** — the
requested resource is the protected secret, so it is never echoed to the child.
In v0 nothing else retains it either — audit logging is target behaviour, not a
shipped sink (AC13's cost paragraph). Every other capability forwards
`DenyReason` unchanged. `auth.begin`'s
unknown-`provider` case (AC14(a)) is the one pre-Allow typed `KELD-AUTH*`
refusal — it denies without reaching the guard and can only refuse, never
admit. Post-Allow failures (user cancel, timeout,
`invalid_grant`, compliance-deny) are `KELD-AUTH-001+` typed errors with
hand-written `Display` fix text, carried on the wire by
`keld_ipc::CallError`/`write_call_error`. AC12 fixes cancel semantics.

**`KELD-AUTH-*` code table (normative; errata pass 4, 2026-09-09).** Keld already
has a canonical error-code registry: `docs/engineering/keld-error-codes.md`, CI-
enforced by `crates/keld-cli/tests/error_registry.rs`, whose scope explicitly
includes `keld-native` `src` — the crate this module lives in. That file, not
this table, is the **single owner** of the code→docs mapping. This table is the
*specification* of which conditions get codes; the registry is where they become
real.

The test asserts the two sets match **in both directions**: a `KELD-*` token in
the scanned sources with no registry heading fails, and a registry heading whose
code appears nowhere in those sources fails too. So each implement PR adds
exactly the headings for the codes that PR makes appear — with non-empty
`crate`, `message` and `fix` lines — and writing this whole table into the
registry ahead of the code fails the same gate as omitting it. Which PR owes
which heading is **derived, not listed**: §4's rows name the AC that raises
each code and §6 maps ACs to tasks, so a third list here would be a third
source to drift. §5's "Implement in" list names the registry file.

One precision, because it bounds what the gate proves: the scan is **textual**.
`extract_keld_codes` matches any `KELD-<AREA><nnn>` token anywhere in a scanned
file, comments and doc-comments included. So the gate proves a code is
*registered*, never that it is *reachable* — `KELD-AUTH-001`'s AC3 trigger is
not expressible in the v0 request surface (its own row says so, and §10
records it), and this test would not catch that.

**Spelling.** Hyphenated `KELD-AUTH-001`, because the registry's rule is "match
the crate that already emits the code. Do not invent a third spelling", and
`keld-native` already emits the hyphenated `KELD-NATIVE-001`
(`crates/keld-native/src/fs.rs:85`). This also matches the D6/D9 decision
records, which wrote `KELD-AUTH-*`. Earlier revisions of this note used the
compact `KELD-AUTH001`; that spelling is withdrawn.

| Code | Condition | Raised for | Fix text must name |
| --- | --- | --- | --- |
| `KELD-AUTH-001` | An authorization presentation in an embedded/in-app user agent (a `keld-wv` navigation) rather than the OS broker or an external user agent | AC3 — **trigger not expressible in the v0 request surface**, so the code is specified now and becomes reachable with the rung that could raise it; AC3's test asserts the refusal, not a wire path | The supported presentations — OS broker or external user agent — and that no `keld-wv` navigation is an IdP login surface |
| `KELD-AUTH-002` | `provider` names no key in the v0 provider-profile table | AC14(a) — the **only** pre-Allow refusal | The unknown key and the closed set of valid profile keys |
| `KELD-AUTH-003` | Exact-literal redirect mode (AC5b) selected together with a dynamic port. **Request-construction only** (D18(b)): if a socket was attempted, the code is `KELD-AUTH-004`, not this one | AC5b — selected by `fixedRedirect` in the `keld.config.ts` `auth` section (D14) | That this mode requires its one registered fixed port, and that bind-0 is not a fallback here |
| `KELD-AUTH-004` | Loopback listener bind failure, or system-browser launch failure, on rung 3 | Windows/macOS/Linux rung 3 | Which of the two failed, and the operator action (free a port / configure a default browser) |
| `KELD-AUTH-005` | Linux: no portal or session bus for the external-user-agent launch | Linux rung 3 | The reviewed launch policy and the missing portal interface |
| `KELD-AUTH-006` | Entra device-compliance Conditional Access is required and no broker can satisfy it | AC7b (D6) | Intune Company Portal / Enterprise SSO as the missing broker, and that v0 makes no device-compliance claim |
| `KELD-AUTH-007` | User cancelled the broker or external-user-agent dialog (`canceledLogin` class). **Non-terminal** | AC12 (D9) | User-initiated retry; MAY add a defect-neutral default-browser hint; MUST NOT assert an Apple defect and MUST NOT auto-retry |
| `KELD-AUTH-008` | The authorization step did not complete within the host's bound | §4 Errors | Retry as a new user-initiated `auth.begin`; never an automatic retry |
| `KELD-AUTH-009` | Terminal refresh failure (`invalid_grant`). Evicts the session from the registry, ending its liveness | §4 Errors, §4 registry | That the account must sign in again via `auth.begin`, and that the host holds no durable credential (D5) |
| `KELD-AUTH-010` | Child-supplied `login_hint` failed host validation: over the byte bound, or containing a control character | §4 Request surface | The violated constraint (length or character class) only — **never** the offending value, which is end-user PII |
| `KELD-AUTH-011` | Loopback/scheme callback failed correlation: unknown or mismatched `state`, or a PKCE verifier that does not match | AC4, AC10 (`state` is host-owned) | That the callback was discarded and the flow must be restarted; MUST NOT echo the received `state` |
| `KELD-AUTH-012` | The IdP returned an OAuth error at the callback (`access_denied`, `invalid_scope`, `interaction_required`, …) | Rung 3 callback | The IdP's error code and that it is the IdP's decision, not Keld's — **sanitised per D18(a)**: provider text is bounded to 256 bytes with truncation marked, control characters stripped, quoted in a delimited position **after** Keld's own code so it can never be read as a code line, and never parsed to select a Keld code path |
| `KELD-AUTH-013` | The authorization-code → token exchange failed | Rung 3, `auth.begin` completion; also `auth.token` when the IdP refuses a requested scope set (D17) | Whether the failure was transport or IdP-rejection, and that no automatic retry occurs. Any provider text carried here is **sanitised per D18(a)** |
| `KELD-AUTH-014` | Silent acquisition needs interaction (`UserInteractionRequired` / `0xCAA10001` / `interaction_required`) | Windows rung 1, `auth.token`; 241 §4.3/§4.4 `[fact]` | That the caller must run a fresh interactive `auth.begin`; MUST NOT auto-escalate to a UI |
| `KELD-AUTH-015` | The resolved provider profile has no `clientId` configured for this application in `keld.config.ts` (D14). **Post-Allow**, so it is not a pre-Allow discriminator | §4 profile table, AC14(c) | The profile key and the `keld.config.ts` `auth` section to add; **no** account or session detail |

Obligations that hold for every row: the code is the first token of the
`Display` string; the message states the condition and then an imperative fix
sentence (`keld-guard`'s `Display` arms are the shape to copy); the error
travels on `keld_ipc::CallError`/`write_call_error`; and each row has a registry
heading. Guard denials are **not** in this table — they stay `KELD-GUARD001`
(`NotGranted`) and `KELD-GUARD002` (`OutOfScope`), owned by `keld-guard`.

**What this table does *not* do.** The rule "no `KELD-AUTH-*` code may disclose
whether an `account_id` resolves" is retained and binding — AC13(d) is written
to obey it, which is why an Allow-with-no-session gets no distinct code. But the
rule is **not** what makes AC13's replies byte-identical, and pass 4 was right
to withdraw that conclusion: AC13's replies are denials, not `KELD-AUTH-*`
errors, and `DenyReason` interpolates the requested resource into its own fix
text. **AC13(c) (D12) is the mechanism**; this table's rule only keeps a future
code from reopening what (c) closes.

**Gate deferral clause (D8, kel102-D9 pattern).** This spec authorizes **no**
`unsafe` code, no dependency addition, no kipc-wire change, and no
manifest-schema change; each later delta requires its own explicit review gate on
the exact final diff. AC9b is a permission-model delta on the shared load path,
not a schema change — the parsed manifest shape is unchanged; its gate rides the
T3 implement PR's permission-model row (§8). Recorded implement-time direction
(not executed here):
sanction `keld-native` as a production-unsafe owner via issue-scoped root +
nested `AGENTS.md` update with independent unsafe-gate evidence (precedents:
`keld-wv` backends, `keld-runtime` Windows modules, reviewed
`keld-ipc::windows_named_pipe`, and the nested-sanctioned `keld-cli` boot
boundary).

### Resolved decisions (owner-delegated, recorded on KEL-89; revocable)

D1–D10 were delegated 2026-09-01 (`linear-comment:f2178f64…`), D11 on 2026-09-09
(`9498d983…`, owner-ratified in `38dab27a…`), and D12–D16 on 2026-09-09
(`973acac2…`, owner-approved in-session and agent-transcribed in
`1b34cf85…`, then owner-ratified from their own identity in `e27751b8…`;
note 228 §3 carries both records), **D17** in errata pass 6
(`linear-comment:413725f1-3b9e-449c-816a-08f72c96e4eb`) and **D18** in pass 7
(`linear-comment:68410a82-52db-438a-a6ac-17420010aeaf`), under the same
owner-ratified standing delegation. Every decision in this note therefore
carries an owner-authored attestation, directly or through that delegation.

| # | Decision | Resolution baked into |
| --- | --- | --- |
| D1 | Spec proceeds now; WAM rung conditional on T2 hardware spike; no-go forks to pack-identity destination + loopback default. **T2 result (2026-09-09): GO — see addendum below.** | §3 AC6, §6 T2/T5, Windows rung table |
| D2 | `auth.token` returns short-lived access token; opaque-handle = future tier | §3 AC2, §4 types, §1 tiers |
| D3 | Exact-string scopes (issuer URL, `acct:<id>`); `**` rejected at manifest load. **Refined by D11 (2026-09-09): the `acct:<id>` half is replaced by the declared issuer literal — see the D11 errata below.** | §3 AC9b, §4 capabilities |
| D4 | accounts/signout/AccountChanged deferred with names reserved | §1, §4 types |
| D5 | v0 host-memory refresh custody + explicit non-claim; `secrets` = fast-follow tier | §4 custody, §1 tiers |
| D6 | macOS device-compliance: v0 non-claim + typed deny; broker-aware routing = destination tier | §3 AC7b, macOS rung 0 |
| D7 | AC5a/AC5b split; `127.0.0.1` default; documented Entra `localhost` exception; `[::1]` per-IdP; `replyUrlsWithType` operator doc | §3 AC5a/AC5b |
| D8 | Gate-deferral clause now; keld-native unsafe-owner sanction is the recorded implement-time direction | §4 clause, §8 |
| D9 | cancel = non-terminal typed error, user-initiated retry only, never auto-retry, no Apple-defect assertion | §3 AC12 |
| D10 | Linux e2e out of scope on KEL-96/T4 edge; component ACs written now. **Premise refreshed 2026-09-09** (KEL-96/T4 landed; decision unchanged, ground narrowed) | §4 Linux availability, §6 T6 |
| D12 | **`auth.token` denials carry the guard's code with a resource-free `message`** — the broker builds `CallError { code: reason.code(), message: <constant> }` instead of forwarding `DenyReason`'s `Display`, because that `Display` interpolates the very resource AC13 protects. Closes the account-existence oracle **and** the cross-issuer disclosure. Narrows `call_error.rs`'s message convention; changes no guard API and no encoding. Code ownership is **not** left unchanged — but by AC13(d), not by D12: see D13's row and AC13's boundary paragraph (errata pass 7 corrected an earlier "no code ownership" here, which contradicted §10) | §3 AC13(c)/AC13(d), §4 Errors, §7 row 13c, §8 |
| D13 | **AC9b rejects control characters as well as `**`** in `app.auth.*` entries, which is what makes AC13(a)'s sentinel unauthorable; plus the Allow-with-no-session invariant, AC13(d), which carries **no** distinct child-visible code because one would disclose exactly what AC13(c) conceals (errata pass 6 correction: pass 5 wrote `KELD-AUTH-015` here, a code D14 already owns for a different condition) | §3 AC9b/AC13(d), §4 code table, §7 rows 9b/13d |
| D14 | **The OAuth client id lives in `keld.config.ts`** under `auth.<profileKey>.clientId` — per-application, operator-authored, not a secret, and not the permissions manifest. Public-contract addition with its own gate | §4 profile table, §5, §7 client id row, §8 |
| D15 | **Requested OAuth `scopes` stay unconstrained in v0**; the per-profile maximum is rejected on the D11 cardinality ground (no non-arbitrary value; a wrong one breaks legitimate apps) and returns with the operator-extensible profile source | §4 Request surface, §4 |
| D16 | **`google` is dropped from the v0 profile table**; one row ships (`entra-common`, the only one with a hardware run). Google survives as a worked illustration of the declared-issuer rule | §4 profile table |
| D18 (`linear-comment:68410a82-52db-438a-a6ac-17420010aeaf`) | **IdP-supplied strings are untrusted input**, and the `KELD-AUTH-003`/`004` tiebreak. Any provider string reaching a `KELD-AUTH-*` message (`error`, `error_description`, `error_uri`, token-exchange text) is bounded to 256 bytes with truncation marked, has control characters stripped on AC9b/D13's predicate, is quoted **after** Keld's own code so it cannot be read as a code line, and never selects a Keld code path. Applies an invariant `crates/keld-ipc/src/call_error.rs` already states (`message` "arrives from a peer and is not trusted to be well-formed") to the one input class this note had exempted by omission. `KELD-AUTH-003` is request-construction only; if a socket was attempted it is `004`. Rejected: dropping provider text, which would destroy the only signal distinguishing `access_denied` from `invalid_scope` | §4 code table, §7 rows `KELD-AUTH-*` untrusted / redirect-mode |
| D17 (`linear-comment:413725f1-3b9e-449c-816a-08f72c96e4eb`) | **AC11's single-flight key is canonical, and `Token.scopes` is pass-through.** The key is `(account_id, canonical_scopes)` — duplicates collapsed, order ignored, case-sensitive, absent ≡ empty — on the authority of RFC 6749 §3.3, which makes order irrelevant and the strings case-sensitive and defines no equality relation of its own. `Token.scopes` reaches the IdP unchanged and is never compared against the establishing session's `granted_scopes`; the cost, no granted-scope echo on the `Token` reply, is a §4 non-claim with the echo named as a destination. Rejected: treating `granted_scopes` as a ceiling (a scope constraint smuggled one capability below D15, and a pre-exchange discriminator over host state) | §3 AC11, §4 Request surface, §7 rows 11/11b/11c, §4 |
| D11 | **`auth.token` scope resource is the issuer URL literal** (same vocabulary as `auth.begin`); `account_id` is a runtime selector resolved against the host's session registry, never an authorization scope (the enforced predicate is **liveness in this host session**, not per-principal establishment — §4 non-claim; errata pass 6 corrected an earlier "a session this principal established", which asserted a binding v0 does not enforce); per-account authorization = named destination tier | §3 AC9b/AC13, §4 capabilities + types, §7 rows 13a/13b, §1 destination tiers |

#### D11 errata (2026-09-09) — `auth.token` scope resource

Recorded on KEL-89 as `linear-comment:9498d983-5c21-4979-bd4e-5fc6c582d601`
(owner-delegated, revocable), resolving blocker **B1** from the auth/06 promote
PR's isolated review (`linear-comment:523e4755-1f22-473c-bfb7-d1a25269a768`,
3/3 refuters).

**Defect.** D3 made `app.auth.token` grants exact `acct:<id>` literals while
`account_id` is minted by `begin` and the policy snapshot is immutable per host
session (kel102 AC8). No operator could author a valid `auth.token` grant before
the account existed, so the capability was unshippable and T3's `auth.token`
slice unimplementable.

**Decision.** `auth.token`'s scope resource is the **issuer URL literal**,
identical in vocabulary, matching semantics and AC9b wildcard rejection to
`auth.begin`. `account_id` remains in `AuthRequest::Token` as a runtime
selector. AC13 adds the enforcement the spec previously lacked entirely: the
guard checks the resolved session's issuer, and an `account_id` that does not
resolve to a **live session in this host session** never yields a token.
**Two corrections, errata pass 6**, because this paragraph was written in pass 1
and both of its clauses were overtaken by later passes that did not revisit it:
it is *not* "a session this principal established" — per-principal
establishment is unenforceable while `Principal::AppProcess` carries no
identity payload, and §4 records that as an explicit non-claim; and it is
*not* "a typed error" — pass 3 removed `auth.token`'s pre-Allow error class
entirely, so an unresolved id routes through the same guard call on the
reserved sentinel and comes back as a **denial** (AC13(a)/(b), §4 Errors).
Stating either of the old forms would claim a guarantee the implementation does
not make.

**Rejected for v0: manifest-declared account alias.** An alias would give the
operator a slot name that the runtime binds to whichever account the human
picks, so it does not constrain *which* human or account signs in. It would,
however, buy two real constraints — a **cardinality cap** and **rebind-deny** —
which D11 gives up (see the trade below). It is rejected on cost, not on
worthlessness: it needs an alias namespace, a binding state machine, collision
and rebind rules and new manifest syntax, and it re-opens the
account-management surface D4 deliberately deferred, to buy a constraint v0 does
not yet need (root `AGENTS.md` YAGNI). Recorded in note 228 §6 with the note's other
rejected alternatives.

**Trade recorded honestly (corrected after independent review, 2026-09-09).**
An earlier draft of this errata called the alias "authorization theater" and
said the lost authority "was never enforceable". Two of three grounds for that
were wrong and are withdrawn:
- **Cardinality is real and is given up.** Alias slots would cap how many
  sessions an app may hold at a granted issuer. Under D11 an app with
  `begin` + `token` at one granted issuer may establish and tokenize an
  **unbounded** number of accounts there, each Allowed identically. v0 defers a
  cap (no non-arbitrary value exists yet, and a wrong cap breaks legitimate
  multi-account apps); it returns with the per-account destination tier.
- **The durable-storage objection was a strawman.** A host-session-scoped
  binding needs no durable storage — AC13 scopes its own enforcement to
  exactly that session. That leg of the rejection is withdrawn.
- **What survives** is the authorability requirement plus YAGNI cost: the alias
  needs a namespace, a binding state machine, collision and rebind rules and new
  manifest syntax to buy a constraint v0 does not yet need, while the issuer
  literal is authorable today with zero new guard code and zero new syntax.
[inference] On the rungs verified to date, reaching an account requires a
`begin` the human completed in the OS broker or the external user agent; PRT
backed silent SSO on an Entra-joined device is **untested** (241 §7) and
`AuthRequest::Begin` carries a child-supplied `login_hint`, so this bound is
conditional, not a guarantee — it is recorded as a non-claim in §4.
AC13 is strictly narrower than before in one dimension that previously had
no constraint at all. `auth.begin` and `auth.token` stay separate capabilities;
the meaningful direction is `begin` without `token` (interactive login without
silent token retrieval) — `token` without `begin` is vacuous, since only
`begin` creates sessions.

**Migration path for the per-account tier (evaluated, recorded).** A third
option — a **union vocabulary** where `app.auth.token` accepts either a declared
issuer literal or an `acct:<id>` literal in the same exact-match list — was
considered. It is rejected **for v0 only**, because `account_id` is host-minted
and host-session scoped (§4 registry), so an `acct:` entry would be
unauthorable today and would ship vocabulary that cannot be used — the
"exists only to look complete" case root `AGENTS.md` YAGNI forbids. It is
recorded as the **migration path**: when durable account identity exists, an
`acct:` literal joins the same list with no manifest-schema break and no guard
change (both are byte-equal literals on the existing matcher; the host attempts
the declared issuer and the account literal against the same node). That is
also what durable identity changes about the earlier "theater" objection: a
*stable* id constrains a persistent account, where an alias only labelled
whichever account was picked.

**Backward compatibility.** `acct:<id>` was never shippable, so no manifest can
depend on it; a pre-existing `acct:`-prefixed entry under `app.auth.token` now
matches no issuer literal and fails closed, consistent with §4's
pre-existing-key policy.

#### T2 result addendum (2026-09-09, in-place dated update; the D11 errata in the same 2026-09-09 revision changes §1/§3/§4/§6/§7/§8/§4/§10 as well — the `revision:` header field owns the full scope statement, not this heading)

D1's own text said the WAM rung is "claimed only after a token is acquired on
hardware." That happened. Two real-hardware runs (device Ramani, Windows 11
25H2) verified `IWebAuthenticationCoreManagerInterop::RequestTokenForWindowAsync`
from an **unpackaged** exe:

- 240 (`docs/research/library/auth-distribution/240-wam-unpackaged-spike.md`) (2026-09-09, no operator tenant available):
  every credential-independent step succeeds — unpackaged proof
  (`APPMODEL_ERROR_NO_PACKAGE`), interop factory resolves, a real top-level
  HWND is accepted, WAM presents a modal broker dialog owned by that HWND with
  no pre-UI package-identity rejection. Token issuance itself was
  `awaiting:operator`; no verdict.
- 241 (`docs/research/library/auth-distribution/241-wam-operator-run.md`) (2026-09-09, operator's own throwaway Entra
  app registration + personal Microsoft account): `ResponseStatus = 0
  (Success)`, a JWT-shaped token, on both the interactive call and the silent
  follow-up (`GetTokenSilentlyWithWebAccountAsync`, 728 ms, no UI).

**D1 verdict: GO.** The Windows WAM rung (§4 rung table, AC6) is confirmed
reachable and functional from an unpackaged `keld-host`-shaped exe for a
**personal Microsoft account signed in through a multitenant Entra registration
at `authority=common`**; the Entra work/school (PRT-backed) path was not
exercised (241 §7). It did not by itself resolve the promote PR's review
findings. Of that review's **three** blockers, two are addressed in this
revision — **B1** (`auth.token`'s `acct:<id>` scope unauthorable before login,
by D11/AC13 above) and **B2** (stale Linux premise, by the refreshed §4
paragraph plus §6 T6 and note 228 §5) — pending re-review; **B3** (no independent
review of the promote PR's final tip) remains open and is not something this
note can close. T3/T5 implementation still waits on
KEL-102/T3 regardless of D1's result. 241 §7 also names two residual Windows
unknowns not resolved by this addendum: whether a single-tenant app
registration needs the `authority` parameter as a full URL, and
Entra-joined/PRT-device behavior.

### Explicit non-claims

No OS containment (per-OS `unverified`, kel78:198). No device-compliance CA on
macOS/Linux. No claim of WAM behavior on an Entra-joined/PRT device or of the
single-tenant authority-URL format (241 §7 — untested by the T2 spike's
multitenant/MSA run), and therefore **no claim that every account an app can
reach required a human to complete a broker or external-user-agent dialog**:
that bound holds on the rungs verified to date, but PRT-backed silent SSO is
untested and `begin` accepts a child-supplied `login_hint` (D11 trade ledger).
No cardinality bound on how many accounts an app may hold at a granted issuer
(D11 gives that up; it returns with the per-account destination tier).
**No manifest bound on requested OAuth scopes** (errata pass 4): the manifest
constrains the issuer only, so an app granted `auth.begin`/`auth.token` at issuer
P may request any scope set at P — including scopes far broader than the sign-in
the grant was authored for. The **only** enforced boundary is the issuer: the
IdP's consent screen is provider-side, is neither observable nor requirable by
the host, and is skipped entirely on the silent path 241 §4.3 measured at
728 ms. A profile-side or manifest-side scope constraint is a named destination
tier (§4 Request surface). Stated here because an unstated widening is worse
than a disclosed one; D15 records the choice and the alternative it rejected.
**No granted-scope echo on `auth.token`** (D17, errata pass 6): the host passes
`Token.scopes` to the IdP unchanged and never compares it to the session's
`granted_scopes`, and `AuthResponse::Token` carries no scope field, so a child
that asks for a broad set and receives a narrower token cannot learn that from
the reply. The IdP's own error surfaces on refusal (`KELD-AUTH-013`), but silent
narrowing is invisible in v0. Adding the echo is a public-API change with its
own gate and is a named destination, not a v0 property.
Account-existence concealment **is** claimed for `auth.token` and is a tested
property — but only because of D12, and the history matters. Three earlier
revisions claimed it on the strength of AC13(a)'s sentinel alone; that was
**false**, because `DenyReason` interpolates the requested resource into its own
fix text and `CallError` copies it to the child verbatim, so the four replies
differed and a caller granted issuer P could additionally read the declared
issuer of a live session at an issuer it held no grant for. Both were reproduced
by execution against the shipped guard. AC13(c) is what actually closes it: the
denial's `message` is a constant, so within a grant level the replies are
byte-identical (row 13c compares encoded bytes and carries a negative control
against a build that forwards `DenyReason`). The claim is scoped to **reply
content**: no constant-time or timing property is claimed, and the resolve step
does more work on a hit than on a miss, so a timing side channel is possible and
is explicitly **not** closed here. What is
**not** claimed is that the property survives KEL-102/T5 role generations
unexamined — a per-principal registry changes what "resolves" means and MUST be
re-derived there. **No claim that an app can only reach
sessions it established** — the enforced predicate is liveness in this host
session (AC13(a)); per-principal establishment is not expressible while
`Principal::AppProcess` carries no identity payload. No
scheme-hijack immunity for
OS-registered handlers (the in-process ASWebAuthenticationSession initializer
callback not transiting OS scheme dispatch is the only defensible narrow form).
No ephemeral-session isolation. No `canceledLogin` diagnosability on macOS 15.x.
No permanent refresh lifetimes. No durable refresh custody in v0. This spec
*targets compliance with* Google/RFC 8252/OAuth 2.1/Entra/Okta/Auth0 policy walls
— conformance is proven per-AC, never asserted as certification.

## 5. Boundaries

- Implement in (after approval, in order of the task ladder): `crates/keld-native`
  (auth module), `crates/keld-ipc` (channel registration via existing dispatch),
  `crates/keld-guard` (AC9b load-time rejection of malformed auth grant
  entries — `**` and control characters, D3/D13 — and nothing else),
  `crates/keld-cli/src/mcp/permissions.rs` (AC9b's new `ManifestError` variant
  forces an arm: that `match` has no wildcard, so the addition is a
  cross-crate compile break, not an optional follow-up — errata pass 7),
  `crates/keld-core` + the `keld-cli` boot compiler (D14: the `keld.config.ts`
  `auth` section and the optional `auth` field on the boot descriptor),
  `keld-cli` doctor (optional T7), `docs/engineering/keld-error-codes.md` (one
  `## KELD-AUTH-…` heading per shipped code — errata pass 4; the file is
  CI-enforced by `crates/keld-cli/tests/error_registry.rs`), architecture tables
  in the same PR.
- MUST NOT: invent Unique #5; embed IdP login in `keld-wv`; store refresh in the
  child; bypass `dispatch_privileged`; claim KEL-78 containment or
  device-compliance support; copy the echo bypass; land the reserved D4 names as
  code (enum variants, channel registrations, or grant keys) in v0 — the
  reservation is documentation only; stage research from Keld root.
- Must-not-touch files (implement PRs): workspace `Cargo.toml` pins except the
  named T4/T5/T6 gated additions; kipc frame header/HELLO constants
  (`02-ipc.md` §2); `keld-runtime` sandbox/admission internals; generated
  packages; `docs/architecture/03-security.md` except the same-PR
  capability-table rows this spec already requires.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

- [x] T1 Review this note; promote the approved revision to
  `docs/specs/keld-auth.md` (Status: approved). No crate code. The reviewing
  authority is the owner or the recorded delegated-approval mode (§8); a
  promotion checking this box MUST state which of the two discharged it and
  MUST NOT imply an independent human review that did not occur.
- [x] T2 Spike (real Windows 10 20348+, Entra tenant): unpackaged
  `RequestTokenForWindowAsync` go/no-go. **GO, 2026-09-09** — see §4
  addendum and 241 (`docs/research/library/auth-distribution/241-wam-operator-run.md`). (The original no-go fork —
  record loopback as Windows default, open the keld-pack identity destination
  item — did not trigger.)
- [ ] T3 Loopback + PKCE + external browser on one OS (macOS or Windows) behind
  `auth.begin`/`auth.token`: falsifies AC1, AC2, AC4, AC5a, AC5b, AC9, AC9b,
  AC10, AC11, AC13, AC14. **Depends on KEL-102/T3 (core admission +
  `keld-core → keld-native` edge) — the declared predecessor.**
- [ ] T4 macOS `ASWebAuthenticationSession` rung: AC7, AC7b, AC12.
- [ ] T5 Windows WAM rung (**T2 gate cleared — GO**): AC6. Still gated on
  KEL-102/T3 (T3's declared predecessor) and on the `windows` WAM-feature
  dependency review (§8).
- [ ] T6 Linux: component harness for AC4/AC5a/AC5b/AC8/loopback-parse now (errata
  pass 8 names AC8 here; it previously had no owning task, and §7 folded it into a
  bundled platform row with criteria belonging to T4 and T5). **B2
  refresh (2026-09-09):** KEL-96/T4 has landed (`c41ec5e`, #145) and
  `surface.no-flag-linux` is `partial`, so e2e activation is a scope decision at
  implement time on that partial surface, no longer a blocked dependency edge
  (§4 Linux availability; D10's decision unchanged).
- [ ] T7 Optional: `keld doctor` Windows scheme-observation check (zero new deps).
  AC3 has no owning task and errata pass 10 records that rather than inventing
  one: its trigger is not expressible in the v0 request surface (§4 code table,
  `KELD-AUTH-001`), so there is nothing for a task to falsify until a rung that
  could raise it exists. §4's "which PR owes which heading is derived, not
  listed" therefore terminates without an owner for that one code — the same
  shape pass 8 closed for AC8, left open here deliberately because AC8 had a
  home to go to and AC3 does not.
- [ ] T8 Prerequisite (not auth code): stage the project's real
  `keld.permissions.jsonc` in `keld dev` (today a hardcoded `{}` is staged), so
  auth grants are exercisable end-to-end in dev (226 F17).

## 7. Test plan

| AC | Type | Anti-flake notes |
| --- | --- | --- |
| 1 | ipc integration (post KEL-102/T3) | empty manifest; side-effect flag never sets |
| 2 | integration | mock token endpoint returns refresh; assert reply bytes lack it |
| 3 | unit | no embedded-UA begin mode exists; typed rejection |
| 4, 5a | unit/integration | capture authorize URL + bound addr; `code_challenge` present **and `code_challenge_method=S256`** — errata pass 8: asserting only presence let a `plain` challenge, or an omitted method (which RFC 7636 defaults to `plain`), satisfy the row while violating AC4; port from bind 0; and the redirect host **equals the resolved profile's recorded D7 form**, table-driven over the profile table rather than hard-coded. Errata pass 7: this row asserted the literal `127.0.0.1`, which **no v0 profile emits** — `entra-common` is the only row and selects the `localhost` D7 exception, so the old assertion could only be satisfied by making `entra-common` emit `127.0.0.1` and forfeiting the Entra port-flexibility guarantee D7 exists to keep. AC5a's `127.0.0.1` default needs a profile that selects it; until one exists, cover the default with a synthetic profile fixture and mark it as such |
| 5b | unit | exact-literal mode with port 0 → typed config error |
| 6-8 | platform integration | `cfg`-gated; explicit "not run" on other OS — never silent pass |
| 7b | unit (mac runner) | compliance-class request → typed deny naming Company Portal |
| 9 | unit | cross-issuer → `OutOfScope` |
| 9b | unit | manifest with `"https://**"` in `app.auth.begin` fails load with typed error |
| 13a CI | ipc integration (post KEL-102/T3) | manifest authored **before any login** grants `app.auth.token: ["https://login.microsoftonline.com/common"]` — the `entra-common` profile's declared issuer literal (§4 profile table) — with no wildcard/sentinel; a host-injected fake session carrying that declared issuer makes `auth.token` for its `account_id` Allow; a second fixture granting a different issuer literal denies `OutOfScope` for the same call. No IdP, no network |
| 13a real | platform integration | the same manifest literal, after a real `begin` at that profile, Allows a real post-login `auth.token` — including when the IdP's returned `iss` **differs from the declared issuer literal** (for `entra-common` the observed `iss` is `https://sts.windows.net/<tenant>/` against a declared `https://login.microsoftonline.com/common`, so the assertion is string inequality plus an Allow, which is falsifiable; an earlier "names a **different tenant**" wording was not, because `common` names no tenant). `cfg`-gated; explicit "not run" on other OS — never a silent pass |
| 13b | ipc integration | **oracle closure, both grant levels.** Ungranted principal: a resolvable and a fabricated `account_id` both return `NotGranted` — byte-identical replies. Principal granted issuer P: an id resolving to a live session at issuer Q ≠ P and a fabricated id both return `OutOfScope` — byte-identical replies. No token, no exchange, no broker/listener in any of the four cases. v0 has one `AppProcess` principal and a memory-only registry, so cross-principal and previous-host-session ids are one failure class with the fabricated case by design — not separate falsifiers |
| 14 | ipc integration | unknown `provider` key → typed pre-Allow `KELD-AUTH*` with no broker, scheme handler or listener started (assert the side-effect flag never sets); known `provider` under no grant → `NotGranted` from the single guard call, still no side effect |
| profile table | unit | every key in the v0 provider-profile table resolves to its tabled declared issuer literal (one row ships after D16), and a key outside the table resolves to nothing, feeding AC14's `KELD-AUTH-002`. Table-driven over the compiled-in table itself, so adding a row cannot silently skip the assertion, and the assertion does not depend on how many rows there are |
| `KELD-AUTH-*` codes | unit | every typed-error variant returns its tabled code from `code()` and its `Display` **starts with** that code and is non-empty after it — the `keld-guard` `assert_code` shape. Exhaustive `match` over the error enum, so a new variant fails to compile rather than escaping the table. Registry coverage is asserted by the existing `crates/keld-cli/tests/error_registry.rs`, not re-implemented here |
| `KELD-AUTH-*` untrusted provider text (D18(a)) | unit | table-driven over hostile provider payloads: an `error_description` of 4 KiB truncates to the 256-byte bound with the truncation marked; one carrying an escaped CR-LF pair and a forged `KELD-GUARD002: ...` line renders with **Keld's** code as the first token and the provider text quoted in its delimited position, so no second code line can be parsed out; one containing C0/C1 control characters renders with them stripped and non-ASCII preserved. Negative control: a build that interpolates provider text unsanitised fails the forged-code-line row |
| redirect-mode tiebreak (D18(b)) | unit | `fixedRedirect` configured and its port unavailable yields `KELD-AUTH-004`, not `KELD-AUTH-003`; `fixedRedirect` configured with no fixed port to use, before any socket, yields `KELD-AUTH-003` |
| `login_hint` | unit | a 257-byte hint and a hint containing `\r\n` each fail with `KELD-AUTH-010`; a hint carrying a **non-ASCII** identifier (an RFC 6531 address) is **accepted**, guarding the pass-4 correction; a valid hint reaches the authorization request percent-encoded as one parameter value. Assert the rejection message does **not** contain the offending value (PII), and that validation runs after Allow (AC14(c)) — an ungranted caller sends a malformed hint and still gets `NotGranted`, byte-identical to the well-formed case |
| 13c (D12 deny rendering) | ipc integration | the four AC13 replies are compared **byte for byte** on the encoded `CallError`, not on a struct: ungranted+resolvable vs ungranted+unresolved must be identical, and grantedP+liveQ vs grantedP+unresolved must be identical. Negative control: a build that forwards `DenyReason::to_string()` must FAIL this test — assert the constant message contains no issuer literal, no scope list and no account id. The capability's own json pointer (`/app/auth/token`) **is** permitted and expected — it is invariant across all four cases, so it carries no information; AC13(c) requires the message to name the manifest key to edit |
| 9b control chars (D13) | unit | a manifest whose `app.auth.token` list holds `"\u0000unresolved"` (JSON-escaped NUL) fails load with AC9b's typed error. Negative control, written inside `keld-guard` so it stays writable after D13 lands: bypass the load-time check by deserializing the same fixture straight into `PermissionsManifest` and assert `evaluate` returns `Allow` for the sentinel — proving the rejection, not the matcher, is what closes the defect |
| 13d (D13 invariant) | ipc integration, **beside row 13c** — errata pass 9 corrects pass 8, which prescribed `keld-guard` on two false premises: `keld-guard` cannot reach `dispatch_privileged` (it lives in `keld-ipc`) or the `keld-native` broker whose synthesized reply this row asserts, and it declares no `[dev-dependencies]` either, so pass 8's stated reason for avoiding `keld-native` applied to it identically. The fixture also needs no particular crate's `serde_json`: `PermissionsManifest` derives `Deserialize` publicly, so **any** crate with a `serde_json` can build the sentinel-allowing manifest that D13 rejects at load — verified from an out-of-tree harness. "Forced" MUST NOT mean stubbing the decision: `dispatch_privileged`, `evaluate` and the sentinel all run | a host driven into Allow-with-no-resolved-session mints no token and replies with the synthesized granted-level denial — assert byte-identity against the **grantedP pair only** (the ungranted pair differs by `code` by design), so the invariant path is not a distinguishable fifth reply; assert no broker, listener or exchange ran |
| client id (D14) | integration | a `keld.config.ts` with `auth: { "entra-common": { clientId } }` is compiled into `keld.boot.json`, survives `BootDocument`'s `deny_unknown_fields` parse, reaches the `keld-native` auth module, and the authorization request carries it. A descriptor **without** the optional `auth` field still parses (schema stays `1`). A profile with no `clientId` refuses `KELD-AUTH-015` **after** Allow with no side effect — and an **ungranted** caller sending that same known provider gets `NotGranted`, byte-identical to the configured case, so configuration state is not a pre-Allow oracle |
| compat (c) | unit | fixture manifest with a pre-existing `acct:`-prefixed entry under `app.auth.token`: assert it matches no declared issuer and denies `OutOfScope` at runtime (deliberate fail-closed, §4 backward compatibility case (c)) |
| compat (a) | unit | fixture manifest whose `app.auth.begin` list holds a bare `"**"` entry fails load with AC9b's typed error (deliberate fail-closed break, §4 backward compatibility) |
| 9b both capabilities | unit | AC9b binds `app.auth.*`, so **both halves of the predicate are asserted on both grant lists**. Wildcards: a bare `"**"` and a `"https://**"` under `app.auth.token` each fail load, alongside the `begin` cases above. Control characters: a NUL-bearing entry under `app.auth.begin` fails load, alongside the `token` fixture. Errata pass 7 added the wildcard symmetry and pass 8 the control-character symmetry — by pass 7's own argument, a regression scoping either half to one capability would otherwise have passed the suite |
| compat (b) | integration | fixture manifest with a pre-existing exact-literal `app.auth.begin` grant: assert it is a live Allow once the channel registers (deliberate activation, §4 backward compatibility) |
| 10 | unit | drop flow-state struct; verifier gone; no sleep-sync |
| 11 | integration | two concurrent `auth.token` for the same account and the same scope set → exactly one exchange observed, and both callers get that exchange's token |
| 11b (D17 canonical key) | unit | the single-flight key is table-driven over pairs that MUST coalesce and pairs that MUST NOT. Coalesce: `["b","a"]` vs `["a","b"]` (order), `["a","a","b"]` vs `["a","b"]` (duplicates), absent vs `[]` (absent ≡ empty). Do **not** coalesce: `["a"]` vs `["A"]` (case-sensitive per RFC 6749 §3.3), `["a"]` vs `["a","b"]` (superset), same scopes under a different `account_id`. Negative control: an implementation keyed on the raw sequence fails the order and duplicate rows, and one that lowercases fails the case row — so the test distinguishes the rule from both neighbouring rules |
| 11c (D17 pass-through) | integration | a `Token` request whose `scopes` exceed the establishing `begin`'s `granted_scopes` is **not** refused by the host: assert the outbound token-endpoint request carries the requested set byte-for-byte, that no host-side comparison against `granted_scopes` occurs, and that an IdP refusal surfaces as `KELD-AUTH-013` carrying the provider's error. Assert `AuthResponse::Token` has no scope field, pinning the §4 non-claim rather than leaving it prose |
| 12 | unit | cancel-class result → non-terminal typed error; no automatic re-invocation |

Bind port 0; await conditions; no fixed ports; no wall-clock sleeps
(`.agents/testing.md`).

## 8. Review gates triggered

| Gate | This note / promote PR | Implement PRs |
| --- | --- | --- |
| Permission model | **Yes — owner sign-off or the recorded delegated-approval mode** (kel102 form), covering: new `auth.*` grants; exact-literal scope semantics; AC9b's load-time rejection of malformed auth grants on the shared load path — `**` **and, per D13, control characters**, which changes which manifests load at all; **(D14) an operator-authored `keld.config.ts` section and a boot-descriptor field becoming load-bearing for an auth flow**; **(D12) the change to what a denied principal observes**, which is a permission-model observable and not merely a public-API one; **and (D11 errata) AC13/AC14's resource derived from host state — a profile table for `begin`, the session registry for `token` — plus their resolve-then-authorize ordering and the recorded existence-oracle non-claim** — the largest permission-model delta in this note; **and (errata pass 4) the enumerated provider-profile table contents, which fix exactly which issuer literals an operator can grant, together with §4's recorded "no manifest bound on requested OAuth scopes" position**; **and (D17) `Token.scopes` reaching the IdP unchanged with no host-side check against the establishing session's `granted_scopes`, which fixes how far a `token` grant reaches at an authorized issuer** | re-reviewed per diff |
| Public API | **Yes — owner sign-off or the recorded delegated-approval mode** (CALL/REPLY shapes and their normative wire names, the typed error contract — now the enumerated `KELD-AUTH-001`–`015` table in §4 plus its `docs/engineering/keld-error-codes.md` registry entries; **(D12) the narrowing of `call_error.rs`'s "message is the error's full `Display` text" for `auth.token` denials**; **(D12, boundary 2) the auth module re-deriving the guard-denial → wire mapping that `impl From<&DenyReason> for CallError` documents itself as the single owner of** — AC13 calls this the heavier of D12's two; **(AC13(d), boundary 3) the auth module synthesizing a `KELD-GUARD002` reply with no `DenyReason` behind it**, against `call_error.rs`'s and `crates/keld-ipc/AGENTS.md`'s rule that `code` is "owned by the crate that produced the failure" — the T3 PR MUST reconcile that nested crate invariant in the same commit or state the blocker; **(D14) the new `keld.config.ts` `auth` section, the optional `auth` field on the boot descriptor, and the new public `keld-core` surface needed to carry it past `ParsedBoot`** — three contracts, not one; **and `keld-guard`'s public surface**: AC9b needs a new `ManifestError` variant, which is a breaking change to a public non-`#[non_exhaustive]` enum and also forces an arm in `crates/keld-cli/src/mcp/permissions.rs`, whose `match` has no wildcard (errata pass 7 added boundaries 2 and 3, the two extra D14 contracts and the `permissions.rs` consequence; AC13's "all three are listed in §8" was false until it did) | re-reviewed per diff |
| Dependency | none (prose only) | named gates: `windows` WAM features (T5), `objc2-authentication-services` (T4), `zbus`/`ashpd` (T6) |
| Wire protocol | none (new channel payload only; frame untouched) | bump only if `02-ipc.md` §2 constants change |
| unsafe | none (D8 clause) | owner decision executed in the first FFI PR (§4 gate-deferral clause) |

KEL-208 (the general URL-glob matcher defect, §4; KEL-209 is its unlinked duplicate) **landed** as PR #203 and was **never** a gate of this
spec and MUST NOT be reviewed or waived through it: it changes the manifest
contract for `net.connect`/`shell.open`, which §5 forbids these PRs from
touching. It carries its own permission-model gate on its own issue.

## 9. Perf impact

none — interactive auth is cold-path; no async runtime on kipc hot paths; no
budget movement claimed; no bench required for promotion.

## 10. Open questions

**Decisions:** none open. D1–D18 are resolved (owner-delegated on KEL-89:
`linear-comment:f2178f64-6f30-4431-8906-271b7e7ce458` for D1–D10,
`linear-comment:9498d983-5c21-4979-bd4e-5fc6c582d601` for D11,
`linear-comment:973acac2-2461-46f6-886e-6a2ebeb77b4b` for D12–D16,
`linear-comment:413725f1-3b9e-449c-816a-08f72c96e4eb` for D17,
`linear-comment:68410a82-52db-438a-a6ac-17420010aeaf` for D18; all revocable). D12–D16 were **approved by the owner in-session**, transcribed by
the agent (`linear-comment:1b34cf85…`) and then **ratified by the owner from
their own Linear identity** (`linear-comment:e27751b8…`, 2026-09-09T16:25Z),
which is the same attestation D11 carries in `38dab27a…`. The provenance
limitation earlier revisions recorded here is closed; errata pass 6 records
that, because this note's previous blob was cut seven minutes before the
ratification landed.
The T2 WAM spike was a conditional task outcome under D1, not a blocking human
decision, and it resolved GO on 2026-09-09 (§4). The permission-model item
the promote PR's review raised — `auth.token`'s scope resource being
unauthorable before login — is closed by D11/AC13.

**Specification gaps that block T3 implementation: none.**

That sentence has been wrong before, so it is worth saying what makes it
checkable rather than repeating it. Errata passes 3, 4 and 5 each recorded gaps
here and closed them; pass 5 wrote "none" while seven contradictions remained,
which three independent reviews of the promote PR's tip then found; **errata
pass 6** closed those seven; and **errata pass 7** closed a further eight that
the *next* three reviews of the next tip found — among them a Public API gate
that listed one of the three boundaries it gates, a `§7` row asserting a
redirect literal no v0 profile emits, and a D14 premise refuted by the shipped
host binary. The pass-by-pass record lives in this note's `revision:` field,
which owns it; note 228 §6 owns the rejected alternatives; and each closure is stated
where it binds rather than only here, so a reader who never opens this section
still meets it.

What is carried rather than closed, and blocks nothing: `KELD-AUTH-001`'s AC3
trigger is not expressible in the v0 request surface, so the code is specified
and its AC3 test asserts the refusal rather than a wire path — AC3 is not in
T3's task list, and §4's code table says so where an implementer meets it.
D16's one-row table also leaves AC9's and row 13b's cross-issuer arms reachable
only through a host-injected session or a grant literal naming no profile;
row 13a CI already names its fake session, and row 13b must do the same.

This section stays in the promote copy per `docs/agents/spec-template.md`
("write none rather than deleting"). B3 — an independent review of the promote
PR's final tip — is a PR gate, not a note item; no note revision closes it (note 228 §9).
