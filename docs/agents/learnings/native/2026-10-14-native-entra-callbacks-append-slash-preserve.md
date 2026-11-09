---
name: "2026-10-14-native-entra-callbacks-append-slash-preserve"
description: "Entra callbacks may append a slash; preserve raw redirects and validate callbacks, not external browser tabs."
area: "native"
tags: ["native", "entra", "callbacks", "append", "slash", "macos"]
platform: "macos"
recorded: "2026-10-14"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L179"
related: []
---

## Historical source

> - 2026-10-14 [native/macos] Entra no-path query callbacks may add one trailing slash: keep the operator raw redirect immutable on authorize+token legs, but validate the exact permitted callback representation before the shared state/single-use parser. ASWebAuthenticationSession on macOS can use the default browser; an external tab does not prove T3 fallback. Actual static AS-start/owned-callback/base-cut trace attributed AUTH011 before correction. (evidence: KEL-89 comment dfc19a0a-9fec-4a7e-a607-5ad1878d708b; Microsoft reply-url restrictions; Apple ASWebAuthenticationSession documentation; primitives::tests::as_return_empty_path_slash_keeps_state_and_raw_oauth_binding; primitives::tests::as_registration_and_callback_reject_aliases_and_wrong_state)
