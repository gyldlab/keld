# Spec: exact channel grants and host-owned navigation identity

- Status: draft
- Tracker: GH-541
- Owner: GYLDLAB
- Updated: 2026-11-20
- Pin: 442f3ebcef777f204cec7073932dc06edfab3a2e

Approval scope at the source revision: draft amendment for P01 and typed dispatch. The historical approval applies to its recorded bytes.

[GH #541](https://github.com/gyldlab/keld/issues/541), F04-T7. Reference: KEL-102. Implementing owners: [#552](https://github.com/gyldlab/keld/issues/552), F04-T8, and [#542](https://github.com/gyldlab/keld/issues/542), F04-T9. Contract owner: GYLDLAB. Reviewed: 2026-10-23.

Source pin: `442f3ebcef777f204cec7073932dc06edfab3a2e` (immutable review baseline). The approved technical commit is
`7e0c1b89c4767f2e6625cd2bfc4d36d86b30fc26`, based on `ef491ebd67ae794e004977a6d6578cdbb7235d61`. At approval time, main's delta from the
review pin comprised two KEL-254 Windows files, the Tokio lock entry and its CI-input digest; no reviewed Keld source or proposed
documentation target had changed. Original KEL-102 and KEL-142 approval records remain scoped to their original content. New exact-content
decision: KEL-102 comment `8fa29b34-bdb6-470f-9f97-9cdf0aa1d0eb`; decision SHA-256
`2da2c23a53848d618f08612a1106d53283b7488601be7fd3eaa5ed95038df5af`. The digest covers the decision JSON line, excluding its newline, and its
five-file manifest binds the approved technical content. That historical follow-up recorded provenance and tracker metadata only.
Independent-human approval is not claimed. No implementation or merge is authorized by this document.

Integration-base refresh: `f19c8847787aaf0e9a14e07c0af008fc5a4c2a1f` subsequently merged #1096's documentation. This branch preserves that
owner's KEL-142 §7 correction and the generated error-registry text. At that refresh, the #541 contract requirements and original decision
artifact were unchanged; #1105's implementation remains unmerged. Historical references below to #1096 being unmerged describe the
approval-time snapshot.

FACT: The [P01 owner decision](https://github.com/gyldlab/keld/issues/541#issuecomment-7046183338) binds tree
`f253c170229aad1becae72097ab2aa80159f1d2b` for the selected Windows Legacy artifact. The historical approval and the pending typed-dispatch proposal retain their separate scopes.

## 1. Goal & non-goals

Enable the minimum Electron renderer-to-main IPC needed for the macOS draw.io proof while preserving host-enforced default deny. IPC means inter-process communication: messages cross from an untrusted renderer through the Rust host to the supervised Bun application process. A channel grant permits one named message route. It does not grant filesystem, camera, shell, or other native authority.

Observable result: a current, host-authenticated main-frame document may forward an application message only when its host-bound window has that exact channel literal. Every miss fails before an app-link frame is written. Every retained request and delivery remains bound to its original live document and Bun session. Removing a guard or lifetime check must make an independently observed test fail.

Non-goals:

- Implementing #552 or #542 under this contract approval alone; each retains its own implementation-spec gate.
- Changing the KIPC frame format, allocating v2 wire fields, selecting endpoint codecs, or defining credit constants. #540 owns those contracts; #465/#543/#544 implement them.
- General window/web grants, media permission enablement, role grants, a new core WebView registry, or a second permission evaluator.
- Claiming Electron navigation equivalence without the recorded oracle owned by #551. Keld tests alone do not establish it.
- Claiming strict OS containment from a channel check. KEL-78 and the approved execution profile own operating-system confinement. The first draw.io proof's explicit legacy profile does not become strict because this amendment passes.
- General subframe IPC, arbitrary remote-content authority, automatic replay, live permission reload, native module enablement, or a fallback to ungated Echo for Electron traffic.

## Parts

| Part | Owns | Load when |
|---|---|---|
| [References and approvals](references.md) | §2 and historical decisions | Checking source approvals and ownership |
| [Acceptance criteria](acceptance.md) | §3, criteria 1–22 | Checking an acceptance criterion |
| [Design](design.md) | §4, including §§4.1–4.9 | Reviewing the policy or document lifecycle |
| [Boundaries and tasks](delivery.md) | §§5–6, including KEL-102 T7 | Assigning implementation or checking dependencies |
| [Test plan](evidence.md) | §7 and its control IDs | Selecting an independent test oracle |
| [Performance and remaining gates](decisions.md) | §§9–10 and the nine historical selections | Checking qualification or approval limits |

## Current blockers

- B-1

## 8. Review gates triggered

Permission model and public API: **required** for #541/#552/#542. Wire protocol: **required at #540's v2 boundary** and any change to completion/envelope fields; this draft cannot waive it. Unsafe: required if #542 changes native frame/delegate or WebKit lifetime operations; no unsafe code is changed in this evidence package. Dependency addition: none proposed.

Documentation: the companion patch changes architecture 03, an included source. In a complete checkout, regenerate with the repository's current generator, then run `just llms-test` and `just llms-check`. Do not hand-edit `llms-full.txt`. Rust format, warning-denied clippy, applicable full tests and required CI remain future implementation gates. None are claimed passed here.

The delegated decision records independent isolated permission/public-API and navigation/lifetime reviews, each with no blocking
specification defect, plus independent refutation of the initial-construction allegation. Review bytes were bound to the approved technical
commit. Final provenance changes require exact-final-diff review. No CodeRabbit review, full CI or native qualification is claimed by this
contract approval; unavailable tooling does not waive those gates.
