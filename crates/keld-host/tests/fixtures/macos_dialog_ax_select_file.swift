// Exact-Host-PID macOS native Open selection acceptance. Nonshipping helper.
// Deliberately requires accessibility trust and posts keyboard events only to
// the exact test Host PID, never globally to the user's foreground app.
import AppKit
import ApplicationServices
import Foundation

guard CommandLine.arguments.count == 3,
      let pid = Int32(CommandLine.arguments[1]), pid > 0 else {
    fputs("INVALID_PID_OR_PATH\n", stderr)
    exit(2)
}
guard AXIsProcessTrusted() else {
    fputs("ACCESSIBILITY_NOT_TRUSTED\n", stderr)
    exit(3)
}
let filePath = CommandLine.arguments[2]
guard filePath.hasPrefix("/"), FileManager.default.fileExists(atPath: filePath) else {
    fputs("NO_EXISTING_FIXTURE_FILE\n", stderr)
    exit(4)
}
// This helper is allowed to select only a single file in the test-created
// directory. It must never click a row of a directory containing user files.
let fileName = (filePath as NSString).lastPathComponent
let fileParent = (filePath as NSString).deletingLastPathComponent
guard let entries = try? FileManager.default.contentsOfDirectory(atPath: fileParent),
      entries == [fileName] else {
    fputs("NOT_A_SINGLE_FILE_TEST_FIXTURE\n", stderr)
    exit(17)
}
guard let application = NSRunningApplication(processIdentifier: pid_t(pid)) else {
    fputs("HOST_PID_NOT_RUNNING\n", stderr)
    exit(5)
}
fputs("GH478_TARGET_ACTIVATION_POLICY=\(application.activationPolicy.rawValue) FRONTMOST_PID=\(NSWorkspace.shared.frontmostApplication?.processIdentifier ?? -1)\n", stderr)

func press(_ key: CGKeyCode, flags: CGEventFlags = []) {
    for down in [true, false] {
        guard let event = CGEvent(keyboardEventSource: nil, virtualKey: key, keyDown: down) else {
            fputs("CG_EVENT_FAILED\n", stderr)
            exit(9)
        }
        event.flags = flags
        event.postToPid(pid_t(pid))
        Thread.sleep(forTimeInterval: 0.02)
    }
}
func typeText(_ value: String) {
    var chars = Array(value.utf16)
    chars.withUnsafeMutableBufferPointer { buffer in
        for down in [true, false] {
            guard let event = CGEvent(keyboardEventSource: nil, virtualKey: 0, keyDown: down) else {
                fputs("CG_EVENT_UNICODE_FAILED\n", stderr)
                exit(11)
            }
            event.keyboardSetUnicodeString(stringLength: buffer.count, unicodeString: buffer.baseAddress!)
            event.postToPid(pid_t(pid))
            Thread.sleep(forTimeInterval: 0.03)
        }
    }
}
func attr(_ element: AXUIElement, _ key: String) -> CFTypeRef? {
    var result: CFTypeRef?
    return AXUIElementCopyAttributeValue(element, key as CFString, &result) == .success ? result : nil
}
func findButton(_ element: AXUIElement, _ label: String, _ depth: Int = 0) -> AXUIElement? {
    if depth > 15 { return nil }
    let role = attr(element, kAXRoleAttribute as String) as? String
    let title = attr(element, kAXTitleAttribute as String) as? String
    if role == kAXButtonRole as String && title == label { return element }
    for child in attr(element, kAXChildrenAttribute as String) as? [AXUIElement] ?? [] {
        if let found = findButton(child, label, depth + 1) { return found }
    }
    return nil
}
let process = AXUIElementCreateApplication(pid_t(pid))
// A CLI-launched Host owns a real window, but macOS may leave its native open
// sheet unfocused while tests run from Terminal. Request focus ONLY for this
// exact authenticated Host before sending per-PID input. Never target another
// process or claim an OS click unless the native button becomes enabled.
let frontmostRequest = AXUIElementSetAttributeValue(
    process, kAXFrontmostAttribute as CFString, kCFBooleanTrue)
fputs("GH478_TARGET_AX_FRONTMOST_REQUEST=\(frontmostRequest.rawValue)\n", stderr)
if NSWorkspace.shared.frontmostApplication?.processIdentifier != pid_t(pid) {
    // A frontmost request may be refused by WindowServer; the guarded mouse
    // path below then fails without sending a global click to another app.
    fputs("GH478_TARGET_NOT_YET_FRONTMOST\n", stderr)
}
Thread.sleep(forTimeInterval: 0.25)
func dump(_ node: AXUIElement, _ depth: Int, _ remaining: inout Int) {
    if depth > 10 || remaining == 0 { return }
    remaining -= 1
    let role = attr(node, kAXRoleAttribute as String) as? String ?? "-"
    let title = attr(node, kAXTitleAttribute as String) as? String ?? ""
    let value = attr(node, kAXValueAttribute as String) as? String ?? ""
    let description = attr(node, kAXDescriptionAttribute as String) as? String ?? ""
    fputs("AX_DEPTH=\(depth) ROLE=\(role) TITLE=\(String(title.prefix(60))) VALUE=\(String(value.prefix(110))) DESC=\(String(description.prefix(60)))\n", stderr)
    if role == "AXBrowser" || role == "AXScrollArea" || role == "AXSheet" || role == "AXTextField" {
        var names: CFArray?
        if AXUIElementCopyAttributeNames(node, &names) == .success,
           let names = names as? [String] {
            fputs("AX_ATTRIBUTES role=\(role) keys=\(names.joined(separator: ","))\n", stderr)
        }
        var actions: CFArray?
        if AXUIElementCopyActionNames(node, &actions) == .success,
           let actions = actions as? [String] {
            fputs("AX_ACTIONS role=\(role) actions=\(actions.joined(separator: ","))\n", stderr)
        }
    }
    for child in attr(node, kAXChildrenAttribute as String) as? [AXUIElement] ?? [] {
        dump(child, depth + 1, &remaining)
    }
}
func inspectBrowserColumns(_ node: AXUIElement, _ depth: Int = 0) {
    if depth > 14 { return }
    if attr(node, kAXRoleAttribute as String) as? String == "AXBrowser" {
        for key in ["AXColumns", "AXVisibleColumns"] {
            let columns = attr(node, key) as? [AXUIElement] ?? []
            fputs("GH478_BROWSER \(key) count=\(columns.count)\n", stderr)
            for column in columns {
                var remaining = 110
                dump(column, 0, &remaining)
            }
        }
    }
    for child in attr(node, kAXChildrenAttribute as String) as? [AXUIElement] ?? [] {
        inspectBrowserColumns(child, depth + 1)
    }
}
func usableOpen() -> AXUIElement? {
    for window in attr(process, kAXWindowsAttribute as String) as? [AXUIElement] ?? [] {
        if let button = findButton(window, "Open"),
           attr(button, kAXEnabledAttribute as String) as? Bool == true {
            return button
        }
    }
    return nil
}

