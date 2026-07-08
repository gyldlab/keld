# keld-wv — adds root AGENTS.md

Spec: `docs/architecture/05-webview-and-native.md`. Platform truth: `docs/research/06-webview-reality.md`.

- `unsafe` allowed (platform backends only): `deny(unsafe_op_in_unsafe_fn)`, `// SAFETY:` citing platform contract.
- All engine/window mutations on UI thread via command queue. Never platform handles from I/O/pool threads.
- `WebEngine` trait changes = design review. Backends stay within trait API.
- Platform quirks: comment OS + version + source link; uncited workarounds reverted.
- Linux: probe GPU stack, apply safe-mode before init — never instruct env-var exports. Emit `degraded-rendering`.
- Cross-engine diffs → baseline matrix; polyfill pack + doctor smooth, not silent paper-over.
