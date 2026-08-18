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

**Feed layout**, one static tree per channel **and target**, servable from any
CDN/S3/GitHub Releases (no server logic required):

```text
<feed-base>/<channel>/<target>/updates.json         # manifest payload (unsigned in-band)
<feed-base>/<channel>/<target>/updates.json.sig     # detached signature over updates.json's raw bytes
<feed-base>/<channel>/<target>/<version>/full.zst                     # full package, zstd-compressed
<feed-base>/<channel>/<target>/<version>/from-<from-version>.delta.zst  # delta, zstd-compressed (diff format: KEL-53 AC2)
```

`<target>` is one of the fixed platform/architecture triples Keld actually ships
(`macos-arm64`, `macos-x64`, `windows-x64`, `linux-x64`, …, matching `keld-pack`'s own
target list — not a wire-level enum defined here). A client polls only the path for its
own compiled-in target, so cross-target confusion would require the feed operator (or
an attacker who can write to the feed) to physically publish the wrong bytes at the
right path — the URL structure is the primary defense. The manifest's own `target`
field (below) is the second, redundant layer: the same defense-in-depth shape as the
`app.id`/`channel` check, for the same reason a single control is not trusted alone.

The signature is a **separate file over the manifest's literal bytes**, not a field
embedded inside the JSON. This removes the need for a canonicalization rule (key order,
whitespace, number formatting) *between signer and verifier* — the client verifies the
exact response bytes before parsing them at all, so no JSON serialization step sits
between what was signed and what was checked. It does **not** by itself remove parser
ambiguity between different JSON implementations; §4a settles that separately: `schema`
must be an unrecognized-value-fails-closed integer (already specified below), and a
parser that accepts duplicate object keys **MUST** reject the manifest rather than
silently taking the last (or first) value — the wire contract has exactly one value per
key, and a parser that can't guarantee that is not a valid implementation of it.

`updates.json.sig` — one line, base64-encoded 64-byte ed25519 signature, no wrapper.

`updates.json` — v0 schema. Every release requires `full`; `deltas` is the only
optional piece (a release **MUST NOT** omit `full` — the full-package fallback and the
post-delta-failure fallback below both assume it exists, so a fixture that rejects a
release missing `full` is part of AC1):

