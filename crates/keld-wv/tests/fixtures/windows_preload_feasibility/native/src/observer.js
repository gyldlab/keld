// Test-only independent raw observer. Its documentId is an observation tag,
// never a capability, credential, grant, source identity, or product epoch.
(() => {
  if (window !== window.top) return;
  const port = window.chrome.webview;
  const post = port.postMessage.bind(port);
  const stringify = JSON.stringify.bind(JSON);
  const parse = JSON.parse.bind(JSON);
  const documentId = crypto.randomUUID();
  const initialHref = String(location.href);
  const report = (kind, extra) =>
    post(
      stringify({
        kind,
        documentId,
        initialHref,
        ...extra,
      }),
    );
  port.addEventListener("message", (event) => {
    let value = event.data;
    if (typeof value === "string") {
      try {
        value = parse(value);
      } catch {
        /* Preserve a non-JSON string. */
      }
    }
    const stamp =
      value && typeof value === "object" ? value.fixtureEpoch : undefined;
    report("probe-raw-delivery", {
      value,
      stampedEpoch: stamp === undefined ? null : stamp,
      stale: typeof stamp === "string" && stamp !== documentId,
    });
  });
  addEventListener(
    "DOMContentLoaded",
    () => {
      report("probe-dom-ready", { title: document.title });
    },
    { once: true },
  );
  addEventListener("error", (event) => {
    report("probe-page-error", {
      message: event.message,
      filename: event.filename,
      line: event.lineno,
    });
  });
  report("probe-created", {});
})();
