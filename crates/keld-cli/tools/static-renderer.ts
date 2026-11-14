/** Proposed first renderer cell: inert self-contained HTML only.
 * Uses Bun's HTML parser, never regex or a mirrored HTML tokenizer. No scripts,
 * arbitrary CSS, forms, frames, images, links or URL-bearing attributes. This intentionally
 * excludes interactive/resource-dependent renderers rather than omitting files.
 * It is format admission, not a replacement for WebView runtime policy.
 */
const TAGS = new Set([
  "html",
  "head",
  "title",
  "body",
  "main",
  "section",
  "article",
  "header",
  "footer",
  "div",
  "span",
  "p",
  "br",
  "hr",
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6",
  "ul",
  "ol",
  "li",
  "dl",
  "dt",
  "dd",
  "strong",
  "em",
  "b",
  "i",
  "u",
  "s",
  "small",
  "sub",
  "sup",
  "pre",
  "code",
  "blockquote",
  "q",
  "table",
  "thead",
  "tbody",
  "tfoot",
  "tr",
  "td",
  "th",
  "caption",
]);
const ATTRIBUTES = new Set([
  "id",
  "class",
  "title",
  "lang",
  "dir",
  "role",
  "aria-label",
]);
const SCAFFOLD_STYLE =
  "<style>\n      html {\n        background: #000;\n        color-scheme: dark;\n      }\n      body {\n        font-family: system-ui, sans-serif;\n        margin: 2rem;\n        background: #000;\n        color: #fff;\n      }\n    </style>";
const SCAFFOLD_META = '<meta charset="utf-8" />';
const META_MARKER = "<!--keld-validation-meta-4b73-->";
const STYLE_MARKER = "<!--keld-validation-style-4b73-->";
const MAX_RENDERER = 16 * 1024 * 1024;

export async function admitStaticRenderer(
  input: Uint8Array,
): Promise<Uint8Array> {
  if (input.length === 0 || input.length > MAX_RENDERER)
    throw new Error("Renderer size outside supported bounds");
  // Defensive snapshot first: admission and returned bytes are the same input.
  const bytes = input.slice();
  const text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  if (text.includes(META_MARKER) || text.includes(STYLE_MARKER))
    throw new Error("Reserved renderer validation marker");
  let sawElement = false;
  let metaCount = 0;
  let styleCount = 0;
  const rewriter = new HTMLRewriter().on("*", {
    element(element) {
      sawElement = true;
      const tag = element.tagName.toLowerCase();
      // The parser identifies whole elements. Roundtrip against exact canonical
      // bytes below rejects alternate attributes, duplicate attrs and any CSS
      // mutation even if the parser normalizes them. Never return this projection.
      if (tag === "meta") {
        if (++metaCount > 1) throw new Error("Duplicate scaffold meta");
        element.replace(META_MARKER, { html: true });
        return;
      }
      if (tag === "style") {
        if (++styleCount > 1) throw new Error("Duplicate scaffold style");
        element.replace(STYLE_MARKER, { html: true });
        return;
      }
      if (!TAGS.has(tag))
        throw new Error("Renderer element outside inert cell");
      for (const [name] of element.attributes) {
        if (!ATTRIBUTES.has(name.toLowerCase()))
          throw new Error("Renderer attribute outside inert cell");
      }
    },
  });
  // Drain to completion so errors at the end cannot be missed. Do not serialize
  // a changed renderer or execute it. Returned original bytes remain literal.
  const reader = rewriter
    .transform(
      new Response(text, {
        headers: { "content-type": "text/html; charset=utf-8" },
      }),
    )
    .body!.getReader();
  const decoder = new TextDecoder("utf-8", { fatal: true });
  let projected = "";
  try {
    while (true) {
      const { value, done } = await reader.read();
      if (done) break;
      projected += decoder.decode(value, { stream: true });
      if (projected.length > MAX_RENDERER + 1024)
        throw new Error("Renderer projection bound");
    }
    projected += decoder.decode();
  } finally {
    reader.releaseLock();
  }
  const restored = projected
    .replaceAll(META_MARKER, SCAFFOLD_META)
    .replaceAll(STYLE_MARKER, SCAFFOLD_STYLE);
  if (restored !== text)
    throw new Error(
      "Renderer scaffold meta/style differs from exact canonical bytes",
    );
  if (!sawElement)
    throw new Error("Renderer must contain supported HTML elements");
  return bytes;
}
