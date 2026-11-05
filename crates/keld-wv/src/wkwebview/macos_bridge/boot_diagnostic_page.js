// Fixed page-world metadata recorder: no port, credential, app global or function call.
(() => {
  "use strict";
  const attribute = "data-keld-boot-diagnostic-v2";
  const set = Element.prototype.setAttribute,
    apply = Reflect.apply,
    stringify = JSON.stringify;
  let active = true,
    writes = 0;
  const metadata = __KELD_BOOT_DIAGNOSTIC_COMMON__(() => publish());
  function publish() {
    const root = document.documentElement;
    if (!active || !root || writes >= 12) return;
    const raw = stringify(metadata.state());
    if (raw.length > 4096) return;
    writes++;
    apply(set, root, [attribute, raw]);
  }
  metadata.listen();
  document.addEventListener("DOMContentLoaded", publish, { once: true });
  window.addEventListener("load", publish, { once: true });
  window.addEventListener(
    "pagehide",
    () => {
      active = false;
    },
    { once: true },
  );
  publish();
})();
