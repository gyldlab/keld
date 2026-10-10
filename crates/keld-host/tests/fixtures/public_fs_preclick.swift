// Read-only prefix in the actual pointer poster process. Never activate or post.
import AppKit
import CoreGraphics
import Foundation

let diagnosticTargetPID = Int(CommandLine.arguments[1])!
let diagnosticTargetTitle = CommandLine.arguments[2]
let diagnosticWorkspace = NSWorkspace.shared
let diagnosticFrontmost = diagnosticWorkspace.frontmostApplication
let diagnosticTargetApplication = NSRunningApplication(processIdentifier: pid_t(diagnosticTargetPID))
let diagnosticRawRows = CGWindowListCopyWindowInfo(
  [.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID
) as? [[String: Any]]
let diagnosticRows = diagnosticRawRows ?? []

func diagnosticFrame(_ row: [String: Any]) -> CGRect? {
  guard let bounds = row[kCGWindowBounds as String] as? NSDictionary else { return nil }
  var rect = CGRect.zero
  guard CGRectMakeWithDictionaryRepresentation(bounds as CFDictionary, &rect) else { return nil }
  return rect
}

func diagnosticDescribe(_ row: [String: Any]) -> [String: Any] {
  [
    "window": row[kCGWindowNumber as String] ?? NSNull(),
    "owner_pid": row[kCGWindowOwnerPID as String] ?? NSNull(),
    "owner": row[kCGWindowOwnerName as String] ?? NSNull(),
    "title": row[kCGWindowName as String] ?? NSNull(),
    "layer": row[kCGWindowLayer as String] ?? NSNull(),
    "onscreen": row[kCGWindowIsOnscreen as String] ?? NSNull(),
    "alpha": row[kCGWindowAlpha as String] ?? NSNull(),
    "bounds": row[kCGWindowBounds as String] ?? NSNull(),
  ]
}

let diagnosticHostRows = diagnosticRows.filter {
  ($0[kCGWindowOwnerPID as String] as? NSNumber)?.intValue == diagnosticTargetPID
}
let diagnosticTargetRow = diagnosticHostRows.first {
  $0[kCGWindowName as String] as? String == diagnosticTargetTitle &&
    ($0[kCGWindowLayer as String] as? NSNumber)?.intValue == 0
}
let diagnosticPoint = diagnosticTargetRow.flatMap(diagnosticFrame).map {
  CGPoint(x: $0.midX, y: $0.midY)
}
// These rectangles are geometric candidates, not proof of actual mouse hit testing.
let diagnosticCoveringRows = diagnosticPoint.map { point in
  diagnosticRows.filter { diagnosticFrame($0)?.contains(point) == true }
} ?? []
let diagnosticPayload: [String: Any] = [
  "poster_pid": ProcessInfo.processInfo.processIdentifier,
  "target_pid": diagnosticTargetPID,
  "target_title": diagnosticTargetTitle,
  "unix_seconds": Date().timeIntervalSince1970,
  "post_event_preflight": CGPreflightPostEventAccess(),
  "listen_event_preflight": CGPreflightListenEventAccess(),
  "screen_capture_preflight": CGPreflightScreenCaptureAccess(),
  "frontmost_pid": diagnosticFrontmost.map { $0.processIdentifier as Any } ?? NSNull(),
  "frontmost_name": diagnosticFrontmost.flatMap { $0.localizedName }.map { $0 as Any } ?? NSNull(),
  "target_active": diagnosticTargetApplication.map { $0.isActive as Any } ?? NSNull(),
  "window_rows_available": diagnosticRawRows != nil,
  "window_rows_total": diagnosticRows.count,
  "host_rows_total": diagnosticHostRows.count,
  "host_rows": Array(diagnosticHostRows.prefix(16)).map(diagnosticDescribe),
  "target_row": diagnosticTargetRow.map { diagnosticDescribe($0) as Any } ?? NSNull(),
  "input_point": diagnosticPoint.map { ["x": $0.x, "y": $0.y] as Any } ?? NSNull(),
  "covering_rows_total": diagnosticCoveringRows.count,
  "front_to_back_geometric_covering_rows": Array(diagnosticCoveringRows.prefix(16)).map(diagnosticDescribe),
  // Session counters include unrelated input and prove no DOM or handler delivery.
  "session_mouse_moved": CGEventSource.counterForEventType(.combinedSessionState, eventType: .mouseMoved),
  "session_mouse_down": CGEventSource.counterForEventType(.combinedSessionState, eventType: .leftMouseDown),
  "session_mouse_up": CGEventSource.counterForEventType(.combinedSessionState, eventType: .leftMouseUp),
]
do {
  let diagnosticData = try JSONSerialization.data(withJSONObject: diagnosticPayload, options: [.sortedKeys])
  print("KELD_KEL140_PRECLICK " + String(data: diagnosticData, encoding: .utf8)!)
} catch {
  print("KELD_KEL140_PRECLICK_ENCODING_ERROR " + String(describing: error))
}
