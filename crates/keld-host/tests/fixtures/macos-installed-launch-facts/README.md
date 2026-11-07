# Installed launch-facts native probe

This fixture observes the shipping installed Host and bundled Bun. Unit tests
under `packages/@keld/electron/src/installed-launch-preload.test.ts` qualify the
module order separately. They do not replace this installed launch.

## Stage with the existing minimal local project

1. Use the current `keld create` output and the existing approved renderer/policy
   fixture. Copy `main.ts`, `early.ts` and `ready-witness.ts` into `src/`.
   The test driver writes `src/witness-config.ts` with one literal export:
   `readyWitnessConfig = { directory, nonce } as const`. Use a new, empty,
   driver-owned evidence directory and a fresh 64-character lowercase hex nonce.
   The witness configuration is test observation data, never production launch
   facts, an environment carrier or an extra app argument.
2. Stage the current framework production `src/` files and `package.json` for
   `@keld/electron`, `@keld/api` and `@keld/kipc` below
   `node_modules/@keld/<package>/`. Preserve the package layout. Exclude test files,
   test directories and package-local `node_modules`; do not copy dependency
   symlinks. Copy the
   release-stamped transport emitted by `keld create` to the staged
   `node_modules/@keld/kipc/src/transport.ts`; do not reconstruct its stamp or
   release constant. There are no application alias or fake-facade substitutions.
3. After preserving that canonical transport in the package, copy
   `transport-preload-observer.ts` to `src/kipc-transport.ts`. This fixture sidecar
   imports the early observer and delegates IPC to the real canonical package. It
   exercises the actual Host's two-preload order; it is not a second IPC owner.
4. Declare `src` and those three package roots as build resources. Use the exact
   current Host and Bun input pins. The production Core/CLI producer supplies all
   three `keld-launch-facts/` modules; the fixture supplies none of them.
5. Build through `keld build --target macos-arm64`, copy/install the resulting
   signed app at an independent path, then launch its actual no-flag Host. Capture
   its output with the existing native acceptance runner. Do not invoke Bun with
   manual launch-fact defines or preload flags.

## Independent oracles

The first dependency prints `GH1404_NATIVE_FACTS_FIRST_DEPENDENCY` before importing
the facade. The next record prints the actual public app getters, private record,
Bun process identity and argv. The fixture derives its expected paths from the
entry URL only as an independent test oracle. Production facts do not derive
their values this way.

Compare the full JSON against the copied app location, the independently read
sealed metadata, the actual Host/Guardian/Bun process census and the input Bun
digest. Before requesting Quit, require the atomic `<pid>.json` witness in that
directory. It is written only after actual `app.whenReady()`, `app.isReady()` and
all fact assertions, through an exclusive mode-0600 partial file followed by rename.
Its exact shape is `{nonce,pid,ppid,ready:true,facts}`; `facts` is the full
BEFORE_READY record. Bind PID/parent PID to independently observed live process
identities and require the unique nonce. Also require the native window. Guardian
stdout may be buffered until shutdown, so a pending log marker is not the causal
pre-Quit gate. Parse the full output and READY marker after shutdown. The fixture
remains alive so the native Quit and owned-family cleanup oracle can run.

Repeat after renaming and relocating the copied app. Set decoy application path,
packaged-state and resource environment variables in the launch runner; they must
not change the observations. The expected user-argument list is empty.

For each of the three generated modules, independently alter or remove that file
in a disposable copied bundle and re-sign it ad hoc with the existing signing
owner. A valid new outer signature must not satisfy the Host's exact owner-byte
check. Require refusal before `GH1404_NATIVE_FACTS_FIRST_DEPENDENCY`, then confirm
no Bun/app effects. Also preserve the existing un-re-signed signature controls.

A passing probe is installed launch-fact evidence. It does not qualify the original
draw.io graph, native editor workflow, publisher authenticity or public distribution.