```json
{
  "schema": 1,
  "channel": "stable",
  "target": "macos-arm64",
  "app": { "id": "com.example.app" },
  "releases": [
    {
      "version": "1.4.2",
      "publishedAt": "2026-08-18T00:00:00Z",
      "full": {
        "url": "1.4.2/full.zst",
        "size": 12345678,
        "blake3": "<64 hex chars>",
        "contentBlake3": "<64 hex chars>"
      },
      "deltas": [
        {
          "fromVersion": "1.4.1",
          "url": "1.4.2/from-1.4.1.delta.zst",
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
- `app.id`, `channel`, and `target` **MUST** match the host's compiled-in application
  identity, the channel it actually requested, and its own compiled-in target, checked
  fail-closed *after* signature verification and *before* any release is selected (step
  2 below). A correctly-signed manifest for a different app, channel, or target is not
  this host's update — accepting it on signature validity alone is exactly how feed
  misrouting, a shared signing key, or a wrong-target URL turns into a cross-app,
  cross-channel, or cross-platform install.
- `version` **MUST** be strict semver (`MAJOR.MINOR.PATCH`, no build-metadata suffix
  participating in ordering). Two `releases[]` entries with the same `version`, or two
  `deltas[]` entries within one release with the same `fromVersion`, make the whole
  manifest invalid — reject it outright (a schema violation, the same as an unknown
  `schema` value), not "pick one arbitrarily." Different clients silently picking
  different entries from the same signed manifest is exactly the failure a duplicate
  would cause if it were merely tolerated.
- Release selection is deterministic: among releases that pass the version-floor check
  (step 4 below), the client selects the single **highest** version, never "any
  newer" — there is exactly one answer to "what does this manifest ask me to install,"
  not a client-dependent choice among several eligible releases.
- Each release's `deltas` array may be empty or contain zero or more entries; how many
  prior versions a publisher generates deltas for (the "last N releases" in the prose
  above) is a publish-time/`keld-pack` decision, not part of this wire contract — the
  client only ever looks for one entry whose `fromVersion` equals its own installed
  version.
- `size` is **normative, not advisory**: downloads are bounded, streaming reads that
  reject an artifact once received bytes exceed `size` (before decompression ever
  starts — an oversized stream is rejected on size alone, not given the chance to
  decompress-bomb anything) and reject a stream that ends short of `size`. A `size`
  field that nothing checks is not a contract; this fixture (short and long artifacts)
  is part of AC1.
- Two hash domains, not one. `blake3` (present on both `full` and every `deltas[]`
  entry) is the digest of the **artifact's bytes as downloaded** — the `.zst` file
  exactly as served — and proves transport integrity of what was fetched, nothing
  about what it decompresses or reconstructs to. `full.contentBlake3` is the digest of
  the **decompressed, installable package bytes**: the full-package path decompresses
  `full.zst` and checks the result against `contentBlake3` before install; the delta
  path decompresses the patch and applies it against the currently-installed content,
  then checks *its* result against that same `full.contentBlake3` — both paths
  converge on one deterministic, checkable content stream regardless of which artifact
  produced it. Deltas carry no `contentBlake3` of their own; there is nothing to check
  a delta's reconstruction against except the release's one canonical content hash.
- **The canonical content stream those bytes are a hash of** is a v0-defined package
  format, not "whatever bytes happen to decompress": a single POSIX ustar archive
  (`.tar`, before the outer zstd wrapper), entries sorted by path (byte order),
  regular files only — **no symlinks, hardlinks, device files, or extended
  attributes in v0** — uniform modes (`0644` files / `0755` directories), no
  mtime/uid/gid preserved. `contentBlake3` is the digest of that exact tar byte
  stream, and extraction is the deterministic inverse: given the same tar bytes, every
  client produces the same directory tree, because there is nothing in the format left
  for a client to interpret differently. Symlinks and richer metadata are a real gap
  for a v1 packaging format (macOS `.app` bundles, in particular, are symlink-heavy) —
  named here as a v0 limitation, not solved by this contract.

**Client verification order — no step may be skipped or reordered:**

1. Fetch `updates.json` + `updates.json.sig`. Verify the detached signature against the
   ed25519 public key **compiled into the host binary at build time** (never fetched
   from the feed itself — a feed that can serve a fake manifest could equally serve a
   fake "trusted" key, so the key cannot be feed-supplied and stay a trust root).
   Reject and stop on any signature failure, before parsing a single field for meaning.
2. Parse JSON only after step 1 passes, with a parser that rejects duplicate keys.
   Reject unknown `schema`. Reject if `app.id`, `channel`, or `target` do not match this
   host's identity/requested channel/own target (fail closed on any mismatch).
3. Reject the whole manifest if any two `releases[]` entries share a `version`, or any
   two `deltas[]` entries within one release share a `fromVersion` — see the schema
   bullets above; this is a shape-validity check, independent of which release ends up
   selected.
4. Among the remaining releases, reject any whose `version` is not **strictly greater
   than the persisted version floor** (see below) — not merely greater than the
   currently-installed version. The floor, not the running version, is the
   replay/downgrade defense: after a local rollback the running version can be lower
   than the floor on purpose, and a feed offering anything at or below the floor is
   either stale or an attacker replaying an old signed manifest, never a legitimate
   forward update. Among what remains, select the single **highest** version.
5. Prefer a `deltas[]` entry whose `fromVersion` equals the installed version; else use
   `full`. Download the chosen artifact as a size-bounded stream: reject once received
   bytes exceed the artifact's `size`, and reject a stream that ends short of it.
6. BLAKE3 the downloaded bytes; compare to that artifact's own `blake3`. Reject and
   discard on mismatch — do not attempt to decompress or apply a patch that failed its
   own transport-integrity check.
7. Decompress. For `full`, check the decompressed tar bytes against `contentBlake3` —
   that is the installable package (see the canonical-content-stream bullet above for
   what "decompressed" means precisely). For a delta, apply it against the
   currently-installed content, then check the *reconstructed* bytes against that same
   release's `full.contentBlake3`. This is the oracle step 6 cannot provide: step 6
   only proves the patch file itself downloaded intact, not that applying it against
   this host's actual base produces the intended result.
8. **Any** failure on the delta path — size mismatch, transport `blake3` mismatch
   (step 6), a decompression error, a patch-application error, or a reconstructed-
   content `contentBlake3` mismatch (step 7) — triggers one `full` attempt (steps 5–7
   again, for the full artifact) **within the same update attempt**, not deferred to
   the next poll. Deferring would just re-select the same `deltas[]` entry next time
   (step 5's preference for a matching delta never changes on its own) and retry the
   identical failure forever.
9. Only content that passed a `contentBlake3` check (full path directly, delta path via
   reconstruction) is eligible to install.

**Atomic swap and rollback:**

- Each verified package is written to its own versioned directory
  (`<app-data>/versions/<version>/`), extracted per the canonical tar rules above.
  Every file in it is `fsync`ed, then the directory itself is `fsync`ed (POSIX: `fsync`
  on an open directory fd — durability for a directory entry is a metadata operation
  the file's own `fsync` does not cover, per the same rename-durability contract POSIX
  `rename(2)` documents: the rename is atomic but not durable without a following
  directory `fsync`). **Only then** is a `.complete` marker file created in that
  directory, and the marker file itself is `fsync`ed, followed by one more directory
  `fsync` to make the marker's own existence durable — a marker that isn't itself
  durably persisted is not a durable "this write finished" signal. A versioned
  directory without a durably-persisted `.complete` marker is a crash-interrupted
  write, never a candidate for `current` or for rollback.
- The **version floor and the `current` pointer are two separate durable files, and
  their publish order is load-bearing**: the floor is advanced first (same
  temp-file + `fsync` + atomic-rename + directory-`fsync` sequence as the pointer,
  described next), and only once that succeeds is `current` published to the new
  version. This ordering — floor before pointer, never the reverse — guarantees the
  floor is never behind whatever `current` could possibly show, even across a crash
  between the two writes: if the process dies after the floor advances but before
  `current` is republished, `current` still points at the old (still valid, still
  `.complete`-marked) version, and the new version's directory sits ready but
  unpublished. Startup recovery detects exactly this case — the highest
  `.complete`-marked versioned directory's version equals the persisted floor, but
  `current` does not yet point there — and *completes* the interrupted publish by
  pointing `current` at that directory. No re-download and no re-verification: the
  directory's own `.complete` marker and the floor already prove this exact version
  was fully verified before the crash: this is finishing an interrupted local write,
  not trusting new, unverified state. (The reverse order — pointer before floor —
  would leave a window where `current` already shows the new version but the floor
  still allows a replayed old signed manifest, exactly the gap this ordering closes.)
- Publishing the pointer (or the floor) is itself a durable-replace, not an in-place
  edit: write a temporary file, `fsync` it, then atomically publish it over the target
  — POSIX `rename()` onto `current` (or `version-floor`) (atomic within one filesystem;
  the directory is `fsync`ed afterward for the same reason above); Windows
  `ReplaceFile`/`MoveFileEx(..., MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)`
  on the temp file (plain `MoveFileEx` alone is not guaranteed atomic across all
  filesystem/volume combinations). **Neither file is ever removed before its
  replacement is durably in place** — there is no window where either pointer is
  absent.
- The **previous** version's directory is kept, not deleted, until the new version has
  been confirmed healthy (e.g. survives `keld-runtime`'s crash-loop breaker window,
  KEL-70).
- Startup recovery, in order: (1) if the highest `.complete`-marked versioned
  directory's version equals the persisted floor but `current` does not point there,
  complete the interrupted publish as described above; else (2) if `current` is
  missing, unreadable, or points at a versioned directory with no `.complete` marker,
  fall back to the most recent kept versioned directory that *does* have one; if
  neither recovers to a valid state, startup fails closed with a typed error — it
  never runs a partially-written install.
- **Rollback** is a host/user-authorized local action: flipping `current` back to the
  kept N-1 directory via the same durable-pointer mechanism above — no re-download, no
  re-verification of already-verified bytes. Rollback **does not** touch the persisted
  version floor: it only changes what is currently running, so a subsequent feed poll
  still can't be tricked by a replayed old signed manifest into re-offering, as if new,
  anything at or below a version this host has already run. Authorization for rollback
  (what triggers it — the crash-loop breaker automatically, or an explicit
  host/operator action) is `keld-update`'s own decision to make when it exists; this
  contract only fixes what rollback *does* to the pointer and the floor, not what
  decides to invoke it.
- **Version floor storage**: a small file alongside `current` (e.g. `version-floor`,
  containing just the semver string), written durably as specified above, updated only
  on a successful install (never by rollback). Missing on first run (no floor yet — any
  signed release for this app/channel/target is accepted); unreadable or corrupt on a
  run that has already installed at least one update is treated the same as an absent
  `current` with no `.complete` marker — fail closed, do not silently treat it as "no
  floor."

## 5. Dev loop targets

- `keld dev` cold → window ≤ 2 s (host prebuilt, Bun start ~10 ms class, webview init
  dominates); warm app-process restart ≤ 300 ms with renderer preserved.
- Unified logs: host (tracing, JSON), app process (stdout), renderer (console capture)
  interleaved in one stream with principal tags; `keld dev --inspect-ipc` is **planned**
  (decoded kipc JSON dump). Today the flag is `KELD-CLI-044` (not live).
- DevTools: system engines expose what they have (CDP on WebView2, Safari inspector on
  macOS, WebKitGTK inspector); `keld dev` prints exact attach instructions per OS —
  no pretending parity exists where it doesn't.
