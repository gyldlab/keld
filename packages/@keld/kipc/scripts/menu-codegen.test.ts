import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { renderMenuBinding } from "./menu-codegen.ts";

const source = readFileSync(
  resolve(import.meta.dir, "../../../../crates/keld-native/src/menu.rs"),
  "utf8",
);
test("GH480 incumbent generator derives one role table and refuses unreviewed schema/bounds changes", () => {
  const output = renderMenuBinding(source);
  expect(output).toContain(
    '"hideothers": Object.freeze({"id":1,"label":"Hide Others","key":"h","modifiers":5',
  );
  expect(output).toContain(
    "export type MenuRoleName = keyof typeof MENU_ROLES",
  );
  expect(() =>
    renderMenuBinding(source.replace("pub depth: u8,", "pub depth: u16,")),
  ).toThrow("schema differs");
  expect(() =>
    renderMenuBinding(
      source.replace("MAX_MENU_DEPTH: u8 = 8", "MAX_MENU_DEPTH: u8 = 9"),
    ),
  ).toThrow("unreviewed bounds");
  const changed = renderMenuBinding(
    source.replace(
      'Copy, "copy", "Copy", "c", 1;',
      'Copy, "copy", "Copy", "x", 1;',
    ),
  );
  expect(changed).toContain(
    '"copy": Object.freeze({"id":8,"label":"Copy","key":"x"',
  );
  expect(changed).not.toBe(output);
});
