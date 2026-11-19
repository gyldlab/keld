import { readFileSync } from "node:fs";
import { expect, test } from "bun:test";
import {
  assertCompatControlArtifactFresh,
  renderCompatControlBinding,
} from "./echo-codegen.ts";

const rust = readFileSync(
  new URL("../../../../crates/keld-ipc/src/compat_control.rs", import.meta.url),
  "utf8",
);
const artifact = readFileSync(
  new URL("../src/compat-control.generated.ts", import.meta.url),
);

test("the canonical compat schema renders the committed artifact", () => {
  assertCompatControlArtifactFresh(rust, artifact);
  expect(() =>
    assertCompatControlArtifactFresh(rust, Buffer.from("stale")),
  ).toThrow("committed compat-control.generated.ts is stale");
});

test("the compat generator rejects ownership, field, and variant order drift", () => {
  const alias =
    "pub type CompatControlEvent = CompatControlEventValue<String, Vec<u8>>;";
  for (const changed of [
    rust.replace(alias, alias + "\n" + alias),
    rust.replace(alias, "#[cfg(windows)]\n" + alias),
    rust.replace(
      alias,
      "pub type CompatControlEvent = CompatControlEventValue<Vec<u8>, String>;",
    ),
    rust.replace(
      "CompatControlEventValue<S, P> {",
      "CompatControlEventValue<S = String, P = Vec<u8>> {",
    ),
    rust.replace("        url: S,", "        url: String,"),
    rust.replace("        origin: S,", "        origin: P,"),
    rust.replace("        topic: S,", "        topic: String,"),
    rust.replace("        payload: P,", "        payload: Vec<u8>,"),
    rust.replace(
      "        message: u32,",
      "        window_id: u32,\n        message: u32,",
    ),
    rust.replace(
      "    DocumentRetired {",
      "    DocumentBound,\n    DocumentRetired {",
    ),
    rust
      .replace("        url: S,", "        origin: S,")
      .replace(
        /(        origin: S,)([\s\S]*?)        origin: S,/,
        "$1$2        url: S,",
      ),
    rust
      .replace("    Consumed,", "    NoListener,")
      .replace(
        /(    NoListener,)([\s\S]*?)    NoListener,/,
        "$1$2    Consumed,",
      ),
  ]) {
    expect(changed).not.toBe(rust);
    expect(() => renderCompatControlBinding(changed)).toThrow("echo codegen");
  }
});

test("the compat generator requires only the exact bounded field deserializers", () => {
  const topic =
    '        #[serde(deserialize_with = "deserialize_compat_topic")]';
  const payload =
    '        #[serde(deserialize_with = "deserialize_compat_payload")]';
  const diagnostic =
    '        #[serde(deserialize_with = "deserialize_compat_diagnostic")]';
  for (const changed of [
    rust.replace(topic + "\n", ""),
    rust.replace(topic, payload),
    rust.replace(diagnostic, diagnostic + "\n" + diagnostic),
    rust.replace(payload, "        #[serde(skip_deserializing)]"),
    rust.replace("        topic: S,", topic + "\n        topic: S,"),
  ]) {
    expect(changed).not.toBe(rust);
    expect(() => renderCompatControlBinding(changed)).toThrow("echo codegen");
  }
});
