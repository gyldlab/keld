import AppKit
import CoreGraphics
import Darwin
import Foundation

// External diagnostic only. Never supplies the test's pass condition.
func monotonicNow() -> Double {
    var t = timespec()
    clock_gettime(CLOCK_MONOTONIC, &t)
    return Double(t.tv_sec) + Double(t.tv_nsec) / 1e9
}
let environment: [String: Any] = [
    "kind": "environment", "mono_s": monotonicNow(),
    "observer_pid": ProcessInfo.processInfo.processIdentifier,
    "screens": NSScreen.screens.count, "main_screen": NSScreen.main != nil,
    "capture_preflight": CGPreflightScreenCaptureAccess(),
    "session": CGSessionCopyCurrentDictionary() as Any? ?? NSNull()
]
func emit(_ record: [String: Any]) throws {
    let data = try JSONSerialization.data(withJSONObject: record, options: [.sortedKeys])
    print(String(decoding: data, as: UTF8.self))
    fflush(stdout)
}
try emit(environment)
for _ in 0..<3600 {
    let start = monotonicNow()
    let all = CGWindowListCopyWindowInfo(.optionAll, kCGNullWindowID) as? [[String: Any]]
    let screen = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]]
    let owned: ([String: Any]) -> Bool = { ($0[kCGWindowOwnerName as String] as? String) == "keld-host" }
    try emit([
        "kind": "census", "mono_s": start, "end_mono_s": monotonicNow(),
        "all_null": all == nil, "screen_null": screen == nil,
        "all": (all ?? []).filter(owned), "screen": (screen ?? []).filter(owned)
    ])
    // Sampling interval; does not synchronize or retry any product operation.
    usleep(100_000)
}
