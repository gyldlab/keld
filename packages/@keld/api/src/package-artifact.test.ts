import { expect, test } from "bun:test";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const packages = resolve(import.meta.dir, "../..");

test("real packed SDK installs offline with one API app identity", () => {
  const scratch = mkdtempSync(join(tmpdir(), "keld-sdk-package-"));
  for (const name of ["api", "kipc"]) {
    const build = Bun.spawnSync(["bun", "pm", "pack", "--ignore-scripts", "--destination", scratch], { cwd: join(packages, name), stdout: "pipe", stderr: "pipe" });
    expect(build.exitCode, new TextDecoder().decode(build.stderr)).toBe(0);
  }
  const api = join(scratch, "keld-api-0.0.1.tgz");
  const kipc = join(scratch, "keld-kipc-0.0.1.tgz");
  const consumer = join(scratch, "consumer");
  require("node:fs").mkdirSync(consumer);
  writeFileSync(join(consumer, "package.json"), JSON.stringify({
    name: "sdk-consumer", private: true, type: "module",
    dependencies: { "@keld/api": `file:${api}`, "@keld/kipc": `file:${kipc}` },
  }));
  const install = Bun.spawnSync(["npm", "install", "--offline", "--ignore-scripts", "--no-audit", "--no-fund", "--cache", join(scratch, "cache")], { cwd: consumer, stdout: "pipe", stderr: "pipe" });
  expect(install.exitCode, new TextDecoder().decode(install.stderr)).toBe(0);
  const manifest = JSON.parse(readFileSync(join(consumer, "node_modules/@keld/api/package.json"), "utf8"));
  expect(manifest.dependencies["@keld/kipc"]).toBe("0.0.1");
  const probe = join(consumer, "probe.mjs");
  writeFileSync(probe, `import {strict as assert} from "node:assert";
import {createRequire} from "node:module";
import * as api from "@keld/api";
import {app} from "@keld/api/internal/app";
import "@keld/api/internal/link";
import "@keld/api/internal/window-codec";
assert.equal(api.app, app);
assert.equal(api.app, createRequire(import.meta.url)("@keld/api").app);
assert.deepEqual(Object.keys(api).sort(), ["app", "auth", "channels", "echoChannel", "fs", "isCallError"].sort());`);
  const run = Bun.spawnSync(["bun", probe], { cwd: consumer, stdout: "pipe", stderr: "pipe" });
  expect(run.exitCode, new TextDecoder().decode(run.stderr)).toBe(0);
});
