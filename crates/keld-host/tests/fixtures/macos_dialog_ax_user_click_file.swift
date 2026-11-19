// GH478: physical native Open-row oracle for an exclusive macOS UI test lane.
// A programmatic AXSelectedChildren setter can select a disabled file, so this
// observer instead posts an actual mouse down/up at a precisely verified row.
// Any missing/unsupported AX fact, click uncertainty, or timeout is FAILURE,
// never evidence that the native panel disabled the requested file.
import AppKit
import ApplicationServices
import CoreGraphics
import Foundation

func fail(_ code: Int32, _ detail: String) -> Never {
    fputs("GH478_USER_CLICK_INCONCLUSIVE code=\(code) \(detail)\n", stderr)
    exit(code)
}

guard CommandLine.arguments.count == 4,
      let pid = Int32(CommandLine.arguments[1]), pid > 0,
      ["allowed", "denied"].contains(CommandLine.arguments[3]) else {
    fail(2, "invalid arguments")
}
guard ProcessInfo.processInfo.environment["KELD_GH478_EXCLUSIVE_GUI"] == "1" else {
    fail(3, "exclusive native GUI lane not explicitly enabled")
}
guard AXIsProcessTrusted() else {
    fail(4, "macOS Accessibility permission is absent")
}

let path = CommandLine.arguments[2]
let filename = (path as NSString).lastPathComponent
let directory = (path as NSString).deletingLastPathComponent
let folderName = (directory as NSString).lastPathComponent
let expectAllowed = CommandLine.arguments[3] == "allowed"
guard path.hasPrefix("/"), ["gh478-widened.svg", "gh478-format.vsdx"].contains(filename),
      folderName.hasPrefix("keld-gh478-visible-open-"),
      let files = try? FileManager.default.contentsOfDirectory(atPath: directory),
      files == [filename], FileManager.default.fileExists(atPath: path),
      NSRunningApplication(processIdentifier: pid_t(pid)) != nil else {
    fail(5, "fixture is not exactly one owned SVG in a unique directory")
}

let app = AXUIElementCreateApplication(pid_t(pid))
let system = AXUIElementCreateSystemWide()
guard AXUIElementSetMessagingTimeout(system, 1) == .success else {
    fail(6, "cannot configure bounded Accessibility calls")
}
let discoveryDeadline = ProcessInfo.processInfo.systemUptime + 8

func attribute(_ node: AXUIElement, _ key: String) -> (AXError, CFTypeRef?) {
    var value: CFTypeRef?
    let result = AXUIElementCopyAttributeValue(node, key as CFString, &value)
    if result == .cannotComplete {
        fail(7, "Accessibility cannot complete \(key)")
    }
    return (result, result == .success ? value : nil)
}

func string(_ node: AXUIElement, _ key: String) -> String? {
    let (error, value) = attribute(node, key)
    guard error == .success else { return nil }
    return value as? String
}

func children(_ node: AXUIElement) -> [AXUIElement] {
    let (error, value) = attribute(node, kAXChildrenAttribute as String)
    guard error == .success else { return [] }
    return value as? [AXUIElement] ?? []
}

func descendants(_ node: AXUIElement, _ depth: Int = 0) -> [AXUIElement] {
    if depth > 15 { return [] }
    return [node] + children(node).flatMap { descendants($0, depth + 1) }
}

func hasRole(_ node: AXUIElement, _ role: String) -> Bool {
    string(node, kAXRoleAttribute as String) == role
}

func enabled(_ node: AXUIElement) -> Bool {
    let (error, value) = attribute(node, kAXEnabledAttribute as String)
    guard error == .success, let value = value as? Bool else {
        fail(8, "Open button AXEnabled is unsupported/unknown: \(error.rawValue)")
    }
    return value
}

struct Target {
    let window: AXUIElement
    let row: AXUIElement
    let field: AXUIElement
    let button: AXUIElement
}

