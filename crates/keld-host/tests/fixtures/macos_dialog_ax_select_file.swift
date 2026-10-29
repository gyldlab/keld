// Exact-Host-PID macOS native Open selection acceptance. Nonshipping helper.
// Select the named OS file row through Accessibility, never a guessed mouse
// coordinate that may point at a directory or another app's window.
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
// This helper may select only the single file in the test-created directory,
// and must not act on any unrelated file row.
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
// A CLI-launched Host can remain in the background during tests. Request
// foreground status for this exact Host, but send no global HID input. Every
// action below uses its own AX tree and must verify the native outcome.
let frontmostRequest = AXUIElementSetAttributeValue(
    process, kAXFrontmostAttribute as CFString, kCFBooleanTrue)
fputs("GH478_TARGET_AX_FRONTMOST_REQUEST=\(frontmostRequest.rawValue)\n", stderr)
if NSWorkspace.shared.frontmostApplication?.processIdentifier != pid_t(pid) {
    // Focus can lag; file discovery and OS action stay PID-scoped regardless.
    fputs("GH478_TARGET_NOT_YET_FRONTMOST\n", stderr)
}
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

// NSOpenPanel's system-owned column browser can expose the file in AXColumns
// before that column is inside the visible viewport. AXVisibleColumns and a
// fixed first-row pixel are not a selection oracle. Select only the exact
// file owned by this test, then require a real enabled Open button.
func browser(_ node: AXUIElement, _ depth: Int = 0) -> AXUIElement? {
    if depth > 15 { return nil }
    if attr(node, kAXRoleAttribute as String) as? String == "AXBrowser" { return node }
    for child in attr(node, kAXChildrenAttribute as String) as? [AXUIElement] ?? [] {
        if let found = browser(child, depth + 1) { return found }
    }
    return nil
}
func currentFolder(_ node: AXUIElement, _ depth: Int = 0) -> String? {
    if depth > 15 { return nil }
    if attr(node, kAXRoleAttribute as String) as? String == "AXPopUpButton",
       attr(node, kAXTitleAttribute as String) as? String == "Where:" {
        return attr(node, kAXValueAttribute as String) as? String
    }
    for child in attr(node, kAXChildrenAttribute as String) as? [AXUIElement] ?? [] {
        if let folder = currentFolder(child, depth + 1) { return folder }
    }
    return nil
}
let expectedFolder = (fileParent as NSString).lastPathComponent
let fileDeadline = Date().addingTimeInterval(6)
var matches: [(AXUIElement, AXUIElement, Int)] = []
while Date() < fileDeadline {
    matches.removeAll()
    for window in attr(process, kAXWindowsAttribute as String) as? [AXUIElement] ?? [] {
        guard currentFolder(window) == expectedFolder, let nativeBrowser = browser(window) else {
            continue
        }
        for (index, column) in (attr(nativeBrowser, "AXColumns") as? [AXUIElement] ?? []).enumerated() {
            for list in attr(column, "AXContents") as? [AXUIElement] ?? [] {
                guard attr(list, kAXRoleAttribute as String) as? String == "AXList" else { continue }
                for row in attr(list, kAXChildrenAttribute as String) as? [AXUIElement] ?? [] {
                    let cells = attr(row, kAXChildrenAttribute as String) as? [AXUIElement] ?? []
                    if cells.contains(where: {
                        attr($0, kAXRoleAttribute as String) as? String == "AXTextField"
                            && attr($0, kAXValueAttribute as String) as? String == fileName
                    }) {
                        matches.append((list, row, index))
                    }
                }
            }
        }
    }
    if !matches.isEmpty { break }
    Thread.sleep(forTimeInterval: 0.05)
}
guard matches.count == 1 else {
    fputs("EXACT_NATIVE_FILE_ROW_COUNT=\(matches.count) EXPECTED=1\n", stderr)
    exit(14)
}
let (fileList, fileRow, column) = matches[0]
var selectable = DarwinBoolean(false)
guard AXUIElementIsAttributeSettable(fileList, kAXSelectedChildrenAttribute as CFString, &selectable) == .success,
      selectable.boolValue else {
    fputs("NATIVE_FILE_ROW_NOT_ACCESSIBILITY_SELECTABLE\n", stderr)
    exit(15)
}
// This acts on the genuine system file-picker row. The AX setter may report
// a transient status while selection takes effect; only a newly enabled OS
// Open button AND the Host's exact returned path count as success.
let rowAction = AXUIElementSetAttributeValue(
    fileList, kAXSelectedChildrenAttribute as CFString, NSArray(array: [fileRow]))
fputs("GH478_EXACT_FILE_ROW_SELECTION column=\(column) ax_status=\(rowAction.rawValue)\n", stderr)
let selectionDeadline = Date().addingTimeInterval(6)
var selected: AXUIElement?
while Date() < selectionDeadline {
    selected = usableOpen()
    if selected != nil { break }
    Thread.sleep(forTimeInterval: 0.05)
}
guard let button = selected else {
    fputs("NATIVE_OPEN_NOT_ENABLED_AFTER_EXACT_FILE_ROW_SELECTION\n", stderr)
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
