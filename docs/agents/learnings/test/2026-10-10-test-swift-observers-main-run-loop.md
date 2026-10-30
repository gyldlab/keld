---
name: "2026-10-10-test-swift-observers-main-run-loop"
description: "Swift workspace observers need the main run loop; drain zero-timeout passes before checking state."
area: "test"
tags: ["test", "swift", "observers", "main", "run", "macos"]
platform: "macos"
recorded: "2026-10-10"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L176"
related: ["docs/agents/learnings/host/2026-10-10-host-macos-gui-clicks-target-active.md"]
---

## Historical source

> - 2026-10-10 [test/macos] In a Swift CLI helper, an `NSWorkspace` observer block runs only while the main run loop runs, and `RunLoop.main.run(mode:before:)` did not return after a `queue: nil` block ran. Deliver on `.main`, drain with zero-timeout `CFRunLoopRunInMode` passes before every check, and stop the loop from the observer, or the activation gate goes stale or the wait sleeps to its deadline. (evidence: GH-652 traces and stale-gate hook, 5/5 failures before the drain)
