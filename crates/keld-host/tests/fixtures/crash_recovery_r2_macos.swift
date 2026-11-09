// GH-1081 round-2 (research only). Two modes, one JSON line each.
//   pixels <cg-window-id> <png-path>: screencapture that one window, then
//     sample the central 60% of the image (read-only).
//   input <pid> <title> <cmd-r|context-reload>: click the host window centre
//     (real CoreGraphics events), then post Cmd+R, or open the WebKit context
//     menu with a right click and choose its first enabled item (Down, Return).
// Refuses to type unless the host is the frontmost application.
import AppKit
import CoreGraphics
import Foundation
import ImageIO

func emit(_ value: [String: Any]) -> Never {
  let data = try! JSONSerialization.data(withJSONObject: value, options: [.sortedKeys])
  print(String(data: data, encoding: .utf8)!)
  exit(0)
}

func windows() -> [[String: Any]] {
  CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
    as? [[String: Any]] ?? []
}
func int(_ row: [String: Any], _ key: CFString) -> Int { (row[key as String] as? NSNumber)?.intValue ?? -1 }
func bounds(_ row: [String: Any]) -> CGRect {
  var rect = CGRect.zero
  if let dict = row[kCGWindowBounds as String] { _ = CGRectMakeWithDictionaryRepresentation(dict as! CFDictionary, &rect) }
  return rect
}

let args = CommandLine.arguments
if args.count >= 4 && args[1] == "pixels" {
  let path = args[3]
  let capture = Process()
  capture.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
  capture.arguments = ["-x", "-o", "-l", args[2], path]
  try! capture.run()
  capture.waitUntilExit()
  guard capture.terminationStatus == 0,
        let source = CGImageSourceCreateWithURL(URL(fileURLWithPath: path) as CFURL, nil),
        let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
    emit(["ok": false, "status": Int(capture.terminationStatus)])
  }
  let width = image.width, height = image.height
  var pixels = [UInt8](repeating: 0, count: width * height * 4)
  let context = CGContext(data: &pixels, width: width, height: height, bitsPerComponent: 8,
    bytesPerRow: width * 4, space: CGColorSpaceCreateDeviceRGB(),
    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
  context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
  var dark = 0, light = 0, total = 0
  func rgb(_ x: Int, _ y: Int) -> (Int, Int, Int) {
    let offset = (y * width + x) * 4
    return (Int(pixels[offset]), Int(pixels[offset + 1]), Int(pixels[offset + 2]))
  }
  for row in 0..<16 {
    for column in 0..<16 {
      let x = min(Int(Double(width) * (0.2 + 0.6 * Double(column) / 15)), width - 1)
      let y = min(Int(Double(height) * (0.2 + 0.6 * Double(row) / 15)), height - 1)
      let (r, g, b) = rgb(x, y)
      total += 1
      if max(r, g, b) < 64 { dark += 1 }
      if min(r, g, b) > 200 { light += 1 }
    }
  }
  let (r, g, b) = rgb(width / 2, height / 2)
  emit(["ok": true, "width": width, "height": height, "dark": Double(dark) / Double(total),
        "light": Double(light) / Double(total), "center": String(format: "#%02x%02x%02x", r, g, b)])
}

// Round 4: find <pid> <title>: the on-screen layer-0 window of that process
// with that exact title (its CGWindow id feeds the `pixels` mode). Read-only.
if args.count >= 4 && args[1] == "find", let pid = Int32(args[2]) {
  let title = args[3]
  guard let row = windows().first(where: {
    int($0, kCGWindowOwnerPID) == Int(pid) && $0[kCGWindowName as String] as? String == title
      && int($0, kCGWindowLayer) == 0
  }) else { emit(["ok": false, "error": "no window", "pid": Int(pid), "title": title]) }
  let frame = bounds(row)
  emit(["ok": true, "window": int(row, kCGWindowNumber), "pid": Int(pid), "title": title,
        "bounds": [frame.origin.x, frame.origin.y, frame.size.width, frame.size.height]])
}

guard args.count >= 5 && args[1] == "input", let pid = Int32(args[2]) else {
  FileHandle.standardError.write(Data("usage\n".utf8))
  exit(2)
}
let title = args[3]
let action = args[4]
if !CGPreflightPostEventAccess() { emit(["ok": false, "error": "post_event_access=false"]) }
guard let host = windows().first(where: {
  int($0, kCGWindowOwnerPID) == Int(pid) && $0[kCGWindowName as String] as? String == title
    && int($0, kCGWindowLayer) == 0
}) else { emit(["ok": false, "error": "no host window"]) }
let frame = bounds(host)
let centre = CGPoint(x: frame.midX, y: frame.midY)
let source = CGEventSource(stateID: .hidSystemState)
func mouse(_ down: CGEventType, _ up: CGEventType, _ button: CGMouseButton) {
  for type in [down, up] {
    CGEvent(mouseEventSource: source, mouseType: type, mouseCursorPosition: centre, mouseButton: button)!
      .post(tap: .cghidEventTap)
    usleep(30_000)
  }
}
func key(_ code: CGKeyCode, _ flags: CGEventFlags = []) {
  for down in [true, false] {
    let event = CGEvent(keyboardEventSource: source, virtualKey: code, keyDown: down)!
    event.flags = flags
    event.post(tap: .cghidEventTap)
  }
}
// Wait (bounded) on an observable, servicing the run loop so NSWorkspace updates.
func await_(_ seconds: Double, _ condition: () -> Bool) -> Bool {
  let deadline = Date().addingTimeInterval(seconds)
  while Date() < deadline {
    if condition() { return true }
    RunLoop.current.run(until: Date().addingTimeInterval(0.02))
  }
  return condition()
}
mouse(.leftMouseDown, .leftMouseUp, .left)
let active = await_(5) { NSWorkspace.shared.frontmostApplication?.processIdentifier == pid }
if !active { emit(["ok": false, "error": "host not frontmost", "frontmost": Int(NSWorkspace.shared.frontmostApplication?.processIdentifier ?? -1)]) }
var result: [String: Any] = ["ok": true, "action": action, "frontmost": true,
  "centre": [centre.x, centre.y]]
if action == "cmd-r" {
  key(15, .maskCommand)  // kVK_ANSI_R
} else if action == "context-reload" {
  let before = Set(windows().filter { int($0, kCGWindowOwnerPID) == Int(pid) }.map { int($0, kCGWindowNumber) })
  mouse(.rightMouseDown, .rightMouseUp, .right)
  var menu: [String: Any]? = nil
  let seen = await_(5) {
    menu = windows().first { int($0, kCGWindowOwnerPID) == Int(pid) && !before.contains(int($0, kCGWindowNumber)) && int($0, kCGWindowLayer) != 0 }
    return menu != nil
  }
  result["menu_seen"] = seen
  if let menu { result["menu_layer"] = int(menu, kCGWindowLayer) }
  if seen {
    key(125)  // kVK_DownArrow: first enabled item
    key(36)   // kVK_Return
    result["menu_closed"] = await_(5) {
      !windows().contains { int($0, kCGWindowOwnerPID) == Int(pid) && int($0, kCGWindowLayer) == (menu.map { int($0, kCGWindowLayer) } ?? -2) && !before.contains(int($0, kCGWindowNumber)) }
    }
  }
}
emit(result)
