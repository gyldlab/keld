// Test-only native Save observer. It never navigates or repairs directoryURL.
// Observe directory and initial filename BEFORE supplying the user's filename.
import AppKit
import ApplicationServices
import Foundation

guard CommandLine.arguments.count == 3,
      let pid = Int32(CommandLine.arguments[1]), pid > 0 else {
    fputs("INVALID_PID_OR_DESTINATION\n", stderr); exit(2)
}
guard AXIsProcessTrusted() else {
    fputs("ACCESSIBILITY_NOT_TRUSTED\n", stderr); exit(3)
}
let destination = CommandLine.arguments[2]
let directory = (destination as NSString).deletingLastPathComponent
let filename = (destination as NSString).lastPathComponent
let folder = (directory as NSString).lastPathComponent
var isDirectory: ObjCBool = false
guard destination.hasPrefix("/"), folder.hasPrefix("keld-gh478-visible-open-"),
      FileManager.default.fileExists(atPath: directory, isDirectory: &isDirectory),
      isDirectory.boolValue, !FileManager.default.fileExists(atPath: destination) else {
    fputs("DESTINATION_NOT_IN_OWNED_TEST_DIRECTORY\n", stderr); exit(4)
}
let process = AXUIElementCreateApplication(pid_t(pid))
func attr(_ element: AXUIElement, _ key: String) -> CFTypeRef? {
    var result: CFTypeRef?
    let status = AXUIElementCopyAttributeValue(element, key as CFString, &result)
    return status == .success ? result : nil
}
func descendants(_ element: AXUIElement, _ depth: Int = 0) -> [AXUIElement] {
    if depth > 16 { return [] }
    return [element] + (attr(element, kAXChildrenAttribute as String) as? [AXUIElement] ?? [])
        .flatMap { descendants($0, depth + 1) }
}
func isRole(_ element: AXUIElement, _ role: String) -> Bool {
    attr(element, kAXRoleAttribute as String) as? String == role
}
func writableName(_ element: AXUIElement) -> Bool {
    // The observed native Save panel also exposes a writable tag editor.
    // Require AppKit's actual filename-field AX identifier, never its ordinal.
    guard isRole(element, kAXTextFieldRole as String),
          attr(element, kAXIdentifierAttribute as String) as? String == "saveAsNameTextField" else {
        return false
    }
    var writable = DarwinBoolean(false)
    return AXUIElementIsAttributeSettable(element, kAXValueAttribute as CFString, &writable) == .success
        && writable.boolValue
}
let deadline = Date().addingTimeInterval(9)
while Date() < deadline {
    for window in attr(process, kAXWindowsAttribute as String) as? [AXUIElement] ?? [] {
        let nodes = descendants(window)
        guard let save = nodes.first(where: {
            isRole($0, kAXButtonRole as String)
                && attr($0, kAXTitleAttribute as String) as? String == "Save"
        }) else { continue }
        let places = nodes.filter {
            isRole($0, kAXPopUpButtonRole as String)
                && attr($0, kAXTitleAttribute as String) as? String == "Where:"
        }
        let names = nodes.filter(writableName)
        if names.count != 1 {
            for name in names {
                for key in ["AXTitle", "AXDescription", "AXIdentifier", "AXPlaceholderValue", "AXHelp", "AXValue"] {
                    fputs("SAVE_NAME_CANDIDATE \(key)=\(String(describing: attr(name, key)))\n", stderr)
                }
            }
        }
        guard places.count == 1, names.count == 1,
              let shownFolder = attr(places[0], kAXValueAttribute as String) as? String,
              let initialName = attr(names[0], kAXValueAttribute as String) as? String else {
            fputs("SAVE_FIELDS_NOT_UNIQUE folders=\(places.count) names=\(names.count)\n", stderr)
            exit(5)
        }
        guard shownFolder == folder else {
            fputs("WRONG_INITIAL_SAVE_DIRECTORY expected=\(folder) actual=\(shownFolder)\n", stderr)
            exit(6)
        }
        guard initialName != folder, !initialName.hasPrefix(folder + ".") else {
            fputs("DIRECTORY_BECAME_SAVE_FILENAME initial=\(initialName)\n", stderr)
            exit(7)
        }
        fputs("SAVE_INITIAL_HINT_VERIFIED directory=\(shownFolder) filename=\(initialName)\n", stderr)
        let status = AXUIElementSetAttributeValue(names[0], kAXValueAttribute as CFString, filename as CFString)
        guard status == .success,
              attr(names[0], kAXValueAttribute as String) as? String == filename else {
            fputs("SAVE_FILENAME_INPUT_FAILED ax_status=\(status.rawValue)\n", stderr); exit(8)
        }
        guard attr(save, kAXEnabledAttribute as String) as? Bool == true,
              AXUIElementPerformAction(save, kAXPressAction as CFString) == .success else {
            fputs("SAVE_BUTTON_NOT_ENABLED_OR_PRESS_FAILED\n", stderr); exit(9)
        }
        print("EXACT_HOST_DIRECTORY_SAVE_SELECTED pid=\(pid) filename=\(filename)")
        exit(0)
    }
    Thread.sleep(forTimeInterval: 0.05)
}
fputs("SAVE_PANEL_NOT_OBSERVED_FOR_EXACT_HOST\n", stderr); exit(10)
