# Windows dialogs
Owner/load: native-windows-dialog/always.
Extends native AGENTS. Independent unsafe review required.

- Core retains W2 parent on Host UI STA through Show; defer after WebView2 callbacks.
- Unsafe only:
  - mod.rs: checked HWND; GetWindowThreadProcessId/GetCurrentThreadId/GetCurrentProcessId, IsWindowVisible/IsWindowEnabled, CoGetApartmentType.
  - file.rs: CoCreateInstance(FileOpenDialog/FileSaveDialog); IFileDialog Get/SetOptions, SetFileTypes/SetFileTypeIndex/SetDefaultExtension/SetFileName/SetFolder/Show; SHCreateItemFromParsingName; GetResults/GetCount/GetItemAt/GetResult/GetDisplayName; owned UTF-16 copy, CoTaskMemFree once.
  - file_timer.rs: SetTimer/KillTimer, TIMERPROC, IFileDialog::Close; IOleWindow QI/GetWindow, GetWindowThreadProcessId/GetCurrentThreadId/GetCurrentProcessId/GetWindow(GW_OWNER), SendMessageW(WM_CLOSE) for current dialog only.
  - message.rs: TaskDialogIndirect/callback; SendMessageW(TDM_CLICK_BUTTON) for current dialog only.
- Local SAFETY. Retained TLS; no raw Rust pointer, borrow across reentry or escaping unwind. Retain through Show; remove timer/TLS before free. No Send/Sync, extra loop/thread, leak or second custody.
- tests.rs: CoInitializeEx/CoUninitialize; CreateWindowExW/DestroyWindow; GetCurrentThreadId/EnumThreadWindows/EnumChildWindows/GetWindow/GetDlgItem/GetClassNameW/GetWindowTextW/IsWindow/IsWindowVisible; SetTimer/KillTimer; SendMessageW(BM_CLICK), own windows only. verification.ps1: MTA UIA; own panel/PID; toggle/readback; reap before reuse.
