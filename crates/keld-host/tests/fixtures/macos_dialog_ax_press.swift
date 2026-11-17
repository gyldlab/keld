// GH478: one OS accessibility action against the exact owned Host PID.
// This is a native acceptance helper, never a shipping runtime dependency.
import ApplicationServices
import Foundation

guard (CommandLine.arguments.count == 3 || CommandLine.arguments.count == 4),
      let rawPid = Int32(CommandLine.arguments[1]), rawPid > 0 else {
    fputs("INVALID_PID\n", stderr)
    exit(2)
}
guard AXIsProcessTrusted() else {
    fputs("ACCESSIBILITY_NOT_TRUSTED\n", stderr)
    exit(3)
}

let title = CommandLine.arguments[2]
let mode = CommandLine.arguments.count == 4 ? CommandLine.arguments[3] : "--press"
guard ["--press", "--observe", "--toggle-checked", "--toggle-unchecked"].contains(mode) else {
    fputs("INVALID_MODE\n", stderr)
    exit(2)
}
let observeOnly = mode == "--observe"
let initialCheckbox: Int? = mode == "--toggle-checked" ? 1 : (mode == "--toggle-unchecked" ? 0 : nil)
let process = AXUIElementCreateApplication(pid_t(rawPid))

func attr(_ element: AXUIElement, _ key: String) -> CFTypeRef? {
    var result: CFTypeRef?
    let status = AXUIElementCopyAttributeValue(element, key as CFString, &result)
    return status == .success ? result : nil
}

func visit(_ element: AXUIElement, _ depth: Int, _ wantedRole: String = kAXButtonRole as String, _ wantedTitle: String = title) -> AXUIElement? {
    if depth > 16 { return nil }
    let role = attr(element, kAXRoleAttribute as String) as? String
    let label = attr(element, kAXTitleAttribute as String) as? String
    if role == wantedRole && label == wantedTitle { return element }
    let children = attr(element, kAXChildrenAttribute as String) as? [AXUIElement] ?? []
    for child in children {
        if let found = visit(child, depth + 1, wantedRole, wantedTitle) { return found }
    }
    return nil
}

let deadline = Date().addingTimeInterval(9)
while Date() < deadline {
    let windows = attr(process, kAXWindowsAttribute as String) as? [AXUIElement] ?? []
    for window in windows {
        if let button = visit(window, 0) {
            if observeOnly {
                print("REAL_HOST_AX_BUTTON_VISIBLE pid=\(rawPid) title=\(title)")
                exit(0)
            }
            if let initial = initialCheckbox {
                guard let checkbox = visit(window, 0, kAXCheckBoxRole as String, "Remember"),
                      let before = attr(checkbox, kAXValueAttribute as String) as? NSNumber,
                      before.intValue == initial else {
                    fputs("CHECKBOX_INITIAL_STATE_MISMATCH\n", stderr)
                    exit(7)
                }
                guard AXUIElementPerformAction(checkbox, kAXPressAction as CFString) == .success else {
                    fputs("CHECKBOX_PRESS_FAILED\n", stderr)
                    exit(8)
                }
                var changed = false
                while Date() < deadline {
                    if let after = attr(checkbox, kAXValueAttribute as String) as? NSNumber,
                       after.intValue == 1 - initial {
                        changed = true
                        break
                    }
                    Thread.sleep(forTimeInterval: 0.01)
                }
                guard changed else {
                    fputs("CHECKBOX_STATE_DID_NOT_CHANGE\n", stderr)
                    exit(9)
                }
                print("REAL_HOST_AX_CHECKBOX_TOGGLED initial=\(initial) final=\(1 - initial)")
            }
            let action = AXUIElementPerformAction(button, kAXPressAction as CFString)
            guard action == .success else {
                fputs("REAL_BUTTON_PRESS_FAILED: \(action.rawValue)\n", stderr)
                exit(5)
            }
            print("REAL_HOST_AX_BUTTON_PRESSED pid=\(rawPid) title=\(title)")
            exit(0)
        }
    }
    Thread.sleep(forTimeInterval: 0.05)
}
fputs("REAL_BUTTON_NOT_FOUND_FOR_EXACT_HOST_PID\n", stderr)
exit(6)
