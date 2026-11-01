/** Owned installed fixture only. Expected provider bytes come from packed archives. */
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";
import { prepareInstalledLifecycle } from "../../../../packages/@keld/electron/fixtures/installed-lifecycle.ts";

const [root, role, early] = process.argv.slice(2);
assert(root && role && early);
const project = await prepareInstalledLifecycle(root);
mkdirSync(join(project, "src"));
writeFileSync(join(project, "src/main.mjs"), readFileSync(role));
writeFileSync(join(project, "src/early.mjs"), readFileSync(early));
writeFileSync(
  join(project, "keld.config.ts"),
  'export default {\n  name: "GH499 packed graph",\n  entry: "src/main.mjs",\n  initialWindow: "none",\n} as const;\n',
);
writeFileSync(
  join(project, "keld.permissions.jsonc"),
  '{"profile":"legacy"}\n',
);
mkdirSync(join(project, "drawio/src/main/webapp"), { recursive: true });
writeFileSync(
  join(project, "drawio/src/main/webapp/index.html"),
  "<html>inert graph-only asset</html>\n",
);

const custody = JSON.parse(
  readFileSync(join(root, "fixture-custody.json"), "utf8"),
);
// This bounded first-proof layout intentionally uses one exact local archive
// spelling in both package and npm lock. npm rewrites absolute dependencies
// to relative `resolved` entries; that is not the closed raw-match form.
// The excluded .keld directory is build input only, never staged/runtime code.
const archiveRoot = join(project, ".keld/approved-artifacts");
mkdirSync(archiveRoot, { recursive: true });
const local = new Map<string, string>();
for (const item of custody.installedPackages) {
  const bytes = readFileSync(item.artifact);
  assert.equal(
    "sha512-" + createHash("sha512").update(bytes).digest("base64"),
    item.integrity,
  );
  const name = basename(item.artifact);
  writeFileSync(join(archiveRoot, name), bytes, { flag: "wx" });
  local.set(item.name, "file:.keld/approved-artifacts/" + name);
}
const packagePath = join(project, "package.json");
const manifest = JSON.parse(readFileSync(packagePath, "utf8"));
for (const name of Object.keys(manifest.dependencies))
  manifest.dependencies[name] = local.get(name);
for (const name of Object.keys(manifest.overrides))
  manifest.overrides[name] = local.get(name);
writeFileSync(packagePath, JSON.stringify(manifest, null, 2) + "\n");
const npm = Bun.which("npm");
assert(npm);
const argv = [
  npm,
  "install",
  "--package-lock-only",
  "--offline",
  "--ignore-scripts",
  "--no-bin-links",
  "--no-audit",
  "--no-fund",
  "--cache",
  join(root, "cache"),
];
const locked = Bun.spawnSync(argv, {
  cwd: project,
  stdout: "pipe",
  stderr: "pipe",
  timeout: 120_000,
});
writeFileSync(
  join(root, "graph-local-lock-command.json"),
  JSON.stringify(
    {
      argv,
      cwd: project,
      exit: locked.exitCode,
      stdout: locked.stdout.toString(),
      stderr: locked.stderr.toString(),
    },
    null,
    2,
  ) + "\n",
);
assert.equal(locked.exitCode, 0, locked.stderr.toString());
const digest = (bytes: Uint8Array) => [
  ...createHash("sha256").update(bytes).digest(),
];
// The existing SDK producer/custody owner derived these per-file expectations
// from all three actual packed archives, not from the installed source tree.
const trees = ["electron", "@keld/api", "@keld/kipc"].map((name) => {
  const files = custody.packedRuntimeFiles[name] as Record<
    string,
    { size: number; sha256: string }
  >;
  assert(files && files["package.json"]);
  const hasher = createHash("sha256");
  for (const path of Object.keys(files).sort((a, b) =>
    Buffer.compare(Buffer.from(a), Buffer.from(b)),
  )) {
    const file = files[path]!;
    hasher.update(path).update(Buffer.from([0]));
    hasher.update(String(file.size)).update(Buffer.from([0]));
    hasher.update(Buffer.from(file.sha256, "hex")).update(Buffer.from([0]));
  }
  return [...hasher.digest()];
});
writeFileSync(
  join(root, "graph-expectations.json"),
  JSON.stringify(
    {
      expected_package_sha256: digest(
        readFileSync(join(project, "package.json")),
      ),
      expected_lock_sha256: digest(
        readFileSync(join(project, "package-lock.json")),
      ),
      expected_package_trees: trees,
    },
    null,
    2,
  ) + "\n",
);
console.log(
  JSON.stringify({
    project,
    scope: "owned fixture; actual packed providers; original application unrun",
  }),
);
