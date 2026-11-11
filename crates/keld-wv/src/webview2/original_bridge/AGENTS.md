# Original Windows bridge
Owner/load: webview-windows-original-bridge/always.
Extends ../AGENTS.md. GH1184; keep the parent native-boundary invariants.

- post.rs only: creating-STA ICoreWebView2::PostWebMessageAsString on the retained view, with bounded immutable NUL-terminated UTF16 and the same Core CONTROL reservation. Reuse the parent's bounded Source/identity helpers before the final exact Core role/document/epoch fence; release guards, then enter the retained API without intervening native work, allocation or wait.
- Only the fixed ready:false carrier accepts RouteRetirementPermit. Native return is not the renderer's consumer receipt; an entered or outcome-unknown operation cannot become a never-written cancellation.
