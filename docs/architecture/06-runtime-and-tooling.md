# Runtime, CLI, Packaging, Updates — The Solution-in-a-Box Layer

## 1. keld-runtime: Bun as a supervised component

- **Contract, not embedding.** Bun has no stable embedding C API (oven-sh/bun#12017 /
  #14252 remain unshipped; bun:ffi is explicitly experimental). Keld therefore treats
  the runtime as a *versioned process contract*: spawn `bun <entry>` with
  `KELD_LINK={fd|pipe}`, `KELD_SHM={handle}`, `KELD_CONTRACT=keld.app.json`; `@keld/api`
  (pure TS + one tiny N-API glue for shm views) speaks kipc back. Pin exact Bun version
  per Keld release (`keld.lock`); CLI downloads the pinned runtime once per machine
  (content-addressed cache), `keld-pack` embeds it per app at build.
- Trimming: ship Bun as-is first (compressed ~25–35 MB inside installers); track
  upstream size work; `runtime: "none"` mode omits it entirely (host-only apps score
  Tauri-class sizes). A `runtime: "node"` escape hatch is deliberately **not** in v1 —
  Bun's Node-compat is the compat plan; revisit only if corpus data forces it.
- Supervision: exponential backoff restart, crash-loop breaker (3 crashes/30 s → error
  window with diagnostics), stdout/stderr captured into unified logs, `--inspect`
  passthrough, graceful-exit protocol (drain kipc, flush state, SIGTERM deadline).
- The renderer outlives app-process restarts (host owns windows) — a reliability
  property none of Electron/Electrobun/Deno has.

## 2. keld CLI: verbs and guarantees

| Verb | Contract |
|---|---|
| `keld create` / `create-keld` | templates: vanilla-ts, react, vue, svelte, solid, electron-migration; first window < 60 s from cold |
| `keld dev` | starts app's own dev server (delegation, Deno lesson D4), spawns host with dev profile (permission recorder, hot-restart of app process on change via Bun watch, devtools open policy) |
| `keld build` | app bundle via the app's bundler → `keld-pack` → signed installers + update artifacts; `--frozen-permissions` gate |
| `keld migrate` | Electron analyzer + config generator + compat report (see 04-electron-compat) |
| `keld doctor` | env checks, native-module DB scan, permission diffs, web-baseline scan (`--web-compat`), Linux GPU matrix probe |
| `keld gen` | schema → TS/Rust codegen (also runs inside dev/build) |
| `keld ext` | plugin scaffolding/build (the only cargo touchpoint, plugin authors only) |

v0 live verbs: `create`, `dev`, `doctor`, `mcp`, `hello`, `ipc-echo`, `ipc-client`.
`keld doctor` checks Bun on PATH, hello-template layout (`keld.config.ts` +
`src/main.ts`), the configured renderer HTML (default `index.html`; missing or
non-project-relative is `KELD-CLI-035`), and a webview info line on macOS,
Windows, and Linux (all three live `WebEngine` backends as of KEL-28).
Native-module DB, permission diffs, and `--web-compat` are
not live. The Linux GPU-stack probe (`webkitgtk::probe_gpu_stack`, KEL-28) runs
automatically at engine creation and applies NVIDIA+Wayland safe-mode
internally; it is not yet its own `keld doctor` line — the `webview` check only
reports backend availability, not safe-mode state. Unknown flags on live verbs with a closed flag set (`create`, `dev`,
`doctor`, `hello`) are `KELD-CLI-044` (exit 2). `keld create` takes one project
name; `--template` is not live (vanilla-ts hello only). `keld dev` takes no
flags; `--watch` and `--inspect-ipc` are not live. Spec-named `build` /
`migrate` / `gen` / `ext` are `KELD-CLI-045` (exit 2) with a tracking issue and
the Phase 2 workaround (`keld create` then `keld dev`) — not a bare "unknown
command". Garbage verbs are `KELD-CLI-046` (exit 2).

**v0 env var is `KELD_APP_LINK`, not `KELD_LINK`/`KELD_SHM`/`KELD_CONTRACT`.**
§1's contract above is the destination shape; `keld-runtime`'s pinning/download of Bun,
the destination env vars, `--inspect` passthrough, and Bun watch hot-restart are not
built yet. Spawn/backoff/crash-loop supervision **is** built (KEL-70):
`keld_runtime::Supervisor` spawns the child, captures its stdout/stderr, and restarts it
on crash with exponential backoff up to a `RestartPolicy` (default 3 crashes / 30s)
before giving up with a typed `KELD-RUNTIME-002`. `keld dev` (`crates/keld-cli/src/dev.rs`
`run_dev_echo`) spawns through that supervisor, not a bare `Command::new("bun")` wait;
the app-link env var is still `KELD_APP_LINK=<endpoint>#<64 hex chars>`
(`docs/architecture/02-ipc.md` §1).
The Bun side speaks kipc directly — `templates/hello/src/kipc.ts` is a
hand-written, wire-exact v0 client (postcard framing, one `HELLO` per
connection, then N `CALL`/`REPLY` via `AppLinkSession`). `keld gen` /
`@keld/schema` codegen (KEL-13) is not built, so this is the actual
"Bun to Rust and back" vertical slice (KEL-30), not the destination codegen
pipeline. `keld ipc-client echo` remains a separate CLI-side kipc client,
useful standalone; the template no longer shells out to it.

Distribution: `@keld/cli` npm package with per-platform binaries under
`optionalDependencies` (esbuild pattern); `bunx keld` / `npx keld` work with zero
global install. Host + runtime binaries fetched signed, verified, cached.

## 3. keld-pack: packaging & cross-compilation

- Formats: macOS `.app`/`.dmg` (+ notarization via rcodesign — pure Rust, no Xcode
  needed for CI), Windows NSIS + MSI (WiX-free Rust authoring, Deno proved viability),
  Linux `.deb`/`.rpm`/AppImage/flatpak manifest.
- **Cross-compile everything from one machine**: because the host is prebuilt per
  platform and JS is portable, `keld build --target win-x64 --target linux-arm64` is
  data assembly + signing. Matches Deno Desktop's headline capability; beats
  Tauri/Electrobun (per-OS build farms) structurally.
- Signing: platform signers driven natively (rcodesign / signtool / osslsigncode
  fallback), config in `keld.build.ts`, CI recipes documented for GitHub Actions.

## 4. keld-update: delta updates as a default

- Artifacts: per-release zstd-compressed bsdiff patches (HDiffPatch evaluated in bench
  before freeze) between the last N releases + full package fallback; static-host
  compatible feed (`updates.json` manifest, any CDN/S3/GitHub Releases).
- Client: host-side (no separate updater binary), background polling with jitter,
  BLAKE3 post-conditions + ed25519 manifest signatures, atomic swap + N-1 rollback,
  channels (stable/beta/canary), UI hooks exposed as kipc events; `autoUpdater` compat
  facade for migrated apps; bridge-release recipe for Electron switchers (04 §7).
- Budget: 1-line JS change → ≤ 50 KB patch (Electrobun demonstrated 4 KB-class is
  feasible; our floor includes manifest + signature overhead).
- All three platforms at v1 — explicitly ahead of Electrobun (Windows stability caveats)
  and Deno Desktop (no Windows auto-update).

### 4a. v0 manifest & feed wire contract (KEL-53 trigger)

The paragraph above names the pieces; this subsection is the byte-level contract KEL-53
needs before its fixtures (valid/tampered manifest, corrupted patch, full-package
fallback, N-1 rollback) can be written as executable acceptance tests instead of
prose. **Not decided here:** bsdiff vs HDiffPatch (KEL-53 AC2 benchmarks that), the
exact `ed25519-dalek`/`zstd`/delta crate choices (KEL-53 AC3, dependency review gate),
or a TUF-style rotating root (03-security.md §4 point 4 names it as a target; v0 below
is a single pinned key, stated as a v0 limitation, not silently narrowed).

**Feed layout**, one static tree per channel, servable from any CDN/S3/GitHub Releases
(no server logic required):

```
<feed-base>/<channel>/updates.json         # manifest payload (unsigned in-band)
<feed-base>/<channel>/updates.json.sig     # detached signature over updates.json's raw bytes
<feed-base>/<channel>/<version>/full.zst                      # full package, zstd-compressed
<feed-base>/<channel>/<version>/from-<from-version>.bsdiff.zst  # delta, zstd-compressed
```

The signature is a **separate file over the manifest's literal bytes**, not a field
embedded inside the JSON. Embedding a signature inside the object it covers requires a
canonicalization rule (key order, whitespace, number formatting) to avoid signature
malleability; a detached signature over the exact response bytes has no such rule to
get wrong, and a client-side fixture can corrupt one byte of `updates.json` and assert
the existing signature now fails verification, with no serialization logic involved.

`updates.json.sig` — one line, base64-encoded 64-byte ed25519 signature, no wrapper.

`updates.json` — v0 schema:

```json
{
  "schema": 1,
  "channel": "stable",
  "app": { "id": "com.example.app" },
  "releases": [
    {
      "version": "1.4.2",
      "publishedAt": "2026-08-18T00:00:00Z",
      "full": { "url": "1.4.2/full.zst", "size": 12345678, "blake3": "<64 hex chars>" },
      "deltas": [
        {
          "fromVersion": "1.4.1",
          "url": "1.4.2/from-1.4.1.bsdiff.zst",
          "size": 45678,
          "blake3": "<64 hex chars>"
        }
      ]
    }
  ]
}
```

- `schema` is an integer, bumped on any incompatible field change — a client that does
  not recognize the value fails closed (refuses the feed) rather than guessing.
- `deltas` may be empty or contain zero or more entries; how many prior versions a
  publisher generates deltas for (the "last N releases" in the prose above) is a
  publish-time/`keld-pack` decision, not part of this wire contract — the client only
  ever looks for one entry whose `fromVersion` equals its own installed version.
- Every `blake3` is the digest of the **downloaded artifact's own bytes** (the `.zst`
  file as served) — this verifies transport integrity of what was fetched, not yet
  what applying a delta produces (see verification order below).