func targets() -> [Target] {
    let (error, all) = attribute(app, kAXWindowsAttribute as String)
    guard error == .success, let windows = all as? [AXUIElement] else { return [] }
    var result: [Target] = []
    for window in windows {
        let nodes = descendants(window)
        let locations = nodes.filter {
            hasRole($0, "AXPopUpButton") && string($0, kAXTitleAttribute as String) == "Where:"
        }
        guard locations.count == 1,
              string(locations[0], kAXValueAttribute as String) == folderName else { continue }
        let opens = nodes.filter {
            hasRole($0, "AXButton") && string($0, kAXTitleAttribute as String) == "Open"
        }
        guard opens.count == 1 else { continue }
        let browsers = nodes.filter { hasRole($0, "AXBrowser") }
        for browser in browsers {
            let (_, value) = attribute(browser, "AXColumns")
            for column in value as? [AXUIElement] ?? [] {
                let (_, contents) = attribute(column, "AXContents")
                for list in contents as? [AXUIElement] ?? [] where hasRole(list, "AXList") {
                    for row in children(list) {
                        let names = children(row).filter {
                            hasRole($0, "AXTextField") && string($0, kAXValueAttribute as String) == filename
                        }
                        for field in names {
                            result.append(Target(window: window, row: row, field: field, button: opens[0]))
                        }
                    }
                }
            }
        }
    }
    return result
}

let frontmostStatus = AXUIElementSetAttributeValue(app, kAXFrontmostAttribute as CFString, kCFBooleanTrue)
guard frontmostStatus == .success else {
    fail(9, "cannot activate exact Host PID: \(frontmostStatus.rawValue)")
}
var found: [Target] = []
while ProcessInfo.processInfo.systemUptime < discoveryDeadline {
    found = targets()
    if found.count == 1, NSWorkspace.shared.frontmostApplication?.processIdentifier == pid_t(pid) {
        break
    }
    if found.count > 1 { fail(10, "ambiguous Host-owned file rows: \(found.count)") }
    _ = sched_yield()
}
guard ProcessInfo.processInfo.systemUptime < discoveryDeadline, found.count == 1,
      NSWorkspace.shared.frontmostApplication?.processIdentifier == pid_t(pid) else {
    fail(11, "exact row or frontmost Host did not become observable before deadline")
}
let target = found[0]
var targetPid: pid_t = 0
guard AXUIElementGetPid(target.row, &targetPid) == .success, targetPid == pid_t(pid) else {
    fail(12, "AX file row belongs to another process")
}
// The native column browser can expose offscreen rows in AXColumns. A failed
// scroll action is not evidence of denial; the exact screen hit below remains
// mandatory before any global mouse event is posted.
let scrollStatus = AXUIElementPerformAction(target.row, "AXScrollToVisible" as CFString)
fputs("GH478_USER_CLICK_SCROLL status=\(scrollStatus.rawValue)\n", stderr)

func rect(_ element: AXUIElement) -> CGRect? {
    let (positionError, positionValue) = attribute(element, kAXPositionAttribute as String)
    let (sizeError, sizeValue) = attribute(element, kAXSizeAttribute as String)
    guard positionError == .success, sizeError == .success,
          let positionValue, let sizeValue,
          CFGetTypeID(positionValue) == AXValueGetTypeID(),
          CFGetTypeID(sizeValue) == AXValueGetTypeID() else { return nil }
    let positionAX = positionValue as! AXValue
    let sizeAX = sizeValue as! AXValue
    guard AXValueGetType(positionAX) == .cgPoint, AXValueGetType(sizeAX) == .cgSize else { return nil }
    var position = CGPoint.zero
    var size = CGSize.zero
    guard AXValueGetValue(positionAX, .cgPoint, &position),
          AXValueGetValue(sizeAX, .cgSize, &size),
          position.x.isFinite, position.y.isFinite, size.width.isFinite, size.height.isFinite,
          size.width > 2, size.height > 2 else { return nil }
    return CGRect(origin: position, size: size)
}

// In macOS column view the selected parent directory can be visible while the
// file row's next column exists only in AXColumns offscreen to the right. Move
// the native horizontal browser scrollbar, never the file selection, before
// requiring an independently verified physical hit on the exact file row.
let horizontal = descendants(target.window).filter {
    hasRole($0, "AXScrollBar") && string($0, "AXOrientation") == "AXHorizontalOrientation"
}
fputs("GH478_USER_CLICK_HORIZONTAL_BARS count=\(horizontal.count)\n", stderr)
for bar in horizontal {
    let status = AXUIElementSetAttributeValue(bar, kAXValueAttribute as CFString, NSNumber(value: 1.0))
    fputs("GH478_USER_CLICK_HORIZONTAL_SCROLL status=\(status.rawValue) position=\(String(describing: rect(bar)))\n", stderr)
}
guard let fieldRect = rect(target.field) ?? rect(target.row) else {
    fail(13, "native file row has no verified on-screen rectangle")
}
let point = CGPoint(x: fieldRect.midX, y: fieldRect.midY)

