import { readFileSync } from "node:fs";
import { expect, test } from "bun:test";
import { renderCompatControlBinding } from "./echo-codegen.ts";

const rust = readFileSync(
  new URL("../../../../crates/keld-ipc/src/compat_control.rs", import.meta.url),
  "utf8",
);
const artifact = readFileSync(
  new URL("../src/compat-control.generated.ts", import.meta.url),
  "utf8",
);

test("borrowed Compat event schema keeps the exact generated wire artifact", () => {
  expect(renderCompatControlBinding(rust)).toBe(artifact);
  expect(renderCompatControlBinding(rust)).toBe(
    renderCompatControlBinding(rust),
  );
});

test("Compat borrowing admits only its exact canonical declaration and owned alias", () => {
  const alias =
    "pub type CompatControlEvent = CompatControlEventValue<String, Vec<u8>>;";
  for (const changed of [
    rust.replace(
      alias,
      "pub type CompatControlEvent = CompatControlEventValue<Vec<u8>, String>;",
    ),
    rust.replace(alias, alias + "\n" + alias),
    rust.replace(alias, "#[cfg(windows)]\n" + alias),
    rust.replace(
      "pub enum CompatControlEventValue<S, P> {",
      "pub enum CompatControlEventValue<S = String, P = Vec<u8>> {",
    ),
    rust.replace("        url: S,", "        url: String,"),
    rust.replace("        origin: S,", "        origin: P,"),
    rust.replace("        topic: S,", "        topic: String,"),
    rust.replace("        payload: P,", "        payload: Vec<u8>,"),
    rust.replace("        webview: u32,", "        webview: S,"),
  ]) {
    expect(changed).not.toBe(rust);
    expect(() => renderCompatControlBinding(changed)).toThrow("echo codegen");
  }
});
