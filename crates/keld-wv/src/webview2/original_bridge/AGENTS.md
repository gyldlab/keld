# Original Windows bridge
Owner/load: webview-windows-original-bridge/always. GH1184.
Extends ../AGENTS.md. Use exact retained creating-STA View.

- installer.rs alone: AddScriptToExecuteOnDocumentCreated/raw completion. Hold counted NUL UTF-16 until return plus callback/same Close. Check callback HRESULT/non-null ID without scan or retention. Post only OriginalWake. Both timely live successes gate navigation.
- callbacks.rs: top-view WebMessageReceived/DOMContentLoaded, QI/NavigationId. Verify sender identity. Hold token slots through add return, same Close and callback exit.
- capture.rs: actual Source/WebMessageAsJson via bounded task strings. Precharge copies. Decode full outer JSON String then canonical envelope.
- post/entry.rs alone: PostWebMessageAsString. Hold immutable charged NUL UTF-16 through return. Exact Core role/document/epoch fence enters Posting. Drop guards, call immediately. Only fixed ready:false uses RouteRetirementPermit. Delivery needs consumer receipt.

No nested pump or ABI unwind.
