# Spec: KEL-142: delivery

Part of: [index](index.md)
Owns: sections 5, 6, 7, 9 and 10

## 5. Boundaries

The approved text has no boundaries section. The index non-goals state the scope limit.

## 6. Tasks

The approved numbered list sets the order, so each task names the previous task.

If any step requires page access to the raw native handler, a second app-link
transport, caller-selected identity, an unbounded pending queue, or a KIPC
frame/version change, stop and revise this spec instead of widening
implementation ad hoc.

### T-1: extract the lifecycle session

- **Blocked by:** none

Extract one singleton lifecycle/app-link session into `@keld/api`; make
`@keld/electron` consume it; keep current lifecycle tests green.

### T-2: add the Echo channel path

- **Blocked by:** T-1

Add the declared Echo `AppChannel` descriptor and one `channels.handle`
path over that same reader/writer.

### T-3: add the wv-link decoder

- **Blocked by:** T-2

Add pure `keld.wv-link/v1` decoder + deterministic hostile table + fuzz
target.

### T-4: install the facade and isolated handler

- **Blocked by:** T-3

Install page facade + isolated WKContentWorld handler in the existing macOS
WebView configuration before navigation.

### T-5: route one host-minted CALL

- **Blocked by:** T-4

Route one bounded host-minted CALL through the current admitted app-link,
with one pending entry and stale-document retirement.

### T-6: run the gates and macOS acceptance

- **Blocked by:** T-5

Run focused unit/hostile tests, full repository gates, then the real
macOS OS-visible acceptance matrix.

## 7. Test plan & evidence

### 8. Fuzzing and deterministic hostile tests

KEL-142 introduces a new untrusted decoder: `keld.wv-link/v1`. Therefore
fuzzing **does apply**.

Required:

- a raw-byte `cargo-fuzz` target for the pure renderer-envelope decoder;
- accepted fuzz inputs must satisfy every version/kind/integer/bounds/no-extra
  invariant;
- no panic, unbounded allocation, or accepted identity/authority field;
- every discovered bug becomes a permanent deterministic regression.

Fuzzing does not replace navigation-generation tests, real WKContentWorld
isolation tests, slow-consumer/pending-call tests, subprocess/app-link
disconnect tests, or real OS-visible click evidence.

## 9. Perf impact

The approved text records none.

## 10. Open questions, blockers & unknowns

The approved text records none.
