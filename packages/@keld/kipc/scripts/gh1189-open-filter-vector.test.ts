import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

import { encodeDialogRequest } from "../../api/src/dialog.generated.ts";

test("W5 pinned Open options match the complete Rust-owned filter vector", () => {
  // Desktop 2edf9fb9 / webapp 24b76c2c chooseFileEntry. Directory, nonce and
  // parent are controlled fixture inputs, not a native/app correlation claim.
  const request = {
    variant: "Open" as const,
    nonce: 1n,
    parent: null,
    options: {
      default_path: "C:\\Users\\fixture\\Documents",
      filters: [
        {
          name: "draw.io Diagrams",
          extensions: ["drawio", "xml", "png", "svg", "html", "pdf"],
        },
        { name: "VSDX Documents", extensions: ["vsdx"] },
        { name: "Mermaid", extensions: ["mmd", "mermaid"] },
        { name: "All Files", extensions: ["*"] },
      ],
      properties: ["OpenFile"] as const,
    },
  };
  const fixture: unknown = JSON.parse(
    readFileSync(
      new URL(
        "../../../../crates/keld-native/tests/fixtures/dialog_open_filters.json",
        import.meta.url,
      ),
      "utf8",
    ),
  );
  if (!Array.isArray(fixture)) {
    throw new Error("W5 fixture must be a JSON byte array");
  }
  const expected = fixture.map((byte: unknown) => {
    if (
      typeof byte !== "number" ||
      !Number.isInteger(byte) ||
      byte < 0 ||
      byte > 255
    ) {
      throw new Error("W5 fixture contains an invalid byte");
    }
    return byte;
  });
  expect([...encodeDialogRequest(request)]).toEqual(expected);
  const reordered = {
    ...request,
    options: {
      ...request.options,
      filters: [...request.options.filters].reverse(),
    },
  };
  expect([...encodeDialogRequest(reordered)]).not.toEqual(expected);
});
