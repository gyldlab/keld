# Keld Security Model — Default Deny, Generated, Auditable

Position in the field: Electron = opt-in checklist (forget one flag → RCE). Tauri =
right model (permissions/scopes/capabilities), hostile DX (hand-written JSON, wildcard
culture). Electrobun = no model. Deno Desktop = compile-time flags, no runtime
enforcement, no per-window separation. Keld's job: Tauri's rigor with zero-config feel.

## 1. Principals and trust

| Principal | Trust | Authority |
|---|---|---|
| keld-host | trusted | everything; enforces all policy |
| app process (Bun) | semi-trusted (developer code, still fenced) | only what the manifest grants |
| webview (per window × per origin) | untrusted | only what its capability block grants |
| native plugin (Rust, in-host) | trusted-by-review | declared capabilities, checked at registration |

Every kipc frame carries the sender's principal id (minted by the host, unforgeable —
peers never self-identify). The guard (`keld-guard`) evaluates
`(principal, channel, args) → allow | deny(reason)` before any handler runs.
**Destination:** host-minted principal on every frame; guard-before-handler for
every privileged call.
**v0:** `FrameHeader` is `{kind, flags, channel, corr, len}` — there is no
principal field on the wire. `keld-guard::evaluate` takes a `Principal` and
default-denies anything other than `AppProcess` (`KELD-GUARD006`) so `app`
scopes cannot be applied to a webview or plugin by accident. Channel grants
are not evaluated. Echo dispatch does not call the guard.

## 2. The manifest: `keld.permissions.jsonc`

One file. Reviewed like a lockfile. Wildcards allowed but linted loudly.

```jsonc
{
  "$schema": "https://keld.dev/schemas/permissions-1.json",
  "app": {                                 // grants for the Bun app process
    "fs":      { "read": ["$APPDATA/**", "$DOCUMENTS/MyApp/**"], "write": ["$APPDATA/**"] },
    "net":     { "connect": ["https://api.myapp.com", "wss://sync.myapp.com"] },
    "shell":   { "open": ["https://*"], "spawn": [{ "cmd": "$RESOURCES/bin/ffmpeg", "args": "reviewed" }] },
    "system":  ["clipboard.read", "clipboard.write", "notifications", "tray", "global-shortcuts"],
    "secrets": ["keychain:myapp/*"]
  },
  "windows": {
    "main":  { "channels": ["notes.*", "el:*"], "web": { "csp": "default" } },
    "about": { "channels": [], "web": { "csp": "static-only" } }
  },
  "audit": { "log": "on-deny" }            // or "all" → structured audit trail
}
```

- **Path scopes** use the vetted-pattern matcher in the host (no regex injection).
  **Destination:** `$VARS` resolved by the host, then symlink/`..` traversal
  normalized after resolution — the classic scope-bypass bugs are test fixtures.
  **v0:** `$VARS` match as literals; `..` is rejected; symlink canonicalization is
  not in this slice.
- **Channel grants** connect to the schema layer: a channel's declared capability set
  (from `.k.ts` contracts) must be ⊆ the caller's grants.
- **CSP injection** by default on every webview (`default` = self + keld:// + declared
  net hosts; `static-only` = no script eval, no net). Opt-out is a named, linted grant.

## 3. Zero-config feel: the manifest is generated, not hand-written

- `keld dev` runs with a **dev-permissive profile + recorder**: every would-be denial
  is allowed but recorded with a stack.
- `keld doctor --permissions` (and `keld build`) is **planned**: diffs recorded
  usage against the manifest and prints exact JSON patches ("your app called
  fs.read('~/Library/…'): add `$APPDATA/**` or change the call"). v0 `keld doctor`
  rejects unknown flags (`KELD-CLI-044`, exit 2); `--permissions` is not live.
- `keld migrate` seeds the manifest from static analysis of Electron API usage
  (dialog → fs read of chosen paths, autoUpdater → net to feed URL, etc.).
- CI mode (`keld build --frozen-permissions`) fails on any manifest drift — the
  lockfile discipline, applied to authority.

## 4. Enforcement mechanics (defense in depth)

1. **Broker pattern (always on)**: the only code with OS authority is the host. The app
   process gets no direct fs/net/shell APIs from Keld — `@keld/api` calls are kipc
   calls into guarded host handlers. (Bun itself can still `fs.read` — see layer 2.)
2. **OS sandbox on the app process (progressive)**: because *authority already lives in
   the host*, we can clamp the Bun child without breaking the model:
   - macOS: `sandbox_init` profile (deny fs-write outside app containers, deny net when
     manifest has none) — v0.3 target;
   - Windows: restricted token + job object; AppContainer as stretch;
   - Linux: landlock + seccomp basic profile.
   Escape hatch per app (`"appSandbox": "off"`) for native modules that need raw access,
   loudly surfaced in `keld doctor`. Electron-compat apps start with sandbox off +
   roadmap to tighten (compat first, then squeeze).
3. **Webview hardening (always on)**: CSP injection, `keld://` scheme is
   fetch-isolated per principal, remote-content windows get `channels: []` unless
   granted, navigation policy hooks (allow-list), devtools off in release unless
   `web.devtools: true`.
   **v0:** camera and microphone requests are default-deny on all three live
   backends: macOS and Linux (KEL-28) both install wry `with_permission_handler`
   (`with_guarded_media_permissions`, shared helper); Windows registers the
   `WebView2` `add_PermissionRequested` handler before the first navigation
   (KEL-65 direct COM — the ordering is compile-enforced). All three call
   `keld-guard::evaluate` as `Principal::AppProcess` on `web.camera` /
   `web.microphone` with requested resource `*` (no platform callback passes
   an origin or a webview principal). Grant with
   `"web": { "camera": ["*"] }` / `"microphone": ["*"]`. CSP injection,
   `keld://` isolation, navigation allow-lists, remote-content `channels: []`,
   and `web.devtools` are not in this slice.
4. **Supply chain**: CLI adopts a 24 h `min-release-age` for template deps (Deno 2.9
   lesson); host binaries + updates are ed25519-signed with a TUF-style rotating root;
   `keld.lock` pins host/Bun/polyfill-pack versions.

## 5. Update security

Update manifests signed (ed25519, key in `keld.build.ts` → CI secret); patches carry
full-file BLAKE3 post-conditions (a bad/malicious diff cannot produce an unverified
binary); rollback keeps N-1 with the same verification; channel pinning
(stable/beta/canary) in the manifest. Threat model documented in `keld-update` crate docs.

## 6. What we deliberately do NOT promise (honesty ledger)

- The app process is the developer's own code; the sandbox protects the *user* from
  supply-chain compromise of that code, not the developer from themselves.
- Webview engine CVEs are the platform's (system engines) or ours to re-ship (pinned
  engines — pinned-engine apps inherit an Electron-style update duty; `keld doctor`
  nags on stale pins).
- No DRM/anti-tamper claims. Signed updates ≠ secure boot.