**Client verification order — no step may be skipped or reordered:**

1. Fetch `updates.json` + `updates.json.sig`. Verify the detached signature against the
   ed25519 public key **compiled into the host binary at build time** (never fetched
   from the feed itself — a feed that can serve a fake manifest could equally serve a
   fake "trusted" key, so the key cannot be feed-supplied and stay a trust root).
   Reject and stop on any signature failure, before parsing a single field for meaning.
2. Parse JSON only after step 1 passes. Reject unknown `schema`. Select the channel's
   `releases` newer than the installed `version` (semver order).
3. Prefer a `deltas[]` entry whose `fromVersion` equals the installed version; else use
   `full`. Download the chosen artifact.
4. BLAKE3 the downloaded bytes; compare to that artifact's own `blake3`. Reject and
   discard on mismatch — do not attempt to apply a patch that failed its own integrity
   check.
5. If a delta: zstd-decompress, apply the bsdiff patch against the currently-installed
   full package. **Then** BLAKE3 the *reconstructed* full package and compare against
   the release's `full.blake3` — the same field the full-package path already carries,
   not a new one. This is the oracle that step 4 cannot provide: step 4 only proves the
   patch file itself downloaded intact, not that applying it against this host's actual
   base produces the intended result. A reconstructed package that fails this check is
   discarded and the client falls back to `full` on the next poll, never installed.
