# Windows test controls
Owner/load: webview-windows-test-controls/always. Extends ../AGENTS.md.
Debug + profile-test-hooks only. Native/design review and instruction gates before compile.

- frame_ingress{,/native}.rs: creating-STA FrameCreated, Frame2 WebMessageReceived and Destroyed subscriptions; actual Frame/IsDestroyed, canonical IUnknown QI, bounded Source/WebMessageAsJson getters only. No injection, forwarding, CSP/grant/setting change or Close/removal.
- Attach observer/frame/tokens before QI/Add; reuse Registration and SAME View/Close owners. Retain through partial Add, error and unwind. Drain needs actual Close, no Add in-call, and sole observer Rc after native inputs drop.
- Fixed metadata stays in OriginalView's existing account/assertion. Existing callback credit covers native text and borrowed trace serialization until disposal. Complete String observation never grants dispatch.
- Reuse OriginalWake only after actual Close leaves callback custody to drain; no new queue, timer or pump.
