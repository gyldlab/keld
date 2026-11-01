import AppKit
import Foundation
import Darwin

// Independent public AppKit observer. It creates no window and changes no settings.
func rect(_ value: NSRect, _ primary: NSRect) -> [String: Double] {
    ["x": value.minX - primary.minX, "y": primary.maxY - value.maxY,
     "width": value.width, "height": value.height]
}
final class DesktopObserver: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        do {
            let screens = NSScreen.screens
            guard Thread.isMainThread, NSApp.windows.isEmpty,
                  let first = screens.first, !screens.isEmpty else {
                throw NSError(domain: "DesktopObserver", code: 1)
            }
            var displays: [[String: Any]] = []
            for screen in screens {
                guard let number = screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber else {
                    throw NSError(domain: "DesktopObserver", code: 2)
                }
                displays.append([
                    "id": number.uint32Value,
                    "bounds": rect(screen.frame, first.frame),
                    "workArea": rect(screen.visibleFrame, first.frame),
                    "scaleFactor": screen.backingScaleFactor,
                ])
            }
            guard let appearance = NSApp.effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) else {
                throw NSError(domain: "DesktopObserver", code: 3)
            }
            let result: [String: Any] = [
                "primary": displays[0], "displays": displays,
                "dark": appearance == .darkAqua,
            ]
            FileHandle.standardOutput.write(try JSONSerialization.data(withJSONObject: result))
            exit(0)
        } catch {
            FileHandle.standardError.write(Data("DESKTOP_OBSERVER_FAILURE \(error)\n".utf8))
            exit(1)
        }
    }
}
let delegate = DesktopObserver()
let application = NSApplication.shared
application.setActivationPolicy(.prohibited)
application.delegate = delegate
application.run()
