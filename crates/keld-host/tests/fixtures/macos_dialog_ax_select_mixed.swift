// Independent native OS oracle for Open with both file and directory modes.
// It selects exact owned AX rows, never uses global mouse/keyboard input.
import AppKit
import ApplicationServices
import Foundation

guard CommandLine.arguments.count == 5,
      let pid = Int32(CommandLine.arguments[1]), pid > 0 else {
    fputs("MIXED_INVALID_ARGUMENTS\n", stderr); exit(2)
}
guard AXIsProcessTrusted() else {
    fputs("MIXED_ACCESSIBILITY_NOT_TRUSTED\n", stderr); exit(3)
}
let file = CommandLine.arguments[2]
let directory = CommandLine.arguments[3]
let choice = CommandLine.arguments[4]
let parent = (file as NSString).deletingLastPathComponent
let fileName = (file as NSString).lastPathComponent
let folderName = (directory as NSString).lastPathComponent
guard ["file", "directory", "both"].contains(choice),
      file.hasPrefix("/"), directory.hasPrefix("/"),
      (directory as NSString).deletingLastPathComponent == parent,
      FileManager.default.fileExists(atPath: file),
      (try? FileManager.default.attributesOfItem(atPath: directory)[.type] as? FileAttributeType) == .typeDirectory,
      Set((try? FileManager.default.contentsOfDirectory(atPath: parent)) ?? []) == Set([fileName, folderName]),
      NSRunningApplication(processIdentifier: pid_t(pid)) != nil else {
    fputs("MIXED_NOT_EXACT_OWNED_TWO_ENTRY_FIXTURE\n", stderr); exit(4)
}
let app = AXUIElementCreateApplication(pid_t(pid))
guard AXUIElementSetMessagingTimeout(AXUIElementCreateSystemWide(), 1) == .success else {
    fputs("MIXED_AX_TIMEOUT_SETUP_FAILED\n", stderr); exit(5)
}
let deadline = ProcessInfo.processInfo.systemUptime + 9
func attr(_ element: AXUIElement, _ key: String) -> CFTypeRef? {
    var value: CFTypeRef?
    let result = AXUIElementCopyAttributeValue(element, key as CFString, &value)
    return result == .success ? value : nil
}
func find(_ node: AXUIElement, _ role: String, _ title: String?, _ depth: Int = 0) -> [AXUIElement] {
    if depth > 15 { return [] }
    let own = attr(node, kAXRoleAttribute as String) as? String == role &&
        (title == nil || attr(node, kAXTitleAttribute as String) as? String == title!)
    var result = own ? [node] : []
    for child in attr(node, kAXChildrenAttribute as String) as? [AXUIElement] ?? [] {
        result += find(child, role, title, depth + 1)
    }
    return result
}
func currentFolder(_ window: AXUIElement) -> String? {
    find(window, "AXPopUpButton", "Where:").first.flatMap {
        attr($0, kAXValueAttribute as String) as? String
    }
}
func rows(_ list: AXUIElement) -> [String: AXUIElement] {
    var matched: [String: AXUIElement] = [:]
    for row in attr(list, kAXChildrenAttribute as String) as? [AXUIElement] ?? [] {
        for field in find(row, "AXTextField", nil) {
            if let value = attr(field, kAXValueAttribute as String) as? String,
               [fileName, folderName].contains(value) { matched[value] = row }
        }
    }
    return matched
}
_ = AXUIElementSetAttributeValue(app, kAXFrontmostAttribute as CFString, kCFBooleanTrue)
var exact: (AXUIElement, [String: AXUIElement])?
while ProcessInfo.processInfo.systemUptime < deadline {
    for window in attr(app, kAXWindowsAttribute as String) as? [AXUIElement] ?? [] {
        guard currentFolder(window) == (parent as NSString).lastPathComponent else { continue }
        for browser in find(window, "AXBrowser", nil) {
            for column in attr(browser, "AXColumns") as? [AXUIElement] ?? [] {
                for list in attr(column, "AXContents") as? [AXUIElement] ?? [] {
                    guard attr(list, kAXRoleAttribute as String) as? String == "AXList" else { continue }
                    let matched = rows(list)
                    if matched[fileName] != nil, matched[folderName] != nil {
                        exact = (list, matched)
                        break
                    }
                }
                if exact != nil { break }
            }
            if exact != nil { break }
        }
        if exact != nil { break }
    }
    if exact != nil { break }
    Thread.sleep(forTimeInterval: 0.05)
}
guard let (list, matched) = exact else {
    fputs("MIXED_EXACT_NATIVE_ROWS_NOT_VISIBLE\n", stderr); exit(6)
}
let names = choice == "file" ? [fileName] : choice == "directory" ? [folderName] : [fileName, folderName]
let selected = names.compactMap { matched[$0] }
guard selected.count == names.count else {
    fputs("MIXED_MISSING_EXACT_ROW\n", stderr); exit(7)
}
var settable = DarwinBoolean(false)
guard AXUIElementIsAttributeSettable(list, kAXSelectedChildrenAttribute as CFString, &settable) == .success,
      settable.boolValue else {
    fputs("MIXED_ROW_SELECTION_UNAVAILABLE\n", stderr); exit(8)
}
let selectionStatus = AXUIElementSetAttributeValue(
    list, kAXSelectedChildrenAttribute as CFString, NSArray(array: selected))
fputs("MIXED_EXACT_SELECTION choice=\(choice) rows=\(selected.count) status=\(selectionStatus.rawValue)\n", stderr)
let observedNames = (attr(list, kAXSelectedChildrenAttribute as String) as? [AXUIElement] ?? [])
    .flatMap { find($0, "AXTextField", nil) }
    .compactMap { attr($0, kAXValueAttribute as String) as? String }
fputs("MIXED_AX_SELECTED_CHILD_NAMES=\(observedNames.sorted())\n", stderr)
guard observedNames.count == names.count, Set(observedNames) == Set(names) else {
    fputs("MIXED_AX_ORDER_INCOMPLETE=\(observedNames) expected=\(names)\n", stderr)
    exit(9)
}
if choice == "directory", let row = matched[folderName] {
    let press = AXUIElementPerformAction(row, kAXPressAction as CFString)
    fputs("MIXED_DIRECTORY_NATIVE_AX_PRESS=\(press.rawValue)\n", stderr)
}
// The native column-browser AX setter can return a transient error after
// updating selection. The independent oracle is the *enabled native button*
// followed by the real Host's exact path reply, not the setter status alone.
var enabledButton: AXUIElement?
while ProcessInfo.processInfo.systemUptime < deadline {
    for window in attr(app, kAXWindowsAttribute as String) as? [AXUIElement] ?? [] {
        let folder = currentFolder(window)
        if folder != (parent as NSString).lastPathComponent &&
           !(choice == "directory" && folder == folderName) { continue }
        if let button = find(window, "AXButton", "Open").first,
           attr(button, kAXEnabledAttribute as String) as? Bool == true {
            enabledButton = button
            break
        }
    }
    if enabledButton != nil { break }
    Thread.sleep(forTimeInterval: 0.05)
}
guard let button = enabledButton else {
    fputs("MIXED_NATIVE_OPEN_NOT_ENABLED\n", stderr); exit(10)
}
guard AXUIElementPerformAction(button, kAXPressAction as CFString) == .success else {
    fputs("MIXED_NATIVE_OPEN_PRESS_FAILED\n", stderr); exit(11)
}
print("MIXED_EXACT_NATIVE_SELECTION pid=\(pid) choice=\(choice) rows=\(selected.count) order=\(observedNames.joined(separator: ","))")