6. Only a package that has passed either the full-package `blake3` check (direct
   download) or both delta checks in step 4 and step 5 (delta path) is eligible to
   install.

**Atomic swap and rollback:** each verified package is written to its own versioned
directory (`<app-data>/versions/<version>/`, fully written and fsynced before use); a
single `current` pointer is flipped to it — a symlink on macOS/Linux, a rename of a
pointer file on Windows (an NTFS directory rename is atomic within one volume; Keld
does not attempt a same-volume guarantee across drive letters). The **previous**
version's directory is kept, not deleted, until the new version has been confirmed
healthy (e.g. survives `keld-runtime`'s crash-loop breaker window, KEL-70). Rollback is
flipping `current` back to the kept N-1 directory — no re-download, no re-verification
of already-verified bytes, and the same atomic-pointer mechanism as a forward update.

## 5. Dev loop targets

- `keld dev` cold → window ≤ 2 s (host prebuilt, Bun start ~10 ms class, webview init
  dominates); warm app-process restart ≤ 300 ms with renderer preserved.
- Unified logs: host (tracing, JSON), app process (stdout), renderer (console capture)
  interleaved in one stream with principal tags; `keld dev --inspect-ipc` is **planned**
  (decoded kipc JSON dump). Today the flag is `KELD-CLI-044` (not live).
- DevTools: system engines expose what they have (CDP on WebView2, Safari inspector on
  macOS, WebKitGTK inspector); `keld dev` prints exact attach instructions per OS —
  no pretending parity exists where it doesn't.
