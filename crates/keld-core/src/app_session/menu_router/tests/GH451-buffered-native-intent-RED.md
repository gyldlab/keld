# GH-451 diagnostic: buffered native Quit crosses a recovered subscriber

**Status: CONFIRMED Core socket RED after G2 Ready; unresolved lifecycle
contract. No installed-app or JS callback acceptance.** This record keeps the deliberate regression
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

## 2026-11-20 additive correction: require and consume G2 Ready

The original red diagnostic above missed a necessary independent predicate:
it transferred a fresh successor and subscribed it but did not deliver an
actual G2 `Ready` frame. Core's global `window_ready` remained true from G1;
the original outcome therefore proved **wire misrouting before a demonstrated
G2 Ready**, not that Bun's `onQuitRequested` had admitted this event.
The original source/log/hash are retained above to preserve history.

The corrected test queued a typed G1 `AppWindowEvent::NativeQuitRequested`
before G1 retirement; used `retire_generation(1)` to revoke the old
subscription and current owner; installed a fresh G2 app-link writer; and
received its real `Subscribed` reply. It then called the production Core
`signal_ready()` under the already-armed recovery state and **read back the
G2 lifecycle Ready Event, channel 3, corr 0, body `[0]`** before dequeuing
the held G1 OS intent. That event still produced a G2 lifecycle
`QuitRequested` Event, channel 3, corr 0, body `[2]`. The old request is
unbound to its source generation.

The test asserts the Ready frame before advancing to the negative oracle;
a failure to publish or consume G2 Ready would fail earlier and could not
produce the recorded red line. A second permanent Core socket positive
control, `fresh_successor_native_quit_follows_its_actual_ready_frame`,
passed with a fresh G2-originated Quit and the same G2 Ready/Subscribe
sequence. These are real Core socket/typed-frame observations. They do not
execute a Bun link, its separate compat-subscription/replay barrier, JS
`before-quit`, the GH531 close veto, an AppKit AppleEvent, or an installed app.
Thus the supported claim is **misdelivery to a ready G2 Core link**, not a
confirmed unintended JS `before-quit` or dirty-window close.

- Tested branch ancestry: `ddd8be575d57e7a248c36c413efc8214c1000c32`,
  parent `9d0d4666fb89c2cda3436c80cfc67d18dc10a0d7`.
- Corrected temporary complete Rust source SHA256 (including the prior three
  passing tests and the new fresh-G2 positive):
  `727f0acf123f4575649d76360e028b2619315a20b894024e248d5284712807c1`.
- Raw combined stdout/stderr:
  `GH451-buffered-G2-ready-RED.log`, SHA256
  `07635d04366df7be5c099ed8b3bcd9fac58d961af1b414a276eb010a095cbf0d`.
- Exact invocation with shell pipeline error preserved:
  `set -o pipefail; cargo test -p keld-core --lib buffered_g1_intent_must_not_reach_fully_ready_g2_subscriber -- --nocapture 2>&1 | tee crates/keld-core/src/app_session/menu_router/tests/GH451-buffered-G2-ready-RED.log`.
- **Actual result:** exit 101, `1 FAILED; 0 passed; 427 filtered`;
  panic at `native_os_quit.rs:67:36`, with G2 frame
  `(FrameHeader { kind: Event, flags: 0, channel: ChannelId(3), corr: CorrelationId(0), len: 1 }, [2])`.
- Fresh-G2 positive: `cargo test -p keld-core --lib fresh_successor_native_quit_follows_its_actual_ready_frame -- --nocapture`:
  exit 0, `1 passed; 0 failed`.

### Corrected temporary test source (deliberately unregistered after capture)

```rust
#[test]
fn buffered_g1_intent_must_not_reach_fully_ready_g2_subscriber() {
    let (mut router, mut first_peer, old_guardian, _ui) =
        app::tests::initial_ready_router_with_window_loop();
    first_peer
        .set_app_link_deadlines(Some(Duration::from_secs(3)))
        .expect("G1 peer deadline");
    let _source = install_original(&mut router);
    subscribe(&router, &mut first_peer);
    ready(&router, &mut first_peer, old_guardian);
    let first_owner = caller(&router);

    let (native_tx, native_rx) = mpsc::channel();
    native_tx
        .send(app::AppWindowEvent::NativeQuitRequested)
        .expect("G1 OS intent queued before G1 retirement");
    router
        .retire_generation(first_owner.attempt)
        .expect("G1 retirement closes current role and subscription");
    let (replacement, mut next_peer, _new_guardian, _next_ui) =
        app::tests::initial_ready_router_with_window_loop();
    next_peer
        .set_app_link_deadlines(Some(Duration::from_millis(100)))
        .expect("G2 peer deadline");
    let mut successor = replacement
        .current
        .lock()
        .expect("new generation")
        .take()
        .expect("fresh G2 role");
    successor.attempt = first_owner.attempt + 1;
    *router.current.lock().expect("router current") = Some(successor);
    subscribe(&router, &mut next_peer);

    // This is the real Core publisher with the recovery gate already armed.
    // G2's Ready event is consumed BEFORE releasing old queued OS intent.
    router.signal_ready().expect("G2 Ready publisher");
    let (ready_header, ready_bytes) = read_frame(&mut next_peer).expect("G2 Ready on new socket");
    assert_eq!(
        (ready_header.kind, ready_header.channel, ready_header.corr),
        (FrameKind::Event, LIFECYCLE_CHANNEL, CorrelationId(0))
    );
    assert_eq!(ready_bytes, [0], "G2 Ready must precede old Quit");

    let held = native_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("release queued G1 OS intent after G2 Ready");
    assert!(matches!(held, app::AppWindowEvent::NativeQuitRequested));
    router
        .native_os_quit_requested()
        .expect("replay queued G1 OS intent through actual Core route");
    no_quit_frame(&mut next_peer);
}
```

The temporary red test was removed from the registered test module before
commit so the green suite is not misrepresented as a fix. The unresolved
GH-451 choice is how to bind an OS Quit request to its originating
generation through AppKit, WV, and Core. Fresh Quit must still reach a
current G2 owner; prior G1 intent must be rejected.
