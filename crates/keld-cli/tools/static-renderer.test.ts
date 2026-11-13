import { expect, test } from "bun:test";
import { admitStaticRenderer } from "./static-renderer";
import { scaffold, rendererNegativeCases } from "./renderer-fixtures.ts";
const bytes = (s: string) => new TextEncoder().encode(s);
test("inert old HTML preserves literal bytes", async () => {
  const input = bytes(
    '<!doctype html><html lang="en"><head><title>Hello</title></head><body><main id="app"><h1>Local app</h1><p>&lt;script&gt; text</p></main></body></html>',
  );
  expect(await admitStaticRenderer(input)).toEqual(input);
});
test("canonical scaffold preserves original markup", async () => {
  expect(await admitStaticRenderer(bytes(scaffold))).toEqual(bytes(scaffold));
});
for (const [index, [html, reason]] of rendererNegativeCases.entries())
  test(`renderer negative ${index}`, async () => {
    await expect(admitStaticRenderer(bytes(html))).rejects.toHaveProperty(
      "message",
      reason,
    );
  });
test("invalid UTF8 and empty input refuse", async () => {
  await expect(
    admitStaticRenderer(new Uint8Array([255])),
  ).rejects.toBeInstanceOf(TypeError);
  await expect(admitStaticRenderer(new Uint8Array())).rejects.toHaveProperty(
    "message",
    "Renderer size outside supported bounds",
  );
});
