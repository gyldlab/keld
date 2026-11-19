# GH-451 diagnostic: buffered native Quit crosses a recovered subscriber

**Status: CONFIRMED RED; unresolved lifecycle contract. Not an accepted test
or an installed-app result.** This record keeps the deliberate regression
falsifier outside the registered test suite. It must turn green before a
source-generation-bound native Quit can be qualified.

- Date/host: 2026-11-20, macOS arm64, CENTILLIONAIREs-Mac-mini.local.
- Isolated worktree: `gh1404-native-quit-product-20261120`.
- Pinned base HEAD before worker edits: `9d0d4666fb89c2cda3436c80cfc67d18dc10a0d7`.
- The six inherited dirty files were retained. The falsifier ran after adding
  three Core socket tests and a WV bridge-join correction; the original six
  files were not reset. SHA256 of the formatted six-file working diff before
  this report: `cadb412e7483a71cf8df3601e03ba5f3b04257fc9acec61f5eb6e9c3216e05ff`.
- SHA256 of the formatted adjacent green Core test module:
  `7f9ab8ca975c060b57b8de3a3bb346bd97b7e3d078c1180ba95f4dda038e3fda`.
- Diagnostic source belongs at the end of
  `crates/keld-core/src/app_session/menu_router/tests/native_os_quit.rs`,
  where the actual helpers `install_original`, `subscribe`, `ready`,
  `caller`, `window_owner`, and `no_quit_frame` are defined.

## Exact red invocation and observation

```console
$ cargo test -p keld-core --lib buffered_native_intent_must_not_cross_to_recovered_subscriber -- --nocapture
running 1 test
thread 'app_session::menu_router::tests::native_os_quit::buffered_native_intent_must_not_cross_to_recovered_subscriber' panicked at crates/keld-core/src/app_session/menu_router/tests/native_os_quit.rs:67:36:
unqualified native Quit must emit no frame: (FrameHeader { kind: Event, flags: 0, channel: ChannelId(3), corr: CorrelationId(0), len: 1 }, [2])
test app_session::menu_router::tests::native_os_quit::buffered_native_intent_must_not_cross_to_recovered_subscriber ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 426 filtered out
error: test failed
# Exit code: 101
```

**Expected:** the old queued AppKit request cannot publish a frame to the
successor link. **Observed:** the new link received lifecycle `QuitRequested`
unit `[2]` with correlation `0`. A current/subscribed/Ready check at dequeue
cannot identify which generation owned an already-queued native event. The
OS-to-WV and WV-to-Core `NativeQuitRequested` event types carry no origin
attempt or generation. This fixture supplies a real new app-link socket and
replaces the registered subscriber. It is a state/wire falsifier, not a
replacement for actual AppKit, renderer veto, or process exit testing.

## Reproduction source (deliberately unregistered)

```rust
#[test]
fn buffered_native_intent_must_not_cross_to_recovered_subscriber() {
    let (mut router, mut first_peer, old_guardian, _ui) =
        app::tests::initial_ready_router_with_window_loop();
    first_peer
        .set_app_link_deadlines(Some(Duration::from_secs(3)))
        .expect("first peer deadline");
    let _source = install_original(&mut router);
    subscribe(&router, &mut first_peer);
    ready(&router, &mut first_peer, old_guardian);
    let first_owner = caller(&router);
    // The AppKit event is already pending in WV/Core's channel here. Its
    // payload contains no original role identity.
    router
        .retire_window_owner(window_owner(&first_owner))
        .expect("first role retirement");
    let (replacement, mut next_peer, _new_guardian, _next_ui) =
        app::tests::initial_ready_router_with_window_loop();
    next_peer
        .set_app_link_deadlines(Some(Duration::from_millis(100)))
        .expect("next peer deadline");
    let mut successor = replacement
        .current
        .lock()
        .expect("new generation")
        .take()
        .expect("new role");
    successor.attempt = first_owner.attempt + 1;
    *router.current.lock().expect("router current") = Some(successor);
    subscribe(&router, &mut next_peer);

    // Dequeuing the old payload now incorrectly targets the replacement.
    router
        .native_os_quit_requested()
        .expect("old native intent dispatch");
    no_quit_frame(&mut next_peer);
}
```

The local diagnostic was removed from the green test module after this
observation. It is retained above for GH-451 to specify and implement an
origin-bound event identity and for native installed-app qualification. The
current `applicationShouldTerminate:` hook independently returns
`NSTerminateCancel` (AppleEvent cancellation); a later Bun Quit does not
prove the AppleEvent was accepted.
