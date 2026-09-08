import CoreGraphics
import Darwin
import Foundation

// Query and prearmed observation share this exact PID/title/layer filter.
func matchingWindows(_ rows: [[String: Any]], title: String) -> [Int: [UInt32]] {
  var matches: [Int: [UInt32]] = [:]
  for row in rows {
    guard let owner = (row[kCGWindowOwnerPID as String] as? NSNumber)?.intValue,
      row[kCGWindowName as String] as? String == title,
      (row[kCGWindowLayer as String] as? NSNumber)?.intValue == 0
    else { continue }
    let number = (row[kCGWindowNumber as String] as! NSNumber).uint32Value
    matches[owner, default: []].append(number)
  }
  return matches
}

func census(_ scope: String, title: String) -> [Int: [UInt32]] {
  let options: CGWindowListOption
  if scope == "on-screen" {
    options = [.optionOnScreenOnly, .excludeDesktopElements]
  } else if scope == "all" {
    options = [.excludeDesktopElements]
  } else {
    fatalError("unknown native-window scope")
  }
  let rows = CGWindowListCopyWindowInfo(options, kCGNullWindowID) as! [[String: Any]]
  return matchingWindows(rows, title: title)
}

enum HistoryResult: Equatable {
  case pending
  case live([UInt32])
  case stale
}

struct PresentationHistory {
  // One first valid snapshot per PID; never a union of windows from different
  // samples, nor a replacement of a previously witnessed window identity.
  var first: [Int: [UInt32]] = [:]
  let count: Int

  mutating func observe(_ onScreen: [Int: [UInt32]]) {
    for (pid, ids) in onScreen where ids.count == count && first[pid] == nil {
      precondition(first.count < 256, "native-window PID history exceeded its bound")
      first[pid] = ids.sorted()
    }
  }

  func result(pid: Int, allWindows: [Int: [UInt32]]) -> HistoryResult {
    guard let observed = first[pid] else { return .pending }
    return observed == (allWindows[pid] ?? []).sorted() ? .live(observed) : .stale
  }
}

func query() {
  let pid = Int(CommandLine.arguments[2])!
  let title = CommandLine.arguments[3]
  let expectation = CommandLine.arguments[4]
  let deadline = Date().addingTimeInterval(Double(CommandLine.arguments[5])!)
  let scope = CommandLine.arguments[6]
  var seen = Set<UInt32>()
  while true {
    let found = census(scope, title: title)[pid] ?? []
    seen.formUnion(found)
    let matches: Bool
    if expectation == "snapshot" {
      matches = true
    } else if expectation.hasPrefix("exact:") {
      let ids = expectation.dropFirst("exact:".count).split(separator: ",").map { UInt32($0)! }.sorted()
      matches = found.sorted() == ids
    } else {
      fatalError("unknown native-window expectation")
    }
    if matches {
      for window in found { print(window) }
      return
    }
    if Date() >= deadline {
      fputs("native-window observed_ids=\(seen.sorted())\n", stderr)
      exit(3)
    }
    sched_yield()
  }
}

func observe() {
  let title = CommandLine.arguments[2]
  let count = Int(CommandLine.arguments[3])!
  let timeout = Double(CommandLine.arguments[4])!
  precondition(count == 1, "initial presentation requires exactly one window")
  let flags = fcntl(STDIN_FILENO, F_GETFL)
  precondition(flags >= 0 && fcntl(STDIN_FILENO, F_SETFL, flags | O_NONBLOCK) == 0)
  var history = PresentationHistory(count: count)
  history.observe(census("on-screen", title: title))
  print("READY")
  fflush(stdout)
  var command: [UInt8] = []
  var input = [UInt8](repeating: 0, count: 32)
  var target: Int?
  var deadline: UInt64?
  while true {
    // stdin EOF is parent loss. It also bounds observer lifetime if the Rust
    // owner exits without unwinding; no target process or UI is controlled.
    let readCount = read(STDIN_FILENO, &input, input.count)
    if readCount == 0 { exit(2) }
    if readCount > 0 {
      precondition(target == nil && command.count + readCount <= 32, "invalid observer command")
      command.append(contentsOf: input.prefix(readCount))
      if command.last == 10 {
        guard let text = String(bytes: command.dropLast(), encoding: .utf8),
          let pid = Int(text), pid > 0 else { fatalError("invalid observer PID") }
        target = pid
        // Collection before binding does not spend the assertion's deadline.
        deadline = DispatchTime.now().uptimeNanoseconds + UInt64(timeout * 1_000_000_000)
      }
    } else if errno != EAGAIN && errno != EINTR {
      fatalError("read observer command failed")
    }
    history.observe(census("on-screen", title: title))
    if let pid = target {
      switch history.result(pid: pid, allWindows: census("all", title: title)) {
      case .live(let ids):
        print("WINDOWS " + ids.map(String.init).joined(separator: ","))
        return
      case .stale:
        fputs("native-window observed identity no longer matches live census\n", stderr)
        exit(3)
      case .pending:
        break
      }
      if DispatchTime.now().uptimeNanoseconds >= deadline! {
        fputs("native-window target was never observed on-screen\n", stderr)
        exit(3)
      }
    }
    sched_yield()
  }
}

// Fixed snapshots prove history/identity logic only. Real product tests below
// the Rust owner retain the independent CoreGraphics on-screen oracle.
func historyRegressions() {
  let pid = 42
  let other = 43
  let id: UInt32 = 7136
  var history = PresentationHistory(count: 1)
  precondition(history.result(pid: pid, allWindows: [pid: [id]]) == .pending,
    "all-window existence must not fabricate presentation")
  history.observe([other: [id]])
  precondition(history.result(pid: pid, allWindows: [pid: [id]]) == .pending,
    "another PID must not supply presentation")
  history.observe([pid: [id, id + 1]])
  precondition(history.result(pid: pid, allWindows: [pid: [id]]) == .pending,
    "two-window snapshot must not satisfy count one")
  history.observe([pid: [id]])
  history.observe([:]) // A Space switch removes the on-screen row.
  precondition(history.result(pid: pid, allWindows: [pid: [id]]) == .live([id]),
    "late consumption must preserve an actually observed live identity")
  precondition(history.result(pid: pid, allWindows: [:]) == .stale,
    "destroyed window must fail")
  precondition(history.result(pid: pid, allWindows: [pid: [id + 1]]) == .stale,
    "replacement window must fail")
  precondition(history.result(pid: pid, allWindows: [pid: [id, id + 1]]) == .stale,
    "additional matching window must fail exact count")
  history.observe([pid: [id + 1]])
  precondition(history.result(pid: pid, allWindows: [pid: [id + 1]]) == .stale,
    "later presentation must not overwrite stale first identity")
  let row: [String: Any] = [kCGWindowOwnerPID as String: pid,
    kCGWindowName as String: "fixture", kCGWindowLayer as String: 0,
    kCGWindowNumber as String: id]
  precondition(matchingWindows([row], title: "fixture") == [pid: [id]])
  precondition(matchingWindows([row], title: "wrong").isEmpty)
  var elevated = row
  elevated[kCGWindowLayer as String] = 1
  precondition(matchingWindows([elevated], title: "fixture").isEmpty)
  print("PASS native-window history: unseen, PID, count, Space, destroyed, replaced, extra, first identity, title, layer")
}

switch CommandLine.arguments[1] {
case "query": query()
case "observe": observe()
case "history-regressions": historyRegressions()
default: fatalError("unknown census mode")
}
