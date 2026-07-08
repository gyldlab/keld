# keld-guard — adds root AGENTS.md

Spec: `docs/architecture/03-security.md`. Security boundary; threat model in crate docs.

- Default-deny: unknown cap/channel/scope, missing manifest → `Deny`. No interim allow.
- Principals host-minted, unforgeable; webview principals rotate on navigation.
- Every `DenyReason`: capability/scope + fix (manifest edit). Deny text is API — test it.
- Scope matching: resolve `$VARS`, symlinks, `..` before match. Bypass fixtures (traversal, symlink swap, case folding, wildcard-swallow) permanent. New matcher → adversarial tests.
- Wildcard grants loud in `keld doctor`. No dev-mode special-case inside engine — profile composed outside, refused in release.
- Hot path (per kipc frame): no alloc on `Allow`, no locks across handler dispatch.
