import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { realpathSync } from "node:fs";
import { values as root } from "./root-matrix.mjs";
import { values as nested } from "@keld/gh502-probe-dependency";
import { app as publicApi } from "@keld/api";
import { app as internalApi } from "@keld/api/internal/app";
import * as link from "@keld/api/internal/link";
import * as kipc from "@keld/kipc";

import * as originalIpc from "electron/internal/original-ipc";
import { createRequire } from "node:module";
const internalRequired = createRequire(import.meta.url)(
  "electron/internal/original-ipc",
);
const internalNames = [
  "awaitOriginalIpcCloseVeto",
  "destroyOriginalIpcDocument",
  "isCurrentOriginalIpcDocument",
];
assert.deepEqual(Object.keys(originalIpc).sort(), internalNames);
for (const name of internalNames)
  assert.equal(
    originalIpc[name],
    internalRequired[name],
    "same internal ESM/CJS helper",
  );
assert.equal(
  originalIpc.isCurrentOriginalIpcDocument({}),
  false,
  "forged event cannot acquire facade capability",
);
await assert.rejects(
  originalIpc.destroyOriginalIpcDocument({}),
  (error) => error.code === "KELD-CORE-054",
);
await assert.rejects(
  originalIpc.awaitOriginalIpcCloseVeto({}),
  (error) => error.code === "KELD-CORE-054",
);
const expected = [
  "BrowserWindow",
  "ClipboardItem",
  "Menu",
  "MenuItem",
  "app",
  "clipboard",
  "dialog",
  "ipcMain",
  "nativeImage",
  "nativeTheme",
  "screen",
  "session",
  "shell",
];
const app = root[0][1].app;
const entry = realpathSync(fileURLToPath(root[0][5]));
let observations = 0;
for (const [caller, rows] of [
  ["app", root],
  ["dependency", nested],
]) {
  for (const [specifier, def, named, ns, required, url, requirePath] of rows) {
    for (const actual of [def.app, named, ns.app, required.app]) {
      assert.equal(actual, app, caller + ": " + specifier);
      observations++;
    }
    assert.deepEqual(
      Object.keys(def).sort(),
      expected,
      "public default member set",
    );
    assert.deepEqual(
      Object.keys(ns)
        .filter((key) => key !== "default")
        .sort(),
      expected,
      "public namespace parity",
    );
    assert.equal(realpathSync(fileURLToPath(url)), entry);
    assert.equal(realpathSync(requirePath), entry);
  }
}
assert.equal(publicApi, internalApi);
for (const key of [
  "FrameReader",
  "DrainSignal",
  "RECEIVE_POLICIES",
  "isCallError",
])
  assert.equal(link[key], kipc[key], "canonical transport identity: " + key);
console.log(
  JSON.stringify({
    observations,
    entry,
    members: expected,
    platform: process.platform,
    arch: process.arch,
    runtime: typeof Bun === "undefined" ? "node" : "bun",
    version: typeof Bun === "undefined" ? process.version : Bun.version,
  }),
);
