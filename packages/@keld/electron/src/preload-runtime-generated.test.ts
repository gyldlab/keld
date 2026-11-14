import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { runInNewContext } from "node:vm";

const root = resolve(import.meta.dir, "../../../..");
const output = resolve(
  root,
  "crates/keld-wv/src/wkwebview/macos_preload_runtime.generated.js",
);

test("embedded preload is the exact pinned generator output", () => {
  const result = Bun.spawnSync(
    [process.execPath, resolve(root, "packages/@keld/electron/scripts/preload-runtime-codegen.ts"), "check"],
    { cwd: root },
  );
  expect(result.exitCode, result.stderr.toString()).toBe(0);
});

test("embedded codec refuses native Map contents instead of flattening them", () => {
  const bundle = readFileSync(output, "utf8");
  const anchor = "  function encodeEndpointArguments(args) {";
  expect(bundle.split(anchor).length).toBe(2);
  // Observe the existing bundled function without replacing its implementation.
  // This is a JS VM contract check, not native WebKit acceptance.
  const observed = bundle.replace(
    anchor,
    "  globalThis.observeEncode = encodeEndpointArguments;\n" + anchor,
  );
  const context = { TextEncoder, TextDecoder };
  runInNewContext(observed, context);
  const result = runInNewContext(`
    (() => {
      const plain = Array.from(observeEncode([{}]));
      const map = new Map([["retained", 42]]);
      Object.setPrototypeOf(map, null);
      let code;
      try { observeEncode([map]); } catch (error) { code = error.code; }
      return { plain, code };
    })()
  `, context);
  expect(Array.from(result.plain)).toEqual([6, 1, 0, 7, 0, 0]);
  expect(result.code).toBe("KELD-COMPAT-100");
});
