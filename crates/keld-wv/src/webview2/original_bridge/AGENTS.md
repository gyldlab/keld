# Original Windows bridge
Owner/load: webview-windows-original-bridge/always.
Extends ../AGENTS.md; GH1184. Keep parent invariants.

- post/entry.rs alone: retained STA PostWebMessageAsString. Immutable bounded UTF16 stays on its existing CONTROL/DATA/call charge. Exact Core role/document/epoch fence; drop guards, then immediate API: no allocation, native work or wait. Only fixed ready:false accepts RouteRetirementPermit. Return is not consumption; entered/unknown is never never-written.
- callbacks.rs: attached WebMessageReceived/DOMContentLoaded tokens, actual NavigationId; retain through same Close and callback exit. capture.rs: actual Source/WebMessageAsJson via bounded task-string owner; prepay controlled copies, decode full outer String then canonical envelope. No guard across native entry.
- installer.rs: STA AddScript/raw completion; counted UTF16 through return+callback or same Close, never in-call disposal. No nested pump. ACK gates navigation; late/retired completion cannot promote.
