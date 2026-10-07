// PANEL-P2 (gyldlab/keld#419) AppKit close/quit veto harness. Scratch only.
// Async veto hooks: windowShouldClose: -> NO + async role decision;
// applicationShouldTerminate: -> NSTerminateLater + replyToApplicationShouldTerminate:.
// Build: swiftc -swift-version 5 -O harness.swift -o harness
// Run:   ./harness <scenario> <runIndex> <out.jsonl>
import AppKit
import Foundation

// MARK: - Logger (JSONL, seq + monotonic ms + wall clock)

final class Log {
    let lock = NSLock()
    let fh: UnsafeMutablePointer<FILE>
    var seq = 0
    let t0 = DispatchTime.now().uptimeNanoseconds
    let scenario: String
    let run: Int
    var exiting = false
    let iso: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return f
    }()
    init(path: String, scenario: String, run: Int) {
        guard let f = fopen(path, "w") else { fatalError("cannot open \(path)") }
        fh = f
        self.scenario = scenario
        self.run = run
    }
    func ev(_ name: String, _ fields: [String: Any] = [:]) {
        var d = fields
        let onMain = Thread.isMainThread
        d["thread"] = onMain ? "main" : "bg:" + (String(cString: __dispatch_queue_get_label(nil)))
        if onMain {
            if let m = CFRunLoopCopyCurrentMode(CFRunLoopGetMain()) {
                d["rl_mode"] = m.rawValue as String
            } else {
                d["rl_mode"] = "none"
            }
            if !exiting { d["ns_modal_window"] = (NSApp.modalWindow != nil) }
        }
        lock.lock()
        seq += 1
        if seq > 3000 { // runaway guard: the disk is nearly full on this host
            fputs("{\"ev\":\"log-cap-exceeded\"}\n", fh); fflush(fh); exit(3)
        }
        d["seq"] = seq
        d["t_ms"] = Double(DispatchTime.now().uptimeNanoseconds - t0) / 1_000_000.0
        d["wall"] = iso.string(from: Date())
        d["ev"] = name
        d["scenario"] = scenario
        d["run"] = run
        if let data = try? JSONSerialization.data(withJSONObject: d, options: [.sortedKeys]),
           let s = String(data: data, encoding: .utf8) {
            fputs(s + "\n", fh)
            fflush(fh)
        }
        lock.unlock()
    }
}

var gLog: Log!
var gWillQuitSeen = false

// MARK: - Delivery mechanisms (simulated role -> host main thread)

enum Delivery: String {
    case gcdMain = "gcd-main-async"
    case performMain = "performSelectorOnMainThread(common)"
    case cfrlDefault = "CFRunLoopPerformBlock(default)"
    case cfrlCommon = "CFRunLoopPerformBlock(common)"
    case cfrlModal = "CFRunLoopPerformBlock(modalPanel)"
}

final class Box: NSObject {
    let b: () -> Void
    init(_ b: @escaping () -> Void) { self.b = b }
    @objc func fire() { b() }
}

