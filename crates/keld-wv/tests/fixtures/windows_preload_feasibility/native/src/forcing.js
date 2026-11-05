// Page-generated tag is an observation oracle, never read by an admission rule.
(() => {
  const post = chrome.webview.postMessage.bind(chrome.webview);
  const stringify = JSON.stringify.bind(JSON);
  const tag = crypto.randomUUID();
  const variant = __VARIANT__;
  const burst = () => {
    for (let index = 0; index < __BURST__; index++) {
      post(
        stringify({
          kind: "forcing-burst",
          tag,
          index,
          href: String(location.href),
        }),
      );
    }
    post(stringify({ kind: "forcing-ended", tag }));
  };
  if (variant === "pagehide" || variant === "unload") {
    addEventListener(variant, burst, { once: true });
    post(stringify({ kind: "forcing-start", tag, trigger: variant }));
  } else if (variant === "busy") {
    setTimeout(() => {
      post(stringify({ kind: "forcing-start", tag, trigger: "busy" }));
      const end = performance.now() + __BUSY_MS__;
      while (performance.now() < end) {
        /* Deliberate bounded renderer contention. */
      }
      burst();
    }, 0);
  } else {
    burst();
    post(stringify({ kind: "forcing-start", tag, trigger: "no-force" }));
  }
  return tag;
})();
