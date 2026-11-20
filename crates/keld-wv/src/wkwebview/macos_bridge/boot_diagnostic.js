// Fixed native-owned private collector; page attribute metadata remains untrusted.
(() => {
  "use strict";
  const dom = document,
    attribute = "data-keld-boot-diagnostic-v2";
  const native = window.webkit.messageHandlers.__keld_app_world;
  const stringify = JSON.stringify,
    apply = Reflect.apply,
    get = Element.prototype.getAttribute;
  const rawUA = navigator.userAgent;
  let ua = typeof rawUA === "string" ? rawUA.slice(0, 512) : "";
  if (ua.length && /[\uD800-\uDBFF]/.test(ua[ua.length - 1]))
    ua = ua.slice(0, -1);
  let credential,
    active = true,
    domLoaded = false,
    loaded = false,
    reports = 0,
    watched;
  const metadata = __KELD_BOOT_DIAGNOSTIC_COMMON__((kind) => snapshot(kind));
  const observer = new MutationObserver(() => snapshot("page"));
  function observePage() {
    const root = dom.documentElement;
    if (root && root !== watched) {
      observer.disconnect();
      observer.observe(root, {
        attributes: true,
        attributeFilter: [attribute],
      });
      watched = root;
    }
    return root;
  }
  function snapshot(phase) {
    if (!active || !credential || reports >= 16) return;
    const root = observePage();
    const page = metadata.readAttribute(
      root ? apply(get, root, [attribute]) : null,
    );
    const own = metadata.state();
    reports++;
    native.postMessage(
      stringify({
        v: 0,
        kind: "origin-probe",
        document: credential,
        href: location.href,
        origin: location.origin,
        protocol: location.protocol,
        hostname: location.hostname,
        port: location.port,
        boot: {
          v: 1,
          phase,
          ua,
          ua_truncated: typeof rawUA === "string" && rawUA.length > ua.length,
          ready_state: dom.readyState,
          dom_loaded: domLoaded,
          loaded,
          records: own.records,
          dropped: own.dropped,
          page,
        },
      }),
    );
  }
  metadata.listen();
  dom.addEventListener(
    "DOMContentLoaded",
    () => {
      domLoaded = true;
      observePage();
      snapshot("dom");
    },
    { once: true },
  );
  window.addEventListener(
    "load",
    () => {
      loaded = true;
      snapshot("load");
    },
    { once: true },
  );
  window.addEventListener(
    "pagehide",
    () => {
      active = false;
      credential = undefined;
      observer.disconnect();
    },
    { once: true },
  );
  Object.defineProperty(globalThis, "__keldBootDiagnosticBind", {
    value: (document) => {
      if (
        credential !== undefined ||
        typeof document !== "string" ||
        !/^[0-9a-f]{32}$/.test(document)
      )
        return;
      credential = document;
      snapshot("bound");
    },
    configurable: false,
    writable: false,
  });
})();
