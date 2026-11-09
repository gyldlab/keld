/**
 * Cross-platform installed-artifact boundary: a genuinely packed API/KIPC
 * distribution must resolve its bare exports and enter the real stamped
 * transport before reporting a missing Host link. This independent regression
 * provides subprocess stderr when Windows packaging fails before HELLO; it
 * does not replace the existing positive same-link Init/ACK oracle.
 */
import { expect, test } from "bun:test";
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { freshWin32AppLinkPipe } from "../../kipc/test/app-link-host.ts";

test("packed SDK imports and fails closed on an unbound app-link using stamped transport", () => {
  const stage = mkdtempSync(join(tmpdir(), "krp-packed-startup-"));
  try {
    const producer = join(
      import.meta.dir,
      "../../kipc/scripts/package-artifacts.ts",
    );
    const packed = Bun.spawnSync([process.execPath, producer, stage], {
      stdout: "pipe",
      stderr: "pipe",
    });
    expect(
      packed.exitCode,
      packed.stdout.toString() + packed.stderr.toString(),
    ).toBe(0);

    for (const name of ["api", "kipc"]) {
      const manifest = JSON.parse(
        readFileSync(
          join(import.meta.dir, `../../${name}/package.json`),
          "utf8",
        ),
      );
      mkdirSync(join(stage, "node_modules", "@keld", name), {
        recursive: true,
      });
      const extracted = Bun.spawnSync(
        [
          "tar",
          "-xzf",
          `keld-${name}-${manifest.version}.tgz`,
          "--strip-components=1",
          "-C",
          `node_modules/@keld/${name}`,
        ],
        { cwd: stage, stdout: "pipe", stderr: "pipe" },
      );
      expect(extracted.exitCode, extracted.stderr.toString()).toBe(0);
    }

    const entry = join(stage, "entry.mjs");
    writeFileSync(
      entry,
      'import {app} from "@keld/api"; console.log("API_IMPORT", typeof app.whenReady);\n',
    );
    const importResult = Bun.spawnSync(
      [process.execPath, "--no-env-file", "--no-install", entry],
      { cwd: stage, stdout: "pipe", stderr: "pipe", timeout: 9_000 },
    );
    expect(importResult.exitCode, importResult.stderr.toString()).toBe(0);
    expect(importResult.stdout.toString()).toBe("API_IMPORT function\n");
    expect(importResult.stderr.toString()).toBe("");

    const wrapper = join(stage, "node_modules/@keld/api/runtime-preload.js");
    const missingEndpoint =
      process.platform === "win32"
        ? freshWin32AppLinkPipe()
        : join(stage, "unbound.sock");
    const preloadResult = Bun.spawnSync(
      [
        process.execPath,
        "--no-env-file",
        "--no-install",
        "--define=__KELD_RUNTIME_PROCESS_FACTS__:true",
        "--preload",
        wrapper,
        entry,
      ],
      {
        cwd: stage,
        env: {
          ...process.env,
          KELD_APP_LINK: missingEndpoint + "#" + "72".repeat(32),
          KELD_NATIVE_WINDOW_FACADE: "1",
        },
        stdout: "pipe",
        stderr: "pipe",
        timeout: 9_000,
      },
    );
    expect(
      preloadResult.exitCode,
      preloadResult.stdout.toString() + preloadResult.stderr.toString(),
    ).toBe(1);
    expect(preloadResult.stdout.toString()).toBe("");
    expect(preloadResult.stderr.toString()).toContain("KELD-IPC-001");
  } finally {
    rmSync(stage, { recursive: true, force: true });
  }
}, 20_000);
