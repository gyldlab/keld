# Design 8: Remaining design items

Part of: [index](index.md)
Owns: Design 8: Remaining design items

## 4. Design: Remaining design items

- **New or changed types:**
  - a private `begin_echo_call` refusal enum in keld-core, with variants
    `Quitting`, `StateUnavailable`, `Busy`, `NoGeneration` and
    `WriteFailed(HostAppError)`;
  - in keld-wv (all private):
    - `PendingRequest.document: String`;
    - `PageOutcome.document: Option<String>`;
    - `BridgeState::settle` returns `Option<String>`;
    - `BridgeState::navigation_started` returns `Option<PendingRequest>`;
  - no public Rust type changes. `RendererBridgeRequest` and
    `RendererBridgeOutcome` are unchanged.
- **TypeScript:** `dispatchApplicationCall` throws a typed no-handler error,
  and `resolveEchoCall` maps it to `KELD-API-002`. Every other throw still maps
  to `KELD-API-001`.
- **Capabilities and manifest:** none.
- **Wire and protocol:**
  - KIPC: no frame, field or version change. New *values* travel in existing
    string fields, and the host now rewrites one value class (S10b).
  - `keld.wv-link/v1`: the private native→isolated result gains one field,
    `document` ([historical §4.7](design-7.md)). The isolated→native envelope is unchanged. See §8.
- **Runtime seam:** none. The execution owner, crash domain and ordering are
  unchanged.
- **Persisted state:** none.
