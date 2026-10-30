// Independent real-Host AppKit accessory switch. Never set model state or
// synthesize a file result: only the OS-visible popup can change its group.
import AppKit
import ApplicationServices
import Foundation

guard CommandLine.arguments.count == 4,
      let pid = Int32(CommandLine.arguments[1]), pid > 0 else {
    fputs("FILTER_SWITCH_BAD_ARGUMENTS\n", stderr); exit(2)
}
guard AXIsProcessTrusted() else {
    fputs("FILTER_SWITCH_ACCESSIBILITY_NOT_TRUSTED\n", stderr); exit(3)
}
let file = CommandLine.arguments[2]
let group = CommandLine.arguments[3]
let duplicateLabels = group == "Duplicate"
let labels = ["draw.io", "VSDX", "Mermaid", "All Files", "Duplicate", "Other"]
guard labels.contains(group), file.hasPrefix("/"),
      FileManager.default.fileExists(atPath: file),
      NSRunningApplication(processIdentifier: pid_t(pid)) != nil else {
    fputs("FILTER_SWITCH_NOT_AN_OWNED_EXISTING_FILE_OR_GROUP\n", stderr); exit(4)
}
let folder = ((file as NSString).deletingLastPathComponent as NSString).lastPathComponent
let app = AXUIElementCreateApplication(pid_t(pid))
guard AXUIElementSetMessagingTimeout(AXUIElementCreateSystemWide(), 1) == .success else {
    fputs("FILTER_SWITCH_AX_TIMEOUT_SETUP_FAILED\n", stderr); exit(5)
}
let deadline = ProcessInfo.processInfo.systemUptime + 9
func attr(_ node: AXUIElement, _ name: String) -> CFTypeRef? {
    var value: CFTypeRef?
    let status = AXUIElementCopyAttributeValue(node, name as CFString, &value)
    return status == .success ? value : nil
}
func descendants(_ node: AXUIElement, _ depth: Int = 0) -> [AXUIElement] {
    if depth > 15 { return [] }
    let children = attr(node, kAXChildrenAttribute as String) as? [AXUIElement] ?? []
    return [node] + children.flatMap { descendants($0, depth + 1) }
}
func owningWindow() -> AXUIElement? {
    for window in attr(app, kAXWindowsAttribute as String) as? [AXUIElement] ?? [] {
        let nodes = descendants(window)
        let folders = nodes.filter {
            attr($0, kAXRoleAttribute as String) as? String == "AXPopUpButton" &&
            attr($0, kAXTitleAttribute as String) as? String == "Where:"
        }
        let hasOpen = nodes.contains {
            attr($0, kAXRoleAttribute as String) as? String == "AXButton" &&
            attr($0, kAXTitleAttribute as String) as? String == "Open"
        }
        if hasOpen, folders.count == 1,
           attr(folders[0], kAXValueAttribute as String) as? String == folder { return window }
    }
    return nil
}
_ = AXUIElementSetAttributeValue(app, kAXFrontmostAttribute as CFString, kCFBooleanTrue)
var chooser: AXUIElement?
while ProcessInfo.processInfo.systemUptime < deadline {
    chooser = owningWindow()
    if chooser != nil { break }
    Thread.sleep(forTimeInterval: 0.04)
}
guard let window = chooser else {
    fputs("FILTER_SWITCH_EXACT_CHOOSER_NOT_VISIBLE\n", stderr); exit(6)
}
func accessoryPopups(_ window: AXUIElement) -> [AXUIElement] {
    descendants(window).filter {
        attr($0, kAXRoleAttribute as String) as? String == "AXPopUpButton" &&
        (attr($0, kAXValueAttribute as String) as? String).map(labels.contains) == true
    }
}
var candidates = accessoryPopups(window)
if candidates.isEmpty {
    // NSOpenPanel may initially fold its app-defined accessory behind the
    // genuine "Show Options" disclosure. That is not a missing filter and
    // must be opened through the OS UI, not by mutating Keld's native model.
    let disclosure = descendants(window).filter {
        attr($0, kAXRoleAttribute as String) as? String == "AXButton" &&
        attr($0, kAXTitleAttribute as String) as? String == "Show Options"
    }
    guard disclosure.count == 1 else {
        fputs("FILTER_SWITCH_OPTIONS_DISCLOSURE_COUNT=\(disclosure.count)\n", stderr); exit(10)
    }
    let pressed = AXUIElementPerformAction(disclosure[0], kAXPressAction as CFString)
    fputs("FILTER_SWITCH_SHOW_OPTIONS_AX_PRESS=\(pressed.rawValue)\n", stderr)
    guard pressed == .success else { exit(11) }
    while ProcessInfo.processInfo.systemUptime < deadline {
        if let expandedWindow = owningWindow() {
            candidates = accessoryPopups(expandedWindow)
            if candidates.count == 1 { break }
        }
        Thread.sleep(forTimeInterval: 0.04)
    }
}
guard candidates.count == 1 else {
    let popups = descendants(window).filter {
        attr($0, kAXRoleAttribute as String) as? String == "AXPopUpButton"
    }
    for p in popups {
        fputs("FILTER_SWITCH_POPUP title=\(String(describing: attr(p, kAXTitleAttribute as String))) value=\(String(describing: attr(p, kAXValueAttribute as String)))\n", stderr)
    }
    fputs("FILTER_SWITCH_ACCESSORY_COUNT=\(candidates.count)\n", stderr); exit(7)
}
let popup = candidates[0]
guard attr(popup, kAXValueAttribute as String) as? String ==
        (duplicateLabels ? "Duplicate" : "draw.io") else {
    fputs("FILTER_SWITCH_WRONG_INITIAL_GROUP\n", stderr); exit(8)
}
// Changing AXValue directly can bypass the real NSPopUpButton menu and let a
// first-group-only implementation pass. Require two OS actions: press the
// actual popup, then press the exact native menu item (including duplicates).
let press = AXUIElementPerformAction(popup, kAXPressAction as CFString)
fputs("FILTER_SWITCH_POPUP_PRESS status=\(press.rawValue)\n", stderr)
guard press == .success else {
    fputs("FILTER_SWITCH_NATIVE_POPUP_PRESS_FAILED\n", stderr)
    exit(11)
}
var item: AXUIElement?
var observedMenuTitles: [String] = []
while ProcessInfo.processInfo.systemUptime < deadline {
    let windows = attr(app, kAXWindowsAttribute as String) as? [AXUIElement] ?? []
    let roots = [popup, window] + windows
    let elements = roots.flatMap { descendants($0) }
    // popup/window/app roots overlap. Counting the same AX menu item twice
    // would make the duplicate-label oracle pass without two real groups.
    var menus: [AXUIElement] = []
    for candidate in elements where attr(candidate, kAXRoleAttribute as String) as? String == "AXMenuItem" {
        if !menus.contains(where: { CFEqual($0, candidate) }) {
            menus.append(candidate)
        }
    }
    observedMenuTitles = menus.compactMap { attr($0, kAXTitleAttribute as String) as? String }
    let matches = menus.filter {
        attr($0, kAXRoleAttribute as String) as? String == "AXMenuItem" &&
        attr($0, kAXTitleAttribute as String) as? String == group
    }
    if duplicateLabels ? matches.count >= 2 : !matches.isEmpty {
        // The second same-named native item is the third original
        // group, not the first "Duplicate" item at source index 0.
        item = duplicateLabels ? matches[1] : matches[0]
        break
    }
    Thread.sleep(forTimeInterval: 0.04)
}
guard let menuItem = item else {
    fputs("FILTER_SWITCH_MISSING_MENU_ITEM group=\(group) observed=\(observedMenuTitles.sorted())\n", stderr)
    exit(12)
}
let menuPress = AXUIElementPerformAction(menuItem, kAXPressAction as CFString)
fputs("FILTER_SWITCH_MENU_PRESS status=\(menuPress.rawValue)\n", stderr)
guard menuPress == .success else {
    fputs("FILTER_SWITCH_NATIVE_MENU_ITEM_PRESS_FAILED\n", stderr)
    exit(13)
}
while ProcessInfo.processInfo.systemUptime < deadline {
    if attr(popup, kAXValueAttribute as String) as? String == group {
        print("FILTER_SWITCH_NATIVE_CHANGED pid=\(pid) group=\(group) folder=\(folder)")
        exit(0)
    }
    Thread.sleep(forTimeInterval: 0.04)
}
fputs("FILTER_SWITCH_DID_NOT_CHANGE actual=\(String(describing: attr(popup, kAXValueAttribute as String))) expected=\(group)\n", stderr)
exit(9)
