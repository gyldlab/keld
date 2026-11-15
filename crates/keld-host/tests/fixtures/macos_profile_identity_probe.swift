//! Signed macOS fixture probe for KEL-135/T3.
//!
//! This fixture intentionally validates the running process before extracting
//! the Team Identifier or signing identifier. Build it inside an `.app` bundle
//! and sign that bundle with a valid Apple development identity.

import CoreFoundation
import Foundation
import Security
import AppKit
import WebKit

func fail(_ message: String, _ status: OSStatus? = nil) -> Never {
    var record: [String: Any] = ["status": "failed", "message": message]
    if let status {
        record["os_status"] = status
    }
    if let data = try? JSONSerialization.data(withJSONObject: record, options: [.sortedKeys]),
       let line = String(data: data, encoding: .utf8) {
        FileHandle.standardError.write(Data((line + "\n").utf8))
    }
    exit(1)
}

let defaultFlags = SecCSFlags(rawValue: 0)
var runningCode: SecCode?
let copyStatus = SecCodeCopySelf(defaultFlags, &runningCode)
guard copyStatus == errSecSuccess, let runningCode else {
    fail("SecCodeCopySelf failed", copyStatus)
}

let validationStatus = SecCodeCheckValidity(
    runningCode,
    SecCSFlags(rawValue: kSecCSStrictValidate),
    nil
)
guard validationStatus == errSecSuccess else {
    fail("running code signature validation failed", validationStatus)
}

var staticCode: SecStaticCode?
let staticStatus = SecCodeCopyStaticCode(runningCode, defaultFlags, &staticCode)
guard staticStatus == errSecSuccess, let staticCode else {
    fail("SecCodeCopyStaticCode failed after validation", staticStatus)
}

var signingInformation: CFDictionary?
let informationStatus = SecCodeCopySigningInformation(
    staticCode,
    SecCSFlags(rawValue: kSecCSSigningInformation),
    &signingInformation
)
guard informationStatus == errSecSuccess,
      let signingInformation,
      let fields = signingInformation as? [String: Any],
      let teamIdentifier = fields[kSecCodeInfoTeamIdentifier as String] as? String,
      let signingIdentifier = fields[kSecCodeInfoIdentifier as String] as? String else {
    fail("validated signing information is missing Team Identifier or signing identifier", informationStatus)
}

let result: [String: Any] = [
    "status": "passed",
    "signature_validated_before_identity_read": true,
    "team_identifier": teamIdentifier,
    "signing_identifier": signingIdentifier,
    "macos_version": ProcessInfo.processInfo.operatingSystemVersionString,
    "webkit_version": WKVersionString()
]
guard JSONSerialization.isValidJSONObject(result),
      let data = try? JSONSerialization.data(withJSONObject: result, options: [.sortedKeys]) else {
    fail("could not encode verified identity evidence")
}
FileHandle.standardOutput.write(data)
FileHandle.standardOutput.write(Data([0x0a]))

if let mode = CommandLine.arguments.dropFirst().first {
    if mode == "--webkit-seed-probe" {
        seedWebKitStore()
    } else if mode == "--webkit-purge-probe" {
        guard let rawIdentifier = CommandLine.arguments.dropFirst(2).first,
              let identifier = UUID(uuidString: rawIdentifier) else {
            fail("purge probe requires a valid store UUID")
        }
        purgeWebKitStore(identifier)
    } else if mode == "--webkit-ephemeral-probe" {
        inspectEphemeralWebKitStore()
    }
}

func WKVersionString() -> String {
    guard let webKitBundle = Bundle(path: "/System/Library/Frameworks/WebKit.framework"),
          let version = webKitBundle.infoDictionary?["CFBundleVersion"] as? String else {
        return "unavailable"
    }
    return version
}

func seedWebKitStore() {
    guard #available(macOS 14.0, *) else {
        fail("identified-store prototype requires macOS 14 or later")
    }
    _ = NSApplication.shared

    let identifier = UUID()
    let persistentViewReadback = autoreleasepool { () -> Bool in
        let persistentStore = WKWebsiteDataStore(forIdentifier: identifier)
        guard persistentStore.isPersistent, persistentStore.identifier == identifier else {
            fail("identified store did not read back the requested UUID and persistent=true")
        }
        return inspectConfiguration(store: persistentStore)
    }
    guard persistentViewReadback else {
        fail("persistent WKWebView configuration did not retain the selected store")
    }
    let result: [String: Any] = [
        "status": "passed",
        "store_identifier": identifier.uuidString.lowercased(),
        "persistent_store_is_persistent": true,
        "persistent_configuration_same_store": persistentViewReadback
    ]
    writeJSON(result)
}

func purgeWebKitStore(_ identifier: UUID) {
    guard #available(macOS 14.0, *) else {
        fail("identified-store purge probe requires macOS 14 or later")
    }
    let app = NSApplication.shared
    app.setActivationPolicy(.prohibited)
    _ = autoreleasepool { () -> Bool in
        let transientStore = WKWebsiteDataStore.nonPersistent()
        return !transientStore.isPersistent && transientStore.identifier == nil
    }
    let delegate = PurgeWebKitDelegate(identifier: identifier)
    app.delegate = delegate
    app.run()
}

final class PurgeWebKitDelegate: NSObject, NSApplicationDelegate {
    let identifier: UUID

    init(identifier: UUID) {
        self.identifier = identifier
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        WKWebsiteDataStore.fetchAllDataStoreIdentifiers { identifiers in
            guard identifiers.contains(self.identifier) else {
                fail("identified store was absent before purge")
            }
            WKWebsiteDataStore.remove(forIdentifier: self.identifier) { error in
                guard error == nil else {
                    fail("identified-store removal failed: \(error?.localizedDescription ?? "unknown")")
                }
                WKWebsiteDataStore.fetchAllDataStoreIdentifiers { remaining in
                    guard !remaining.contains(self.identifier) else {
                        fail("removed identifier still appeared in WebKit's identifier enumeration")
                    }
                    writeJSON([
                        "status": "passed",
                        "store_identifier": self.identifier.uuidString.lowercased(),
                        "store_present_before_removal": true,
                        "removal_completion_succeeded": true,
                        "store_absent_after_completion": true
                    ])
                    NSApplication.shared.terminate(nil)
                }
            }
        }
    }
}

func inspectEphemeralWebKitStore() {
    _ = NSApplication.shared
    let ephemeralReadback = autoreleasepool { () -> Bool in
        let store = WKWebsiteDataStore.nonPersistent()
        guard !store.isPersistent, store.identifier == nil else {
            fail("nonpersistent store read-back is not ephemeral")
        }
        return inspectConfiguration(store: store)
    }
    guard ephemeralReadback else {
        fail("ephemeral WKWebView configuration did not retain the selected store")
    }
    writeJSON([
        "status": "passed",
        "store_is_persistent": false,
        "store_identifier": NSNull(),
        "configuration_same_store": true
    ])
}

func writeJSON(_ result: [String: Any]) {
    guard let data = try? JSONSerialization.data(withJSONObject: result, options: [.sortedKeys]) else {
        fail("could not encode WebKit fixture evidence")
    }
    FileHandle.standardOutput.write(data)
    FileHandle.standardOutput.write(Data([0x0a]))
}

func inspectConfiguration(store: WKWebsiteDataStore) -> Bool {
    autoreleasepool {
        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = store
        let view = WKWebView(frame: .zero, configuration: configuration)
        let observed = view.configuration.websiteDataStore
        return observed === store && observed.isPersistent == store.isPersistent
            && observed.identifier == store.identifier
    }
}
