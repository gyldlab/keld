# Original Windows bridge
Owner/load: webview-windows-original-bridge/always.
Extends ../AGENTS.md; GH1184.

- Only `installer.rs` may call `ICoreWebView2::AddScriptToExecuteOnDocumentCreated` on the retained creating-STA view and implement its raw completion handler. Keep counted, NUL-terminated UTF-16 through call return and callback or same-controller Close. Never dispose the input in-call.
- The completion handler MUST check HRESULT and non-null script ID without scanning or retaining the ID. Post only the existing wake; no nested pump or unwind may cross the ABI.
- Navigation requires successful native return and timely successful completion. A late, retired, or failed registration MUST NOT become ready.