// The native panel starts in the fixture's declared directory. Use its REAL
// visible Finder-style column view to click the only fixture file. This is a
// genuine OS mouse action targeted at the exact Host PID, never an injected
// native result or permission-granting code in Keld.
func browser(_ node: AXUIElement, _ depth: Int = 0) -> AXUIElement? {
    if depth > 15 { return nil }
    if attr(node, kAXRoleAttribute as String) as? String == "AXBrowser" { return node }
    for child in attr(node, kAXChildrenAttribute as String) as? [AXUIElement] ?? [] {
        if let found = browser(child, depth + 1) { return found }
    }
    return nil
}
func lastColumnPoint() -> CGPoint? {
    for window in attr(process, kAXWindowsAttribute as String) as? [AXUIElement] ?? [] {
        guard let list = browser(window) else { continue }
        let columns = attr(list, "AXVisibleColumns") as? [AXUIElement] ?? []
        guard let last = columns.last,
              let positionObject = attr(last, kAXPositionAttribute as String),
              let sizeObject = attr(last, kAXSizeAttribute as String) else { continue }
        let position = positionObject as! AXValue
        let size = sizeObject as! AXValue
        var point = CGPoint.zero
        var dimensions = CGSize.zero
        guard AXValueGetValue(position, .cgPoint, &point),
              AXValueGetValue(size, .cgSize, &dimensions),
              dimensions.width > 120, dimensions.height > 80 else { continue }
        fputs("GH478_NATIVE_BROWSER_COLUMN x=\(point.x) y=\(point.y) w=\(dimensions.width) h=\(dimensions.height)\n", stderr)
        // In macOS column view, the first item sits below the column header.
        // This test owns exactly one file in this directory. The *actual*
        // enabled Open button and returned path verify that selection worked.
        return CGPoint(x: point.x + 65, y: point.y + 44)
    }
    return nil
}
let readyDeadline = Date().addingTimeInterval(4)
var location: CGPoint?
while Date() < readyDeadline && location == nil {
    location = lastColumnPoint()
    if location == nil { Thread.sleep(forTimeInterval: 0.05) }
}
guard let location else {
    fputs("NATIVE_FILE_BROWSER_COLUMN_MISSING\n", stderr)
    exit(14)
}
for down in [true, false] {
    guard NSWorkspace.shared.frontmostApplication?.processIdentifier == pid_t(pid) else {
        fputs("HOST_NOT_FRONTMOST_MOUSE_NOT_SENT\n", stderr)
        exit(16)
    }
    guard let event = CGEvent(
        mouseEventSource: nil,
        mouseType: down ? .leftMouseDown : .leftMouseUp,
        mouseCursorPosition: location,
        mouseButton: .left) else {
        fputs("NATIVE_FILE_MOUSE_EVENT_UNAVAILABLE\n", stderr)
        exit(15)
    }
    // On macOS 10.15+, native Open panels render in an AppKit XPC process,
    // not the Host PID. A postToPid(Host) mouse event cannot select its file
    // row. The exact Host must be frontmost and own the observed AXSheet
    // before this one actual HID click is sent to WindowServer.
    event.post(tap: .cghidEventTap)
    Thread.sleep(forTimeInterval: 0.03)
}
let selectionDeadline = Date().addingTimeInterval(6)
var selected: AXUIElement?
while Date() < selectionDeadline {
    selected = usableOpen()
    if selected != nil { break }
    Thread.sleep(forTimeInterval: 0.05)
}
guard let button = selected else {
    fputs("FILE_SELECTION_DID_NOT_ENABLE_OPEN\n", stderr)
    var remaining = 140
    for window in attr(process, kAXWindowsAttribute as String) as? [AXUIElement] ?? [] {
        dump(window, 0, &remaining)
        inspectBrowserColumns(window)
    }
    exit(12)
}
guard AXUIElementPerformAction(button, kAXPressAction as CFString) == .success else {
    fputs("ACTUAL_OPEN_BUTTON_PRESS_FAILED\n", stderr)
    exit(13)
}
print("EXACT_HOST_NATIVE_FILE_SELECTED pid=\(pid) basename=\((filePath as NSString).lastPathComponent)")
