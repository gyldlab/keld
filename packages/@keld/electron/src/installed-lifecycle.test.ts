import { expect, test } from "bun:test";
import {
  appendFileSync,
  existsSync,
  mkdtempSync,
  readFileSync,
  renameSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { prepareInstalledLifecycle } from "../fixtures/installed-lifecycle.ts";

test("owned KEL72 fixtures use the actual installed alias before entry evaluation", async () => {
  const root = mkdtempSync(join(tmpdir(), "keld-lifecycle-installed-"));
  let passed = false;
  const observations: unknown[] = [];
  try {
    const consumer = await prepareInstalledLifecycle(root);
    const runner = resolve(
      import.meta.dir,
      "../fixtures/installed-lifecycle.ts",
    );
    function run(label: string, script = "shim_env.ts") {
      const result = Bun.spawnSync(
        [process.execPath, "--no-install", runner, "run", consumer, script],
        {
          cwd: consumer,
          env: {
            ...process.env,
            KELD_APP_LINK: join(root, "unbound.sock") + "#" + "72".repeat(32),
          },
          stdout: "pipe",
          stderr: "pipe",
          timeout: 15_000,
        },
      );
      const observation = {
        label,
        script,
        exit: result.exitCode,
        stdout: new TextDecoder().decode(result.stdout),
        stderr: new TextDecoder().decode(result.stderr),
      };
      observations.push(observation);
      writeFileSync(
        join(root, "probe-receipt.json"),
        JSON.stringify(observations, null, 2) + "\n",
      );
      return observation;
    }
    const positive = run("healthy process fields");
    expect(positive.exit, positive.stderr).toBe(0);
    expect(positive.stdout).toContain("KEL72_TYPE=browser");
    expect(positive.stdout).toContain("KEL72_ELECTRON=0.0.1");
    const sticky = run("sticky connection failure", "app_ready.ts");
    expect(sticky.exit, sticky.stderr).toBe(0);
    for (const marker of [
      "KEL72_FAILURE_STICKY",
      "KEL72_CONNECT_CALLS=1",
      "KEL72_UNHANDLED_COUNT=0",
    ])
      expect(sticky.stdout).toContain(marker);
    const manifestPath = join(consumer, "package.json");
    const manifest = readFileSync(manifestPath);
    try {
      const changed = JSON.parse(manifest.toString());
      delete changed.dependencies.electron;
      writeFileSync(manifestPath, JSON.stringify(changed));
      const refused = run("missing declared Electron dependency");
      expect(refused.exit).not.toBe(0);
      expect(refused.stderr).toContain("KEL72_ALIAS_DEPENDENCIES_CHANGED");
      expect(refused.stdout).not.toContain("KEL72_TYPE=");
    } finally {
      writeFileSync(manifestPath, manifest);
    }
    for (const [name, text, code] of [
      [
        "tsconfig.json",
        '{"compilerOptions":{"paths":{"electron":["./forbidden.ts"]}}}',
        "KEL72_COMPETING_TSCONFIG",
      ],
      [
        "bunfig.toml",
        '[alias]\nelectron = "./forbidden.ts"\n',
        "KEL72_COMPETING_BUNFIG",
      ],
    ]) {
      const path = join(consumer, name);
      try {
        writeFileSync(path, text);
        const refused = run("competing " + name);
        expect(refused.exit).not.toBe(0);
        expect(refused.stderr).toContain(code);
        expect(refused.stdout).not.toContain("KEL72_TYPE=");
      } finally {
        rmSync(path);
      }
    }
    const kipc = join(consumer, "node_modules/@keld/kipc");
    const removed = join(root, "removed-kipc");
    renameSync(kipc, removed);
    try {
      const refused = run("missing installed KIPC");
      expect(refused.exit).not.toBe(0);
      expect(refused.stderr).toContain("KEL72_MISSING_DEPENDENCY: @keld/kipc");
      expect(refused.stdout).not.toContain("KEL72_TYPE=");
    } finally {
      renameSync(removed, kipc);
    }
    // One fault at a time. Only the actual installed runtime code changes;
    // tarball integrity, package-lock, manifest, and fixture entry remain valid.
    for (const [name, relative, marker] of [
      [
        "Electron entry",
        "node_modules/electron/dist/index.js",
        "KEL72_MUTANT_ELECTRON_EXECUTED",
      ],
      [
        "API application leaf",
        "node_modules/@keld/api/app.js",
        "KEL72_MUTANT_API_EXECUTED",
      ],
      [
        "KIPC canonical Worker",
        "node_modules/@keld/kipc/transport.js",
        "KEL72_MUTANT_KIPC_EXECUTED",
      ],
    ] as const) {
      const path = join(consumer, relative);
      const original = readFileSync(path);
      try {
        appendFileSync(
          path,
          "\nconsole.error(" + JSON.stringify(marker) + ");\n",
        );
        const refused = run("changed installed bytes: " + name);
        expect(refused.exit).not.toBe(0);
        expect(refused.stderr).toContain("KEL72_INSTALLED_BYTE_MISMATCH");
        expect(refused.stderr).not.toContain(marker);
        expect(refused.stdout).not.toContain("KEL72_TYPE=");
      } finally {
        writeFileSync(path, original);
      }
    }

    const custody = JSON.parse(
      readFileSync(join(root, "fixture-custody.json"), "utf8"),
    ) as { installedPackages: { name: string; artifact: string }[] };
    const electronArtifact = custody.installedPackages.find(
      (item) => item.name === "electron",
    )?.artifact;
    expect(electronArtifact).toBeDefined();
    const archivePath = electronArtifact!;
    const originalArchive = readFileSync(archivePath);
    try {
      appendFileSync(archivePath, "KEL72_MUTATED_PACKED_ARCHIVE");
      const refused = run("changed packed Electron archive bytes");
      expect(refused.exit).not.toBe(0);
      expect(refused.stderr).toContain("KEL72_PACKED_ARTIFACT_CHANGED");
      expect(refused.stdout).not.toContain("KEL72_TYPE=");
    } finally {
      writeFileSync(archivePath, originalArchive);
    }

    const apiLeaf = join(consumer, "node_modules/@keld/api/app.js");
    const retiredLeaf = join(root, "temporarily-removed-installed-api-leaf");
    renameSync(apiLeaf, retiredLeaf);
    try {
      const refused = run("missing installed API file");
      expect(refused.exit).not.toBe(0);
      expect(refused.stderr).toContain("KEL72_INSTALLED_FILE_SET_CHANGED");
      expect(refused.stdout).not.toContain("KEL72_TYPE=");
    } finally {
      renameSync(retiredLeaf, apiLeaf);
    }
    const extraFile = join(
      consumer,
      "node_modules/electron/dist/keld-extra.js",
    );
    expect(existsSync(extraFile)).toBe(false);
    try {
      writeFileSync(extraFile, "console.error('KEL72_EXTRA_EXECUTED');\n");
      const refused = run("extra installed executable file");
      expect(refused.exit).not.toBe(0);
      expect(refused.stderr).toContain("KEL72_INSTALLED_FILE_SET_CHANGED");
      expect(refused.stderr).not.toContain("KEL72_EXTRA_EXECUTED");
      expect(refused.stdout).not.toContain("KEL72_TYPE=");
    } finally {
      rmSync(extraFile, { force: true });
    }

    // macOS/Linux own this symlink falsifier. No Windows symlink privilege assumed.
    if (process.platform !== "win32") {
      const alias = join(consumer, "node_modules/@keld/api/keld-linked.js");
      expect(existsSync(alias)).toBe(false);
      try {
        symlinkSync("app.js", alias);
        const refused = run("unexpected installed package symlink");
        expect(refused.exit).not.toBe(0);
        expect(refused.stderr).toContain("KEL72_INSTALLED_SYMLINK");
        expect(refused.stdout).not.toContain("KEL72_TYPE=");
      } finally {
        rmSync(alias, { force: true });
      }
    }

    const restored = run("restored installed graph");
    expect(restored.exit, restored.stderr).toBe(0);
    expect(restored.stdout).toContain("KEL72_TYPE=browser");
    passed = true;
  } finally {
    if (passed && process.env.KELD_ELECTRON_KEEP_FIXTURE !== "1")
      rmSync(root, { recursive: true, force: true });
    else console.log("KEL72_INSTALLED_FIXTURE " + root);
  }
}, 180_000);
