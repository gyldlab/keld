# Spec: macOS minimum renderer bridge and `@keld/api` lifecycle/channel

- Status: approved
- Tracker: KEL-142
- Owner: UNKNOWN (the approved text names no owner)
- Updated: 2026-10-31
- Pin: 606a47e319e8e7a43530e3f249ff22416fe46274
- Parent contract: KEL-139
- Approval: active user session · approved content head `2e8da1b7ba19783eceb671093620419664537415` · Linear comment `53510bc6-66e0-441c-8992-010b6a8719e6` · decision SHA-256 `8e97d6daf9fe516b3844827bf0138af66a8343e37892ea278387f4f7c503a4a7`

{"schema":"keld.kel142-approval/v1","decision":"approved","approved_content_head":"2e8da1b7ba19783eceb671093620419664537415","linear_comment_id":"53510bc6-66e0-441c-8992-010b6a8719e6","source":"active-user-voice-session-2026-09-30"}

#1321 moved this spec into the directory layout. The approved rules are unchanged, and the
approved numbers remain in the part headings. Legacy section references require a named
part and heading, not the directory alone. The historical design assumptions are dated
in [design-api](design-api.md#3-historical-design-facts-approved-2026-09-30), with separately
verified API ownership facts at `Pin`. Other historical claims remain unqualified.

> **Approved navigation companion, unmerged (GH #541).** The approval JSON,
> content head and digest above remain the historical KEL-142 approval. The
> [companion amendment](../gh541-channel-grants-navigation/index.md) records the explicitly
> marked qualifications in sections 5.2, 6, AC-5 and AC-6 in the source parts. Its separate delegated
> approval is KEL-102 comment `8fa29b34-bdb6-470f-9f97-9cdf0aa1d0eb`, decision SHA-256
> `2da2c23a53848d618f08612a1106d53283b7488601be7fd3eaa5ed95038df5af`, technical content
> `7e0c1b89c4767f2e6625cd2bfc4d36d86b30fc26`. The earlier approval is unchanged.
> GH #542 retains its own approved implementation-spec and native-evidence gates;
> this contract decision does not authorize implementation or merge. KEL-142's
> historical Done status does not qualify the new navigation implementation.

## 1. Goal & non-goals

Ship the smallest real macOS renderer-to-app slice required by KEL-139:

```text
main-frame page
  -> window.keld.invoke(...)
  -> host-owned WKWebView bridge
  -> current admitted app-link generation
  -> @keld/api typed handler in Bun
  -> matching reply
  -> same document
```

The same `@keld/api` package also becomes the single generic owner for the
already-live host lifecycle semantics: Ready, LastWindowClosed, and Quit.

This task does **not** add filesystem authority, a second app-link transport,
general schema codegen, broad Electron compatibility, recovery/replay, Windows
or Linux qualification, `send/on/stream/meta`, or a bulk plane.

Non-goals:

- KEL-140 guarded filesystem implementation;
- KEL-143 Bun recovery/restart;
- KEL-144 Rust-quarantine product acceptance;
- `window.keld.send/on/stream/meta`;
- broad KEL-80 replay/PTY/shared-memory work;
- general `@keld/schema` or `keld gen`;
- BrowserWindow or wider Electron Tier 1;
- Windows/Linux renderer bridge qualification;
- background automatic request replay;
- a second app-link socket, frame parser, HELLO implementation, deadline owner,
  lifecycle reader, or WebView identity owner.

## Parts

| Part | Owns | Load when |
|---|---|---|
| [refs.md](refs.md) | Section 2: authority and exact predecessors | Checking what this spec consumes |
| [acceptance.md](acceptance.md) | Section 3: AC-1 to AC-8 | Writing or reviewing a test |
| [design-api.md](design-api.md) | Section 4: constraints and the frozen public API | Changing `window.keld`, `@keld/api` or the Electron facade |
| [design-bridge.md](design-bridge.md) | Section 4: trust boundary, ordering, errors | Changing the WKWebView bridge, wv-link or error codes |
| [delivery.md](delivery.md) | Sections 5, 6, 7, 9, 10: boundaries, tasks, fuzzing, perf, questions | Planning or implementing a slice |

## Current blockers

none

## 8. Review gates triggered

| Gate | Applies | Reason |
|---|---|---|
| Public API | **Yes** | freezes `window.keld.invoke`, `@keld/api.app`, `channels.handle`, and channel descriptor types |
| Wire protocol | **Yes, wv-link only** | first private `keld.wv-link/v1` envelope; KIPC frame/version bytes stay unchanged |
| Permission model | **No** | no privileged/native operation or grant representation is added |
| Dependency addition | **No new package** | reuse existing `serde_json`, `getrandom`, `objc2`, `objc2-web-kit`; feature widening is reviewed in implementation diff |
| Unsafe | **Potentially yes** | direct WebKit content-world/delegate plumbing stays inside `keld-wv`; every required unsafe block needs a local `// SAFETY:` proof and review |

Human approval is required on the exact technical spec revision before source
implementation begins.
