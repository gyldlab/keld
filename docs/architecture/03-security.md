# Keld Security Model — Default Deny, Generated, Auditable

Position in the field: Electron = opt-in checklist (forget one flag → RCE). Tauri =
right model (permissions/scopes/capabilities), hostile DX (hand-written JSON, wildcard
culture). Electrobun = no model. Deno Desktop = compile-time flags, no runtime
enforcement, no per-window separation. Keld's job: Tauri's rigor with zero-config feel.

## 1. Principals and trust

The three core principal classes are host, app-process role, and webview. Optional
native-addon workers, reviewed native plugins and the signed update relaunch helper have
their own policy; they do not inherit identity or grants from a core class.

| Principal | Trust | Authority |
|---|---|---|
| keld-host | trusted authority root | every framework-controlled privileged resource; enforces policy |
| app-process role (supervised Bun; one or more) | semi-trusted developer/extension code | only that host-minted role's grants |
| webview (host WebView id + navigation generation; origin is policy context) | untrusted | only what its capability block grants |
| native-addon worker (optional compat process) | untrusted native code | OS-sandboxed bounded profile; broker only, never host authority |
| native plugin (optional Rust, in-host) | trusted TCB code | manifest constrains registered channels; in-process code is not syscall-confined |
| update relaunch helper (optional signed process) | trusted narrow mechanism | verified staged artifact + exact install target only |

Every dispatched kipc frame is associated with sender identity that the host minted;
peers never self-identify. The destination supervisor binds a principal to the accepted
link and passes it as trusted metadata beside the decoded frame—it is not a
caller-controlled wire field. Webview identity is likewise derived from the host's
engine/navigation registry. The guard (`keld-guard`) evaluates
`(principal, channel, args) → allow | deny(reason)` before any handler runs.
**Destination:** host-minted principal in every dispatch context; guard-before-handler
for every privileged call.
**v0:** `FrameHeader` is `{kind, flags, channel, corr, len}` — there is no
principal field on the wire. KEL-70 supervises the generic child but does not bind its
accepted link to a role principal. `keld-guard::evaluate` takes a `Principal` and
default-denies anything other than `AppProcess` (`KELD-GUARD006`) so `app`
scopes cannot be applied to a webview or plugin by accident. Channel grants
are not evaluated. Echo dispatch does not call the guard — it is an
unprivileged demo channel (KEL-30), deliberately not routed through this.
`keld_ipc::guard_dispatch::dispatch_privileged` (KEL-69) is the sanctioned
guard-before-handler entry point for a privileged `Call`: it runs
`evaluate` and only invokes the handler closure on `Allow`, verified
end-to-end over a real kipc session (real socket, real `HELLO` handshake,
real filesystem side effect gated on the decision). Host `fs.read` /
`fs.write` (KEL-71) is the first production capability on that path.
Host-lifecycle `Quit` / `Ready` / `LastWindowClosed` (KEL-72,
`LIFECYCLE_CHANNEL`) are session control on an already-minted app-link, not
OS-authority handlers, and stay ungated like echo.

**Destination (KEL-75):** every supervised child role has a distinct principal and
app-link. A role does not inherit the primary app principal merely because the same
package spawned it. `keld.config.ts` declares the bundled entry and one lifecycle owner
(`primary`, `app-bound`, or `window-bound`); `keld.permissions.jsonc` declares a
generated role capability subset or an explicitly reviewed role-specific addition.
Neither a role name, PID, token, `KELD_APP_LINK`, caller payload nor Electron facade
option can select authority. The role schema is not live; the current manifest parser
does not accept a `roles` key.

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
- **Role grants (destination):** a generated role capability record must be a subset of
  the app ceiling by default. Any additional role-specific authority is a separately
  versioned, reviewable manifest change; it cannot arise from an Electron
  `utilityProcess` option or child request. This schema is not live.
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
   (KEL-65 direct COM — the ordering is compile-enforced). All three evaluate
   `web.camera` / `web.microphone` as the requesting `Principal::Webview`
   (host-minted `WebviewId`, generation `0` until navigation rotation lands)
   with requested resource `*` (no platform callback passes an origin).
   Missing identity and `AppProcess` fail closed (`KELD-GUARD007`) so `/app`
   media grants cannot apply to a remote or other webview. A minted webview
   principal is still `KELD-GUARD006` until window-level grants exist. CSP
   injection,
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
