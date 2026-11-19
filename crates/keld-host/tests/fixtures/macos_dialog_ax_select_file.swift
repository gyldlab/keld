// Exact-Host-PID macOS native Open selection acceptance. Nonshipping helper.
// Select the named OS file row through Accessibility, never a guessed mouse
// coordinate that may point at a directory or another app's window.
import AppKit
import ApplicationServices
import Foundation

guard (CommandLine.arguments.count == 3 ||
       (CommandLine.arguments.count == 4 &&
        CommandLine.arguments[3] == "--allow-native-denial")),
      let pid = Int32(CommandLine.arguments[1]), pid > 0 else {
    fputs("INVALID_PID_OR_PATH\n", stderr)
    exit(2)
}
guard AXIsProcessTrusted() else {
    fputs("ACCESSIBILITY_NOT_TRUSTED\n", stderr)
    exit(3)
}
let filePath = CommandLine.arguments[2]
let allowNativeDenial = CommandLine.arguments.count == 4
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

// Apple scopes this timeout to every AX object in this observer process.
// Each phase keeps its original six-second budget, including nested traversals.
guard AXUIElementSetMessagingTimeout(AXUIElementCreateSystemWide(), 1) == .success else {
    fputs("NATIVE_AX_TIMEOUT_CONFIGURATION_FAILED\n", stderr); exit(18)
}
var observationPhase = "setup"
var observationDeadline = ProcessInfo.processInfo.systemUptime + 6
func requireObservationTime() {
    guard ProcessInfo.processInfo.systemUptime < observationDeadline else {
        fputs("NATIVE_AX_OBSERVATION_DEADLINE phase=\(observationPhase)\n", stderr); exit(19)
    }
}
func attr(_ element: AXUIElement, _ key: String) -> CFTypeRef? {
    requireObservationTime()
    var result: CFTypeRef?
    let status = AXUIElementCopyAttributeValue(element, key as CFString, &result)
    requireObservationTime()
    guard status != .cannotComplete else {
        fputs("NATIVE_AX_MESSAGE_INCOMPLETE phase=\(observationPhase)\n", stderr); exit(20)
    }
    return status == .success ? result : nil
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
requireObservationTime()
let frontmostRequest = AXUIElementSetAttributeValue(
    process, kAXFrontmostAttribute as CFString, kCFBooleanTrue)
requireObservationTime()
fputs("GH478_TARGET_AX_FRONTMOST_REQUEST=\(frontmostRequest.rawValue)\n", stderr)
if NSWorkspace.shared.frontmostApplication?.processIdentifier != pid_t(pid) {
    // Focus can lag; file discovery and OS action stay PID-scoped regardless.
    fputs("GH478_TARGET_NOT_YET_FRONTMOST\n", stderr)
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
observationPhase = "file-row"
observationDeadline = ProcessInfo.processInfo.systemUptime + 6
var matches: [(AXUIElement, AXUIElement, Int)] = []
var witnessedExactFolder = false
var witnessedNativeBrowser = false
var witnessedOpenButton = false
var lastOpenEnabled = false
while ProcessInfo.processInfo.systemUptime < observationDeadline {
    matches.removeAll()
    for window in attr(process, kAXWindowsAttribute as String) as? [AXUIElement] ?? [] {
        guard currentFolder(window) == expectedFolder else { continue }
        witnessedExactFolder = true
        if let button = findButton(window, "Open") {
            witnessedOpenButton = true
            lastOpenEnabled = attr(button, kAXEnabledAttribute as String) as? Bool == true
        }
        guard let nativeBrowser = browser(window) else { continue }
        witnessedNativeBrowser = true
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
    // A native picker that hides a wrong-extension row already enforces the
    // declared restriction. Accept this ONLY for the explicit negative oracle
    // after independently observing the correct folder and real browser.
    if allowNativeDenial && matches.isEmpty && witnessedExactFolder &&
        witnessedNativeBrowser && witnessedOpenButton && !lastOpenEnabled {
        print("NATIVE_OS_DENIED_FILE_ROW pid=\(pid) basename=\(fileName)")
        exit(0)
    }
    fputs("EXACT_NATIVE_FILE_ROW_COUNT=\(matches.count) EXPECTED=1 FOLDER_MATCH=\(witnessedExactFolder) BROWSER_MATCH=\(witnessedNativeBrowser) OPEN_PRESENT=\(witnessedOpenButton) OPEN_ENABLED=\(lastOpenEnabled)\n", stderr)
    exit(14)
}
let (fileList, fileRow, column) = matches[0]
if allowNativeDenial {
    let rowEnabled = attr(fileRow, kAXEnabledAttribute as String) as? Bool
    let cellEnabled = (attr(fileRow, kAXChildrenAttribute as String) as? [AXUIElement] ?? [])
        .compactMap { attr($0, kAXEnabledAttribute as String) as? Bool }
    fputs("NATIVE_DENIAL_ROW_ENABLED=\(String(describing: rowEnabled)) CELLS=\(cellEnabled)\n", stderr)
    if rowEnabled == false {
        print("NATIVE_OS_DENIED_FILE_ROW pid=\(pid) basename=\(fileName)")
        exit(0)
    }
}
var selectable = DarwinBoolean(false)
requireObservationTime()
let settableStatus = AXUIElementIsAttributeSettable(fileList, kAXSelectedChildrenAttribute as CFString, &selectable)
requireObservationTime()
guard settableStatus == .success, selectable.boolValue else {
    if allowNativeDenial && settableStatus == .success && !selectable.boolValue &&
        usableOpen() == nil {
        print("NATIVE_OS_DENIED_FILE_ROW pid=\(pid) basename=\(fileName)")
        exit(0)
    }
    fputs("NATIVE_FILE_ROW_NOT_ACCESSIBILITY_SELECTABLE\n", stderr)
    exit(15)
}
// This acts on the genuine system file-picker row. The AX setter may report
// a transient status while selection takes effect; only a newly enabled OS
// Open button AND the Host's exact returned path count as success.
requireObservationTime()
let rowAction = AXUIElementSetAttributeValue(
    fileList, kAXSelectedChildrenAttribute as CFString, NSArray(array: [fileRow]))
requireObservationTime()
fputs("GH478_EXACT_FILE_ROW_SELECTION column=\(column) ax_status=\(rowAction.rawValue)\n", stderr)
observationPhase = "open-enable"
observationDeadline = ProcessInfo.processInfo.systemUptime + 6
var selected: AXUIElement?
while ProcessInfo.processInfo.systemUptime < observationDeadline {
    selected = usableOpen()
    if selected != nil { break }
    Thread.sleep(forTimeInterval: 0.05)
}
guard let button = selected else {
    if allowNativeDenial {
        print("NATIVE_OS_DENIED_FILE_ROW pid=\(pid) basename=\(fileName)")
        exit(0)
    }
    fputs("NATIVE_OPEN_NOT_ENABLED_AFTER_EXACT_FILE_ROW_SELECTION\n", stderr)
    exit(12)
}
requireObservationTime()
let pressStatus = AXUIElementPerformAction(button, kAXPressAction as CFString)
requireObservationTime()
guard pressStatus == .success else {
    fputs("ACTUAL_OPEN_BUTTON_PRESS_FAILED\n", stderr)
    exit(13)
}
print("EXACT_HOST_NATIVE_FILE_SELECTED pid=\(pid) basename=\((filePath as NSString).lastPathComponent)")