func deliver(_ d: Delivery, _ block: @escaping () -> Void) {
    let rl = CFRunLoopGetMain()
    switch d {
    case .gcdMain:
        DispatchQueue.main.async(execute: block)
    case .performMain:
        let box = Box(block)
        box.performSelector(onMainThread: #selector(Box.fire), with: nil, waitUntilDone: false)
    case .cfrlDefault:
        CFRunLoopPerformBlock(rl, CFRunLoopMode.defaultMode.rawValue, block)
        CFRunLoopWakeUp(rl)
    case .cfrlCommon:
        CFRunLoopPerformBlock(rl, CFRunLoopMode.commonModes.rawValue, block)
        CFRunLoopWakeUp(rl)
    case .cfrlModal:
        CFRunLoopPerformBlock(rl, RunLoop.Mode.modalPanel.rawValue as CFString, block)
        CFRunLoopWakeUp(rl)
    }
}

func timer(_ ms: Int, mode: RunLoop.Mode, _ block: @escaping () -> Void) {
    let t = Timer(timeInterval: Double(ms) / 1000.0, repeats: false) { _ in block() }
    RunLoop.main.add(t, forMode: mode)
}

// MARK: - Model

enum Hook: String { case asyncGated = "async-gated", syncYes = "sync-yes", asyncUngated = "async-ungated" }

struct Dismiss { let method: String; let button: Int; let delayMs: Int } // method: click|abort

struct Config {
    var windows: [(String, Bool)] = [("W1", true), ("W2", false)]
    var hook: Hook = .asyncGated
    var roleDelivery: Delivery = .cfrlCommon
    var roleDelayMs = 80
    var closeRoleDelayOverrideMs: [String: Int] = [:]
    var closeTimeoutMs: Int? = nil
    var dismiss: [Int: Dismiss] = [:]            // by global dialog index (1-based)
    var defaultDismiss = Dismiss(method: "click", button: 1, delayMs: 400) // button 1 = "Don't Save"
    var dialogChoice: [String: Int] = [:]        // per window: button index to click
    var quitPolicy = "stop-at-first-veto"
    var probesOnDialog = false
    var probesOnTerminate = false
    var secondCloseDuringModalMs: Int? = nil
    var steps: [String] = []
    var watchdogMs = 8000
}

final class Win {
    let id: String
    let ns: NSWindow
    var dirty: Bool
    var tombstoned = false
    var pendingTxn: Int? = nil
    var txnCounter = 0
    var closeRequests = 0
    var onOutcome: ((String) -> Void)? = nil
    init(id: String, ns: NSWindow, dirty: Bool) { self.id = id; self.ns = ns; self.dirty = dirty }
}

// MARK: - Host

final class Host: NSObject, NSApplicationDelegate, NSWindowDelegate {
    var cfg: Config
    var wins: [Win] = []
    let roleQueue = DispatchQueue(label: "role")
    var outstandingRole = 0
    var openDialogs = 0
    var dialogIndex = 0
    var probesPending = 0
    var quitting = false
    var quitReplied = false
    var quitAnyVeto = false
    var quitQueue: [Win] = []
    var finishing = false
    var stepRunning = false

    init(cfg: Config) { self.cfg = cfg }

    func win(_ ns: NSWindow) -> Win? { wins.first { $0.ns === ns } }

    // MARK: launch / steps
    func applicationDidFinishLaunching(_ n: Notification) {
        gLog.ev("harness-start", [
            "os": ProcessInfo.processInfo.operatingSystemVersionString,
            "hook": cfg.hook.rawValue, "role_delivery": cfg.roleDelivery.rawValue,
            "role_delay_ms": cfg.roleDelayMs, "quit_policy": cfg.quitPolicy,
            "close_timeout_ms": cfg.closeTimeoutMs ?? NSNull(),
        ])
        var x: CGFloat = 120
        for (id, dirty) in cfg.windows {
            let w = NSWindow(contentRect: NSRect(x: x, y: 300, width: 320, height: 200),
                             styleMask: [.titled, .closable, .resizable], backing: .buffered, defer: false)
            w.isReleasedWhenClosed = false
            w.title = "\(id)\(dirty ? " (dirty)" : "")"
            w.isDocumentEdited = dirty
            w.delegate = self
            w.makeKeyAndOrderFront(nil)
            wins.append(Win(id: id, ns: w, dirty: dirty))
            gLog.ev("window-created", ["win": id, "dirty": dirty])
            x += 360
        }
        NSApp.activate(ignoringOtherApps: true)
        let wd = cfg.watchdogMs
        DispatchQueue.global().asyncAfter(deadline: .now() + .milliseconds(wd)) {
            gLog.ev("watchdog", ["ms": wd, "outstanding_role": self.outstandingRole, "quitting": self.quitting, "quit_replied": self.quitReplied])
            gLog.exiting = true
            exit(2)
        }
        timer(200, mode: .common) { self.idleCheck() }
    }

    func scheduleIdle() {
        CFRunLoopPerformBlock(CFRunLoopGetMain(), CFRunLoopMode.commonModes.rawValue) { self.idleCheck() }
        CFRunLoopWakeUp(CFRunLoopGetMain())
    }

    func idleCheck() {
        if finishing || stepRunning { return }
        let busy = outstandingRole > 0 || openDialogs > 0 || probesPending > 0
            || (quitting && !quitReplied) || wins.contains { $0.pendingTxn != nil }
        if busy { return }
        if cfg.steps.isEmpty {
            finishing = true
            timer(250, mode: .common) {
                gLog.ev("harness-end", ["windows": self.wins.map { ["win": $0.id, "visible": $0.ns.isVisible, "tombstoned": $0.tombstoned] }])
                gLog.exiting = true
                exit(0)
            }
            return
        }
        let step = cfg.steps.removeFirst()
        stepRunning = true
        timer(150, mode: .common) {
            self.stepRunning = false
            self.runStep(step)
            self.scheduleIdle()
        }
    }

    func runStep(_ step: String) {
        gLog.ev("step", ["step": step])
        if step.hasPrefix("close:") {
            let id = String(step.dropFirst(6))
            guard let w = wins.first(where: { $0.id == id }) else { return }
            gLog.ev("close-request", ["win": id, "source": "step:performClose"])
            w.ns.performClose(nil)
        } else if step == "quit" {
            gLog.ev("terminate-call", [:])
            NSApp.terminate(nil)
            gLog.ev("terminate-returned", [:])
        }
    }

    // MARK: role (simulated supervised Bun role)
    func roleAsk(_ w: Win, txn: Int, kind: String, payload: String?) {
        let reply: String
        var delay = cfg.roleDelayMs
        if kind == "close" {
            reply = w.dirty ? "veto_prompt" : "allow"
            if let o = cfg.closeRoleDelayOverrideMs[w.id] { delay = o }
        } else {
            reply = (payload == "discard") ? "allow" : "veto" // cancel/abort fail closed
        }
        outstandingRole += 1
        gLog.ev("role-ask", ["win": w.id, "txn": txn, "kind": kind, "payload": payload ?? NSNull(), "delay_ms": delay])
        let via = cfg.roleDelivery
        roleQueue.asyncAfter(deadline: .now() + .milliseconds(delay)) {
            gLog.ev("role-send", ["win": w.id, "txn": txn, "kind": kind, "reply": reply, "via": via.rawValue])
            deliver(via) { self.onRoleReply(w, txn: txn, kind: kind, reply: reply, via: via) }
        }
    }

    func onRoleReply(_ w: Win, txn: Int, kind: String, reply: String, via: Delivery) {
        outstandingRole -= 1
        gLog.ev("role-reply", ["win": w.id, "txn": txn, "kind": kind, "reply": reply, "via": via.rawValue, "open_dialogs": openDialogs])
        defer { scheduleIdle() }
        if w.tombstoned {
            gLog.ev("stale-reply-dropped", ["win": w.id, "txn": txn, "reason": "tombstoned"])
            return
        }
        if cfg.hook == .asyncGated && w.pendingTxn != txn {
            gLog.ev("stale-reply-dropped", ["win": w.id, "txn": txn, "reason": "txn-mismatch"])
            return
        }
        switch reply {
        case "allow": commitClose(w, txn: txn, reason: "role-allow")
        case "veto_prompt": showPrompt(w, txn: txn)
        default: endTxn(w, txn: txn, outcome: "veto")
        }
    }

    // MARK: dialog
    func showPrompt(_ w: Win, txn: Int) {
        dialogIndex += 1
        let idx = dialogIndex
        let alert = NSAlert()
        alert.messageText = "\(w.id) has unsaved changes"
        alert.informativeText = "txn \(txn) dialog \(idx)"
        alert.addButton(withTitle: "Cancel")      // NSAlertFirstButtonReturn = 1000
        alert.addButton(withTitle: "Don't Save")  // NSAlertSecondButtonReturn = 1001
        openDialogs += 1
        var d = cfg.dismiss[idx] ?? cfg.defaultDismiss
        if cfg.dismiss[idx] == nil, let b = cfg.dialogChoice[w.id] { d = Dismiss(method: d.method, button: b, delayMs: d.delayMs) }
        gLog.ev("dialog", ["phase": "open", "win": w.id, "txn": txn, "idx": idx, "open_dialogs": openDialogs,
                           "api": "NSAlert.runModal", "opened_from_delivery": cfg.roleDelivery.rawValue,
                           "dismiss_method": d.method, "dismiss_button": d.button, "dismiss_delay_ms": d.delayMs])
        timer(d.delayMs, mode: .modalPanel) {
            gLog.ev("dialog-dismiss", ["win": w.id, "idx": idx, "method": d.method == "click" ? "NSButton.performClick(button \(d.button))" : "NSApp.abortModal"])
            if d.method == "click" { alert.buttons[d.button].performClick(nil) } else { NSApp.abortModal() }
        }
        if cfg.probesOnDialog && idx == 1 { startProbes(tag: "dialog") }
        if let ms = cfg.secondCloseDuringModalMs, idx == 1 {
            timer(ms, mode: .modalPanel) {
                gLog.ev("close-request", ["win": w.id, "source": "during-modal:NSWindow.performClose"])
                w.ns.performClose(nil)
                gLog.ev("close-request-returned", ["win": w.id, "source": "during-modal:NSWindow.performClose"])
            }
            timer(ms + 100, mode: .modalPanel) {
                gLog.ev("close-request", ["win": w.id, "source": "during-modal:host-close-api"])
                if self.requestClose(w, entry: "host-close-api") { w.ns.close() }
            }
        }
        let resp = alert.runModal()
        openDialogs -= 1
        let mapped: String
        switch resp {
        case .alertFirstButtonReturn: mapped = "cancel"
        case .alertSecondButtonReturn: mapped = "discard"
        case .abort: mapped = "abort"
        default: mapped = "other"
        }
        gLog.ev("dialog", ["phase": "result", "win": w.id, "txn": txn, "idx": idx, "response_raw": resp.rawValue,
                           "response": mapped, "open_dialogs": openDialogs])
        roleAsk(w, txn: txn, kind: "dialog-result", payload: mapped)
    }

    // MARK: close lifecycle
    func windowShouldClose(_ sender: NSWindow) -> Bool {
        guard let w = win(sender) else { return true }
        return requestClose(w, entry: "windowShouldClose:")
    }

    /// Single owner of the close decision; both the AppKit hook and the host's
    /// programmatic close API (a role calling win.close()) enter here.
    func requestClose(_ w: Win, entry: String) -> Bool {
        w.closeRequests += 1
        gLog.ev("close", ["win": w.id, "dirty": w.dirty, "request_no": w.closeRequests, "hook": cfg.hook.rawValue,
                          "entry": entry, "pending_txn": w.pendingTxn ?? NSNull(), "open_dialogs": openDialogs])
        switch cfg.hook {
        case .syncYes:
            gLog.ev("close-return", ["win": w.id, "returned": true, "sync": true])
            return true
        case .asyncGated:
            if w.tombstoned {
                gLog.ev("close-return", ["win": w.id, "returned": false, "reason": "tombstoned"])
                return false
            }
            if let p = w.pendingTxn {
                gLog.ev("close-coalesced", ["win": w.id, "pending_txn": p])
                gLog.ev("close-return", ["win": w.id, "returned": false, "reason": "coalesced"])
                return false
            }
        case .asyncUngated:
            break
        }
        w.txnCounter += 1
        let txn = w.txnCounter
        w.pendingTxn = txn
        gLog.ev("close-return", ["win": w.id, "returned": false, "txn": txn, "reason": "await-role"])
        roleAsk(w, txn: txn, kind: "close", payload: nil)
        if let to = cfg.closeTimeoutMs {
            timer(to, mode: .common) {
                if w.pendingTxn == txn && !w.tombstoned {
                    gLog.ev("close-timeout", ["win": w.id, "txn": txn, "ms": to])
                    self.commitClose(w, txn: txn, reason: "timeout")
                }
            }
        }
        return false
    }

    func commitClose(_ w: Win, txn: Int, reason: String) {
        w.tombstoned = true
        w.pendingTxn = nil
        gLog.ev("tombstone", ["win": w.id, "txn": txn, "reason": reason])
        gLog.ev("destroy-call", ["win": w.id, "api": "NSWindow.close"])
        w.ns.close()
        gLog.ev("destroy-call-returned", ["win": w.id, "visible": w.ns.isVisible])
    }

    func endTxn(_ w: Win, txn: Int, outcome: String) {
        w.pendingTxn = nil
        gLog.ev("close-outcome", ["win": w.id, "txn": txn, "outcome": outcome, "visible": w.ns.isVisible])
        let cb = w.onOutcome
        w.onOutcome = nil
        cb?(outcome)
    }

    func windowWillClose(_ n: Notification) {
        guard let ns = n.object as? NSWindow, let w = win(ns) else { return }
        gLog.ev("destroy", ["win": w.id, "api": "windowWillClose:", "tombstoned": w.tombstoned])
        CFRunLoopPerformBlock(CFRunLoopGetMain(), CFRunLoopMode.commonModes.rawValue) {
            gLog.ev("closed", ["win": w.id, "visible": w.ns.isVisible])
            gLog.ev("tombstone-before-closed", ["win": w.id, "ok": w.tombstoned])
            w.pendingTxn = nil
            gLog.ev("close-outcome", ["win": w.id, "outcome": "closed"])
            let cb = w.onOutcome
            w.onOutcome = nil
            cb?("closed")
            self.scheduleIdle()
        }
        CFRunLoopWakeUp(CFRunLoopGetMain())
    }

    // MARK: quit
    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        gLog.ev("before-quit", ["api": "applicationShouldTerminate:"])
        if quitting {
            gLog.ev("should-terminate", ["returned": "terminateCancel", "reason": "quit-already-in-progress"])
            return .terminateCancel
        }
        quitting = true
        quitReplied = false
        quitAnyVeto = false
        quitQueue = wins.filter { !$0.tombstoned }
        if cfg.probesOnTerminate { startProbes(tag: "terminateLater") }
        CFRunLoopPerformBlock(CFRunLoopGetMain(), CFRunLoopMode.commonModes.rawValue) { self.quitNext() }
        CFRunLoopWakeUp(CFRunLoopGetMain())
        gLog.ev("should-terminate", ["returned": "terminateLater"])
        return .terminateLater
    }

    func quitNext() {
        if quitQueue.isEmpty { quitReply(!quitAnyVeto); return }
        let w = quitQueue.removeFirst()
        w.onOutcome = { outcome in
            if outcome == "veto" {
                self.quitAnyVeto = true
                if self.cfg.quitPolicy == "stop-at-first-veto" {
                    gLog.ev("quit-serial-stop", ["win": w.id, "remaining": self.quitQueue.map { $0.id }])
                    self.quitQueue.removeAll()
                }
            }
            CFRunLoopPerformBlock(CFRunLoopGetMain(), CFRunLoopMode.commonModes.rawValue) { self.quitNext() }
            CFRunLoopWakeUp(CFRunLoopGetMain())
        }
        gLog.ev("close-request", ["win": w.id, "source": "quit-serializer:performClose"])
        w.ns.performClose(nil)
    }

    func quitReply(_ v: Bool) {
        quitReplied = true
        gLog.ev("reply-to-should-terminate", ["value": v, "api": "replyToApplicationShouldTerminate:"])
        if !v {
            quitting = false
            gLog.ev("quit-cancelled", ["windows": wins.map { ["win": $0.id, "visible": $0.ns.isVisible, "tombstoned": $0.tombstoned] }])
        }
        NSApp.reply(toApplicationShouldTerminate: v)
        gLog.ev("reply-returned", ["value": v])
        scheduleIdle()
    }

    func applicationWillTerminate(_ n: Notification) {
        gWillQuitSeen = true
        gLog.ev("will-quit", ["api": "applicationWillTerminate:"])
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { false }

    // MARK: re-entrancy probes
    @objc func probeSel(_ name: NSString) { probe(name as String) }

    func probe(_ name: String) {
        probesPending -= 1
        gLog.ev("probe", ["probe": name, "in_dialog": openDialogs > 0, "in_terminate_later": quitting && !quitReplied])
        scheduleIdle()
    }

    func startProbes(tag: String) {
        let names = ["timer-defaultMode", "timer-commonModes", "timer-modalPanelMode", "perform-afterDelay(default)",
                     "gcd-main-asyncAfter", "bg->gcd-main-async", "bg->performSelectorOnMainThread(common)",
                     "bg->CFRunLoopPerformBlock(default)", "bg->CFRunLoopPerformBlock(common)",
                     "bg->CFRunLoopPerformBlock(modalPanel)"]
        probesPending += names.count
        gLog.ev("probes-armed", ["tag": tag, "count": names.count, "probe_delay_ms": 100])
        timer(100, mode: .default) { self.probe("timer-defaultMode") }
        timer(100, mode: .common) { self.probe("timer-commonModes") }
        timer(100, mode: .modalPanel) { self.probe("timer-modalPanelMode") }
        self.perform(#selector(probeSel(_:)), with: "perform-afterDelay(default)" as NSString, afterDelay: 0.1)
        DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(100)) { self.probe("gcd-main-asyncAfter") }
        roleQueue.asyncAfter(deadline: .now() + .milliseconds(100)) {
            gLog.ev("probe-bg-sender-runs", ["tag": tag])
            DispatchQueue.main.async { self.probe("bg->gcd-main-async") }
            let box = Box { self.probe("bg->performSelectorOnMainThread(common)") }
            box.performSelector(onMainThread: #selector(Box.fire), with: nil, waitUntilDone: false)
            deliver(.cfrlDefault) { self.probe("bg->CFRunLoopPerformBlock(default)") }
            deliver(.cfrlCommon) { self.probe("bg->CFRunLoopPerformBlock(common)") }
            deliver(.cfrlModal) { self.probe("bg->CFRunLoopPerformBlock(modalPanel)") }
        }
    }
}

// MARK: - Scenarios

func config(for s: String) -> Config {
    var c = Config()
    switch s {
    case "s1_close_veto_then_allow":
        c.steps = ["close:W1", "close:W2"]
    case "s2a_quit_two_dirty_allow":
        c.windows = [("W1", true), ("W2", true)]
        c.steps = ["quit"]
    case "s2b_quit_second_vetoes":
        c.windows = [("W1", true), ("W2", true)]
        c.dialogChoice = ["W1": 1, "W2": 0] // W1 Don't Save, W2 Cancel
        c.steps = ["quit"]
    case "s2c_quit_first_vetoes_stop":
        c.windows = [("W1", true), ("W2", true)]
        c.dialogChoice = ["W1": 0, "W2": 1]
        c.steps = ["quit"]
    case "s2d_quit_first_vetoes_continue":
        c.windows = [("W1", true), ("W2", true)]
        c.dialogChoice = ["W1": 0, "W2": 1]
        c.quitPolicy = "continue"
        c.steps = ["quit"]
    case "s3a_reentrancy_modal_from_gcd_block":
        c.roleDelivery = .gcdMain
        c.defaultDismiss = Dismiss(method: "abort", button: 0, delayMs: 500)
        c.probesOnDialog = true
        c.steps = ["close:W1"]
    case "s3b_reentrancy_modal_from_runloop_block":
        c.roleDelivery = .cfrlCommon
        c.defaultDismiss = Dismiss(method: "abort", button: 0, delayMs: 500)
        c.probesOnDialog = true
        c.steps = ["close:W1"]
    case "s3c_reentrancy_terminate_later":
        c.windows = [("W1", false), ("W2", false)]
        c.roleDelayMs = 400
        c.probesOnTerminate = true
        c.steps = ["quit"]
    case "s3d_terminate_later_default_mode_delivery":
        c.windows = [("W1", false), ("W2", false)]
        c.roleDelivery = .cfrlDefault
        c.watchdogMs = 3000
        c.steps = ["quit"]
    case "n1_sync_yes_while_dirty":
        c.hook = .syncYes
        c.steps = ["close:W1"]
    case "n2_timeout_autoclose":
        c.closeTimeoutMs = 300
        c.closeRoleDelayOverrideMs = ["W1": 1500]
        c.steps = ["close:W1"]
    case "p3_second_close_during_modal_gated":
        c.secondCloseDuringModalMs = 150
        c.defaultDismiss = Dismiss(method: "click", button: 1, delayMs: 500)
        c.steps = ["close:W1"]
    case "n3_second_close_during_modal_ungated":
        c.hook = .asyncUngated
        c.secondCloseDuringModalMs = 150
        c.dismiss = [1: Dismiss(method: "abort", button: 0, delayMs: 1000), 2: Dismiss(method: "abort", button: 0, delayMs: 200)]
        c.steps = ["close:W1"]
    default:
        fatalError("unknown scenario \(s)")
    }
    return c
}

let args = CommandLine.arguments
guard args.count == 4, let runIdx = Int(args[2]) else {
    fputs("usage: harness <scenario> <run> <out.jsonl>\n", stderr)
    exit(64)
}
gLog = Log(path: args[3], scenario: args[1], run: runIdx)
atexit {
    gLog.exiting = true
    if gWillQuitSeen {
        gLog.ev("quit", ["api": "atexit after applicationWillTerminate: (NSApp terminate path)"])
    } else {
        gLog.ev("process-exit", ["note": "harness exit, not an app quit"])
    }
}
let host = Host(cfg: config(for: args[1]))
let app = NSApplication.shared
app.setActivationPolicy(.regular)
app.delegate = host
app.run()
