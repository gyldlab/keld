// GH-1081 round-1 read-only census (research only): every WebKit XPC service
// process with its kernel image path and launchd responsible PID. WebKit's
// helpers are launchd children, so responsibility, not ppid, binds them to the
// host. Prints one JSON line; never signals or attaches to any process.
import Darwin
import Foundation

typealias Responsible = @convention(c) (Int32) -> Int32
// RTLD_DEFAULT is ((void *)-2) in <dlfcn.h>; Swift does not import the macro.
guard let symbol = dlsym(UnsafeMutableRawPointer(bitPattern: -2), "responsibility_get_pid_responsible_for_pid") else {
  print("WEBKIT_CENSUS_UNAVAILABLE responsibility symbol missing")
  exit(3)
}
let responsible = unsafeBitCast(symbol, to: Responsible.self)
let capacity = Int(proc_listallpids(nil, 0)) + 256
var pids = [Int32](repeating: 0, count: capacity)
let filled = pids.withUnsafeMutableBufferPointer { buffer in
  proc_listallpids(buffer.baseAddress, Int32(buffer.count * MemoryLayout<Int32>.size))
}
guard filled > 0 else {
  print("WEBKIT_CENSUS_UNAVAILABLE proc_listallpids=\(filled)")
  exit(3)
}
var rows: [[String: Any]] = []
for pid in pids.prefix(Int(filled)) where pid > 0 {
  var path = [CChar](repeating: 0, count: 4096)
  guard proc_pidpath(pid, &path, UInt32(path.count)) > 0 else { continue }
  let image = String(cString: path)
  guard let service = image.split(separator: "/").last, service.hasPrefix("com.apple.WebKit.") else { continue }
  rows.append(["pid": Int(pid), "responsible": Int(responsible(pid)), "service": String(service)])
}
let data = try JSONSerialization.data(withJSONObject: ["rows": rows], options: [.sortedKeys])
print("WEBKIT_CENSUS " + String(data: data, encoding: .utf8)!)
