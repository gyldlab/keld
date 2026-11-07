# W4 Windows geometry projection — bounded source-executed evidence

Status: **CI-only arithmetic proof, not native Windows, source integration or W4 completion.** No application/Host/Core/window/GUI/native API was launched for these tests. No boundary change.

## Source authority and independence

The owned function is `crates/keld-wv/src/webview2/desktop_sample/display.rs::logical` from the retained isolated OneFive/Ramani W4 Windows test tree at `88eac7bca0ee0994f2815b09cadb82d5fe906299`. On November 7, 2026, the actual Windows file's complete SHA-256 was read back as:

- `display.rs`: `aee0b6aa5aaba105eab061fd873513f132d25629d5cf11aba3c58643cfa1e7cd`
- Verbatim final `logical` function (1,417 UTF-8 bytes): `6801ee1096cfb537b9ba30d47e738f8dac4e5ef19bfbb36e15589bbc31012dd6`
- Reviewed complete executable fixture with independent test oracles: `4bed36bc789db0e50bf2e74449609aa09ed86ceaaba9be564d4085f1f4b27d5a` (also checked by the driver before compilation).

The complete Windows source was copied **byte-for-byte** to private evidence (4,706 bytes, full SHA matches). The test scaffold supplies only a minimal equivalent `AppDesktopRect` type and `WvError` wrapper, not a replacement geometry algorithm. `#[rustfmt::skip]` protects the verbatim function's source bytes; formatting applies to the surrounding test scaffold. The test runner refuses if either the whole Windows source hash or the function body differs. The pinned Windows file ends with this function. Source changes require an explicit new review and pin, not an automatic suffix match. Private Windows ancestry was not pushed.

Independent expected results come from the actual recorded native physical monitor `1920×1080`, work area `1920×1020`, and DPI120/96 = 1.25, corroborated by the original Electron observation. Expected logical results are `1536×864` and `1536×816`. Non-divisible input edges `[1,2,1919,1019]` at 1.25 must enclose all physical pixels: logical `x0,y1,width1536,height815`. Invalid scale and zero/reversed width/height must refuse, not publish a zero-size usable display.

## Actual scoped execution and negative controls

On macOS/aarch64 using Rust 1.99.1, compiled with `rustc --edition=2024 -D warnings --test`:

- Exact copied source with a verified live-Windows-file snapshot: **4 PASS / 0 FAIL**.
- Only `ceil` for upper edges replaced with `floor`: compiled successfully; the exact fractional-bound test **intentionally FAILED** on a wrong rounded rectangle (semantic assertion, Rust test exit 101).
- Only positive-width filter removed: compiled successfully; the zero-width refusal test **intentionally FAILED** because a zero-width display was admitted (semantic assertion, exit 101).
- Byte-exact original restored: **4 PASS / 0 FAIL**.
- Tampered full Windows `display.rs`, tampered extracted function, separately mutated test-only oracle, and reused evidence directory: all four **REFUSED before compilation** (nonzero exits).

Retained raw source copies, each compiler/test log, mutation snapshots and machine-readable manifest live only in private `.git/keld-closeout/w4-native-geometry-20261107-1100/`. The current source-and-oracle-pinned run is `campaign-oracle-pin-03/manifest.json`; earlier exploratory `campaign-01` (no full-source input), `campaign-source-bound` and `campaign-strict-02` are retained, not substituted for the final run. The one-file executable test and Python verification driver are the only new source artifacts.

To reproduce against a fresh verified Windows source copy (with an empty new evidence destination):

```sh
python3 tools/windows_desktop_facts/verify_production_geometry.py \
  --windows-source /path/to/verified/display.rs \
  --evidence-dir /path/to/NEW-evidence-directory
```

The runner refuses changed or missing pinned bytes and never overwrites an existing evidence directory. It writes all mutation source copies, compiler/test output and the result manifest under that directory. The negative controls are intentionally temporary copies; they do not edit shipping source or the reviewed excerpt.

## Exact remaining boundaries

This passes **only pure logical geometry projection and refusal arithmetic**. It does not qualify Win32 `GetMonitorInfoW`, actual DPI/awareness and window enumeration, DisplayConfig device identity, WinRT text scale, HighContrast/registry appearance, native failed-acquisition semantics, native cleanup, post-Init event collection, Core reader admission, public Ready, original draw.io first window or recovery.

GH481's exact bounded Windows S1 proposal still needs an authorized contract-owner acceptance decision. W2/W5 must explicitly grant the OneFive/Ramani native/build lane for the separately staged `desktop_capture_refusal_preserves_identity_history` test, its retained-history fault control, restoration and raw native resource evidence. GH1061's protected-tools CI path remains pending. PR1401 is OPEN/DRAFT; **do not merge or enable auto-merge**. D13/GH446 is already approved and unchanged.
