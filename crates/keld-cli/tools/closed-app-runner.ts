// Local-source compiler closure entry. Not a release/signing authority.
import { bundleCapturedApp } from "./closed-app.ts";
import { decodeInputs, encodeOutputs, readBoundedInput } from "./protocol.ts";
import { admitStaticRenderer } from "./static-renderer.ts";
import {
  buildReleaseTransport,
  TEST_HOOK_NAMES,
} from "../../../packages/@keld/kipc/scripts/package-artifacts.ts";
import {
  stampTransport,
  verifyTransportStamp,
} from "../../../packages/@keld/kipc/src/transport.ts";
if (import.meta.main) {
  try {
    if (process.argv.length !== 3)
      throw new Error("Exact retained native parser required");
    const captured = decodeInputs(await readBoundedInput(Bun.stdin.stream()));
    const renderer = await admitStaticRenderer(captured.renderer);
    const parser = {
      tsserverPath: process.argv[2],
      virtualRoot: process.cwd().replaceAll("\\", "/") + "/virtual",
    };
    const main = await bundleCapturedApp(
      captured.modules,
      parser,
      captured.entry,
    );
    // The retained module derives its own source root from import.meta.dir.
    // No caller-selected alternate source root participates in this build.
    const release = await buildReleaseTransport();
    if (TEST_HOOK_NAMES.some((name) => release.includes(name)))
      throw new Error("Release transport contains test hooks");
    const transport = new TextEncoder().encode(stampTransport(release));
    verifyTransportStamp(transport, "kipc-transport.js");
    const rendererDigest = new Uint8Array(
      await crypto.subtle.digest("SHA-256", renderer),
    );
    await Bun.write(Bun.stdout, encodeOutputs(main, transport, rendererDigest));
  } catch {
    console.error("Captured application compiler refused input or output");
    process.exitCode = 1;
  }
}
