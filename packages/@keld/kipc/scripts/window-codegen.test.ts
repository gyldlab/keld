import { readFileSync } from "node:fs";
import { expect, test } from "bun:test";
import { renderWindowCodecBinding } from "./echo-codegen.ts";
const rust = readFileSync(
  new URL("../../../../crates/keld-ipc/src/window.rs", import.meta.url),
  "utf8",
);
const adapter = readFileSync(
  new URL("../../api/src/window-codec.ts", import.meta.url),
  "utf8",
);
test("Rust Window append-only extension is deterministic and artifact-bound", () => {
  expect(renderWindowCodecBinding(rust, adapter)).toBe(adapter);
  expect(renderWindowCodecBinding(rust, adapter)).toBe(
    renderWindowCodecBinding(rust, adapter),
  );
  const tampered = adapter.replace('case "LoadUrl":', 'case "LoadOther":');
  expect(tampered).not.toBe(adapter);
  expect(renderWindowCodecBinding(rust, tampered)).toBe(adapter);
});
test("Rust Window unknown field/type/ordinal changes are refused by the same generator", () => {
  const movedLoaded = rust
    .replace("    Loaded,\n", "")
    .replace(
      "pub enum WindowResponse {",
      "pub enum WindowResponse {\n    Loaded,",
    );
  for (const changed of [
    movedLoaded,
    rust.replace("    LoadUrl {", "    LoadOther {"),
    rust.replace(
      "        document_generation: u32,",
      "        document_generation: u64,",
    ),
    rust.replace("    Loaded,", ""),
  ])
    expect(() => renderWindowCodecBinding(changed, adapter)).toThrow(
      "echo codegen",
    );
});
