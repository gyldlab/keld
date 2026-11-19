import assert from "node:assert/strict";
import { join } from "node:path";
import { test } from "bun:test";
import { APP_LINK_IO_DEADLINE_MS, withIoDeadline } from "@keld/kipc";

const admitted = {
  schema: 8,
  kind: "development",
  name: "Selected editor",
  root: String.raw`\\?\D:\retained-stage`,
  paths: {},
  originalResourcePolicy: {
    contract: "gh473-original-editor-offline/v1",
    graphSha256: "sha256:" + "ab".repeat(32),
    policySha256: "sha256:" + "cd".repeat(32),
    windowKey: "editor",
    selector: "original-editor-offline",
  },
};

for (const mode of [
  "selected",
  "wrong-profile",
  "missing-policy",
  "invalidated",
  "late-import",
]) {
  test(`Windows selected Session registration: ${mode}`, async () => {
    const child = Bun.spawn(
      [
        process.execPath,
        "--no-install",
        "--no-env-file",
        "--define=__KELD_WINDOWS_HOST__:true",
        "--define=__KELD_WINDOWS_DESKTOP_SNAPSHOT__:true",
        `--define=__KELD_RUNTIME_PROCESS_FACTS__:${mode !== "wrong-profile"}`,
        ...(mode === "selected" ||
        mode === "invalidated" ||
        mode === "late-import"
          ? [`--define=__KELD_HOST_LAUNCH_FACTS__:${JSON.stringify(admitted)}`]
          : []),
        join(import.meta.dir, "windows-session.test-role.ts"),
        mode,
      ],
      {
        env: {
          ...process.env,
          KELD_APP_LINK: "controlled-link",
          KELD_NATIVE_WINDOW_FACADE: "1",
        },
        stdin: "ignore",
        stdout: "pipe",
        stderr: "pipe",
      },
    );
    try {
      const [exit, stdout, stderr] = await withIoDeadline(
        Promise.all([
          child.exited,
          new Response(child.stdout).text(),
          new Response(child.stderr).text(),
        ]),
        APP_LINK_IO_DEADLINE_MS,
      );
      assert.equal(exit, 0, `${stdout}\n${stderr}`);
      const accepted = [
        ...stderr.matchAll(
          /Keld GH-553 session\.defaultSession\.webRequest\.(\w+):/g,
        ),
      ].map((record) => record[1]);
      assert.deepEqual(
        accepted,
        mode === "selected"
          ? ["onHeadersReceived", "onBeforeRequest", "onBeforeRequest"]
          : mode === "late-import"
            ? ["onBeforeRequest"]
            : [],
      );
    } finally {
      if (child.exitCode === null) child.kill();
      await child.exited;
    }
  });
}
