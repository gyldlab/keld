# Windows desktop facts native feasibility probe

Test-only, standalone Rust binary for GH1186. It is outside the KELD Cargo
workspace and does not enable a product getter, add a mirror, change Ready,
or create an HWND (a native Windows window).

## Build and run

Use the repository-pinned Rust toolchain on Windows. Dependencies reuse
the repository pins: windows-sys 0.61.2 and serde_json 1.0.150.
Use a dedicated target directory, never an existing agent's build directory.

```powershell
cargo build --manifest-path tools/windows_desktop_facts/native/Cargo.toml --locked --offline --target-dir D:/WORK/keld/.keld-work/build/gh1186-w4-native-20261105
cargo test --manifest-path tools/windows_desktop_facts/native/Cargo.toml --locked --offline --target-dir D:/WORK/keld/.keld-work/build/gh1186-w4-native-20261105
```

The five unit tests are pure: they make no Windows calls. **Every executable
invocation below samples native desktop state and needs the serialized Ramani
native-test slot**, including the negative controls.

```powershell
& $probe --mode pmv2
& $probe --mode unaware
& $probe --mode pmv2 --control invalid-monitor
& $probe --mode pmv2 --control failed-dpi
& $probe --mode pmv2 --control high-contrast-size
```

Here, `$probe` is the resulting `debug/keld-windows-desktop-facts-probe.exe`.
Stdout contains exactly one JSON object followed by a newline. Success exits
0. Native failure or invalid input exits 1. A control that unexpectedly
succeeds exits 3. Errors have no `sample` field. A runner must inspect
`error.kind` and `error.source`, not only the nonzero exit.

## Evidence schema (version 1)

Top level: `schemaVersion`, `probe`, `status`, `processId`, `request`
(`mode`, `control`), `machine`, `dpiContext`, `cleanup`, and either
`sample` or `error`.

- `sample.desktop.displays[]`: `sourceName`, diagnostic
  `observedHandleHex`, `primary`, `bounds`, `workArea`, `dpi`,
  `physicalMode`, and all matching `nativePaths`.
- Rectangle fields are `x`, `y`, `width`, `height`.
  `coordinateSpace` declares physical pixels for exact PMv2, or
  DPI-virtualized caller units for the unaware child. The independent
  `physicalMode.width/height` always comes from EnumDisplaySettingsW.
- `dpi.x/y` are raw effective DPI from GetDpiForMonitor.
  `dpi.deviceScaleFactor` is precisely DPI / 96; it does **not** include
  UISettings.TextScaleFactor and does not claim Electron scale parity.
- A native path retains source and target adapter LUID low/high parts,
  source/target IDs, target device path, friendly name, availability,
  output technology and flags. There is no invented numeric display ID.
  HMONITOR and GDI source names are not stable identity promises.
- `primaryWorkAreaViaSpi` independently reads SPI_GETWORKAREA.
  Exact PMv2 bounds must agree with physical mode dimensions and primary
  work area must agree with the SPI read, or the sample fails.
- `sample.theme` retains registry status/value for AppsUseLightTheme and
  SystemUsesLightTheme, HIGHCONTRAST flags/active, and COLOR_WINDOW /
  COLOR_WINDOWTEXT as raw COLORREF plus RGB channels.
  `nonForcedColorScheme` is null when high contrast is on or the app
  preference is unavailable. Forced-color Electron theme/contrast policy
  is deliberately not guessed.
- `error` contains `kind`, `source`, numeric `status` (or null), and
  `detail`. A failed BOOL call can legitimately leave last-error zero.

## Failure-first controls and cleanup

`invalid-monitor` calls the same GetMonitorInfoW wrapper with a null
monitor. `failed-dpi` enumerates an actual monitor and gives
GetDpiForMonitor an invalid DPI enum. `high-contrast-size` uses the same
SPI_GETHIGHCONTRAST wrapper with cbSize zero while retaining a full writable
structure. None accepts a substituted default fact after native failure.
Expected error sources are respectively `GetMonitorInfoW`,
`GetDpiForMonitor`, and `SystemParametersInfoW(SPI_GETHIGHCONTRAST)`.

The probe owns no windows or monitor handles and installs no observer.
It only changes the DPI awareness of its own new process. It frees any
system-allocated HIGHCONTRAST scheme buffer with LocalFree before output.
Registry queries use borrowed predefined keys. It never writes display,
theme, registry or accessibility settings.

## Limits and authoritative references

This one-shot probe cannot establish OS event delivery, multi-monitor
Chromium layout parity, Electron numeric identity parity, or cross-reboot
identity stability. Clone paths are retained rather than collapsed.
QueryDisplayConfig size races retry at most four times; any other failure
or inconsistent cross-check is an explicit failed sample. Its caller owns
subscription ordering, generation rejection and replay tests.

- [QueryDisplayConfig](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-querydisplayconfig)
- [DisplayConfigGetDeviceInfo](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-displayconfiggetdeviceinfo)
- [EnumDisplaySettingsW physical pixel semantics](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-enumdisplaysettingsw)
- [GetDpiForMonitor caller-awareness caveat](https://learn.microsoft.com/en-us/windows/win32/api/shellscalingapi/nf-shellscalingapi-getdpiformonitor)
- [Exact DPI context equality](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-aredpiawarenesscontextsequal)
- [HMONITOR validity](https://learn.microsoft.com/en-us/windows/win32/gdi/hmonitor-and-the-device-context)
- [HIGHCONTRASTW and buffer ownership](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-highcontrastw)
- [GetSysColor and valid black](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getsyscolor)