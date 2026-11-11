// Read-only availability check, not containment or qualification evidence.
import { dlopen, FFIType } from "bun:ffi";

if (process.platform !== "win32" ||
    process.env.SystemRoot?.replaceAll("\\", "/").toLowerCase() !== "c:/windows") {
  throw new Error("Expected the verified Windows system-root location");
}
const library = dlopen("C:\\Windows\\System32\\kernel32.dll", {
  GetCurrentProcessId: { args: [], returns: FFIType.u32 },
});
try {
  if (library.symbols.GetCurrentProcessId() !== process.pid) {
    throw new Error("Native process identity did not match this Bun process");
  }
  console.log("BUN_FFI_READONLY_PID_MATCH=true");
} finally {
  library.close();
}
