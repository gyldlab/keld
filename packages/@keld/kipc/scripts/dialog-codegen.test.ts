import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import * as generator from "./echo-codegen.ts";

const source = readFileSync(
  resolve(import.meta.dir, "../../../../crates/keld-native/src/dialog.rs"),
  "utf8",
);

test("dialog codec is generated from the exact native schema", () => {
  const render = Reflect.get(generator, "renderDialogBinding");
  expect(typeof render).toBe("function");
  const result: string = render(source);
  expect(result).toContain('variant: "Message"');
  expect(result).toContain('variant: "Cancel"');
  expect(result).toContain("encodeDialogRequest");
  expect(result).toContain("decodeDialogReply");
  expect(result).toContain('variant: "Open"');
  expect(result).toContain('variant: "Save"');
  expect(() =>
    render(source.replace("        nonce: u64,", "        nonce: u32,")),
  ).toThrow();
  const wrongOpenNonce = source.replace(
    /(\bOpen\s*\{[\s\S]*?nonce:\s*)u64/,
    "$1u32",
  );
  expect(wrongOpenNonce).not.toBe(source);
  expect(() => render(wrongOpenNonce)).toThrow();
  expect(() =>
    render(
      source.replace(
        "pub enum OpenProperty {",
        "pub enum OpenPropertyRenamed {",
      ),
    ),
  ).toThrow();
  expect(() =>
    render(
      source.replace(
        "pub enum DialogRequestV1 {",
        '#[serde(tag = "type")]\npub enum DialogRequestV1 {',
      ),
    ),
  ).toThrow();
  expect(() =>
    render(
      source.replace(
        "#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\npub enum DialogRequestV1",
        '#[serde(tag = "type")]\n#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\npub enum DialogRequestV1',
      ),
    ),
  ).toThrow();
});
