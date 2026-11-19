import { expect, test } from "bun:test";
import { resolve } from "node:path";

const entry = resolve(import.meta.dir, "menu.ts");
const facts = {
  schema: 8,
  kind: "development",
  name: "Selected Windows app",
  root: "\\\\?\\D:\\verified-stage",
  paths: {},
  originalResourcePolicy: {
    contract: "gh473-original-editor-offline/v1",
    graphSha256: "sha256:" + "ab".repeat(32),
    policySha256: "sha256:" + "cd".repeat(32),
    windowKey: "editor",
    selector: "original-editor-offline",
  },
};

function observe(record: unknown, selected = true) {
  return Bun.spawnSync({
    cmd: [
      process.execPath,
      "--no-install",
      "--no-env-file",
      "--define=__KELD_WINDOWS_HOST__:" + selected,
      "--define=__KELD_RUNTIME_PROCESS_FACTS__:" + selected,
      "--define=__KELD_HOST_LAUNCH_FACTS__:" + (JSON.stringify(record) ?? "undefined"),
      "-e",
      `globalThis["__KELD_HOST_LAUNCH_FACTS__"] = { schema: 8, windowsApplicationMenu: "absent-v1" };
      const { Menu } = await import(${JSON.stringify(entry)});
      const observe = (action) => {
        try { return action() === undefined ? "void" : "unexpected value"; }
        catch (error) { return [error.code, error.name, error.member, error.ticket, error.kind]; }
      };
      const nonNull = new Proxy({}, { get() { throw new Error("nonnull argument read"); } });
      console.log(JSON.stringify([
        observe(() => Menu.setApplicationMenu(null)),
        observe(() => Menu.setApplicationMenu(nonNull)),
        observe(() => new Menu())
      ]));`,
    ],
    env: { ...process.env, KELD_NATIVE_WINDOW_FACADE: selected ? "1" : "0" },
    stdout: "pipe",
    stderr: "pipe",
    timeout: 4_000,
  });
}

const refusal = (member: string, kind: "method" | "constructor") => [
  "KELD-COMPAT-013",
  "UnsupportedMemberError",
  member,
  "GH-556",
  kind,
];
const unqualified = refusal("Menu.setApplicationMenu(null)", "method");
const nonNull = refusal("Menu.setApplicationMenu", "method");
const constructor = refusal("Menu", "constructor");

test("the attested schema eight menu-null call returns void", () => {
  const child = observe({ ...facts, windowsApplicationMenu: "absent-v1" });
  expect(child.exitCode, child.stderr.toString()).toBe(0);
  expect(child.stderr.toString()).toBe("");
  expect(JSON.parse(child.stdout.toString())).toEqual(["void", nonNull, constructor]);
});

test("absent fact, unselected launch and ambient decoys leave the typed refusal", () => {
  for (const child of [observe(facts), observe(undefined), observe(undefined, false)]) {
    expect(child.exitCode, child.stderr.toString()).toBe(0);
    expect(child.stderr.toString()).toBe("");
    expect(JSON.parse(child.stdout.toString())).toEqual([unqualified, nonNull, constructor]);
  }
});

test("malformed, older and foreign attestation fails at immutable capture", () => {
  const attested = { ...facts, windowsApplicationMenu: "absent-v1" };
  for (const child of [
    observe({ ...facts, windowsApplicationMenu: "absent-v2" }),
    observe({ ...attested, schema: 3 }),
    observe(attested, false),
  ]) {
    expect(child.exitCode).not.toBe(0);
    expect(child.stderr.toString()).toContain("KELD-IPC-029");
    expect(child.stdout.toString()).toBe("");
  }
});