func hitIsExactRow(_ point: CGPoint) -> Bool {
    var hit: AXUIElement?
    guard AXUIElementCopyElementAtPosition(system, Float(point.x), Float(point.y), &hit) == .success,
          var cursor = hit else { return false }
    for _ in 0..<10 {
        if CFEqual(cursor, target.row) || CFEqual(cursor, target.field) { return true }
        let (error, parent) = attribute(cursor, kAXParentAttribute as String)
        guard error == .success, let parent,
              CFGetTypeID(parent) == AXUIElementGetTypeID() else { return false }
        cursor = parent as! AXUIElement
    }
    return false
}

guard hitIsExactRow(point) else {
    var hit: AXUIElement?
    let hitStatus = AXUIElementCopyElementAtPosition(system, Float(point.x), Float(point.y), &hit)
    let hitRole = hit.flatMap { string($0, kAXRoleAttribute as String) } ?? "unknown"
    fail(14, "screen hit test did not resolve to the exact visible file row; point=\(point), hitStatus=\(hitStatus.rawValue), hitRole=\(hitRole)")
}
let before = enabled(target.button)
if before {
    fail(15, "Open was enabled before physical click; an initial selection would invalidate the input oracle")
}
guard NSWorkspace.shared.frontmostApplication?.processIdentifier == pid_t(pid), hitIsExactRow(point) else {
    fail(16, "Host lost foreground or row changed before physical click")
}
guard CGPreflightPostEventAccess() else {
    fail(22, "CoreGraphics physical event posting is not authorized")
}
fputs("GH478_USER_CLICK_BEFORE mode=\(CommandLine.arguments[3]) pid=\(pid) native_open=false exact_row_hit=true\n", stderr)

// The actual HID mouse path is intentional and must run only under the
// explicitly opted-in exclusive macOS GUI lane. AXSelectedChildren is NEVER
// set; action delivery is independently calibrated by the All Files control.
guard let down = CGEvent(mouseEventSource: nil, mouseType: .leftMouseDown,
                         mouseCursorPosition: point, mouseButton: .left),
      let up = CGEvent(mouseEventSource: nil, mouseType: .leftMouseUp,
                       mouseCursorPosition: point, mouseButton: .left) else {
    fail(17, "could not construct real mouse down/up events")
}
down.post(tap: .cghidEventTap)
up.post(tap: .cghidEventTap)
fputs("GH478_USER_CLICK_POSTED mode=\(CommandLine.arguments[3]) native_mouse_down_up=true\n", stderr)

let completionDeadline = ProcessInfo.processInfo.systemUptime + 3
if expectAllowed {
    while ProcessInfo.processInfo.systemUptime < completionDeadline {
        if enabled(target.button) { break }
        _ = sched_yield()
    }
    guard enabled(target.button) else {
        fail(18, "All Files click failed to enable Open; physical delivery unproven")
    }
    guard AXUIElementPerformAction(target.button, kAXPressAction as CFString) == .success else {
        fail(19, "could not physically finish All Files Open")
    }
    fputs("GH478_USER_CLICK_AFTER mode=allowed native_open=true native_open_pressed=true\n", stderr)
    print("GH478_USER_CLICK_ALLOWED pid=\(pid) basename=\(filename)")
} else {
    // Denial is an OS outcome only after a verified, physically targeted
    // down/up; a missing AX button value or an event timeout is not denial.
    while ProcessInfo.processInfo.systemUptime < completionDeadline {
        if enabled(target.button) {
            fail(20, "SVG physical click ENABLED Open under XML-only filter")
        }
        _ = sched_yield()
    }
    guard NSWorkspace.shared.frontmostApplication?.processIdentifier == pid_t(pid) else {
        fail(21, "Host lost foreground during native denial observation")
    }
    fputs("GH478_USER_CLICK_AFTER mode=denied native_open=false native_open_pressed=false\n", stderr)
    print("GH478_USER_CLICK_DENIED pid=\(pid) basename=\(filename) open_enabled=false")
}
