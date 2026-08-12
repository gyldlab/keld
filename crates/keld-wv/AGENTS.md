# keld-wv — adds root AGENTS.md

Spec: `docs/architecture/05-webview-and-native.md`. Platform truth: `docs/research/06-webview-reality.md`. v0 trait: `src/engine.rs` (not the full spec 05 sketch).

- `unsafe` MAY appear in platform backends only. Those modules MUST `deny(unsafe_op_in_unsafe_fn)` and MUST cite a platform contract in `// SAFETY:`.
- All engine/window mutations MUST run on the UI thread (today: tao's main-thread event loop; later: keld-core command queue). Agents MUST NOT touch platform handles from I/O or pool threads.
- `WebEngine` trait changes MUST go through design review. Backends MUST stay within the trait API. Agents MUST NOT add a trait method until a live backend implements it in the same PR (root YAGNI).
- Platform quirks MUST comment OS + version + source link; uncited workarounds MUST be reverted.
- Linux: agents MUST probe the GPU stack and apply safe-mode before init — MUST NOT instruct env-var exports. Emit `degraded-rendering`.
- Cross-engine diffs MUST go to the baseline matrix; polyfill pack + doctor smooth. Agents MUST NOT silently paper over them.
