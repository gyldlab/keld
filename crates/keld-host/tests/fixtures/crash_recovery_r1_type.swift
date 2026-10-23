// GH-1081 round-1 (research only): post real OS keyboard events, one UTF-16
// unit per key down/up pair, into the frontmost application. Runs after the
// shared CLICK_SCRIPT focused the host textarea; refuses unless the host is the
// active application, so keystrokes never land in another program.
import AppKit
import CoreGraphics
import Foundation

let pid = pid_t(CommandLine.arguments[1])!
let text = CommandLine.arguments[2]
if !CGPreflightPostEventAccess() {
  FileHandle.standardError.write(Data("TYPE_POSTER_UNAVAILABLE post_event_access=false\n".utf8))
  exit(5)
}
let frontmost = NSWorkspace.shared.frontmostApplication?.processIdentifier ?? -1
guard frontmost == pid else {
  print("TYPE_TARGET_NOT_ACTIVE frontmost=\(frontmost) target=\(pid)")
  exit(3)
}
let source = CGEventSource(stateID: .hidSystemState)
for unit in text.utf16 {
  var code = [unit]
  for down in [true, false] {
    // Key code 0 carries the Unicode override; the text input system inserts `unit`.
    let event = CGEvent(keyboardEventSource: source, virtualKey: 0, keyDown: down)!
    event.keyboardSetUnicodeString(stringLength: 1, unicodeString: &code)
    event.post(tap: .cghidEventTap)
  }
}
print("TYPED units=\(text.utf16.count) target=\(pid)")
