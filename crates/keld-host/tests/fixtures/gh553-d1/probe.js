// Values below are read from actual native responses and the actual original
// preload's effects. The Rust scenario supplies independent exact AX oracles.
(async () => {
  const label = (id, value) => {
    document.getElementById(id).textContent = value;
  };
  try {
    const data = await fetch("samples.bin");
    if (!data.ok) throw new Error("sample response " + data.status);
    const bytes = new Uint8Array(await data.arrayBuffer());
    const wasm = await fetch("answer.wasm");
    if (!wasm.ok) throw new Error("wasm response " + wasm.status);
    const instance = await WebAssembly.instantiate(await wasm.arrayBuffer());
    label(
      "resource",
      "D1 resources " +
        [...bytes].join(",") +
        " wasm=" +
        instance.instance.exports.answer(),
    );
  } catch (error) {
    label("resource", "D1 resources failed " + String(error));
  }
  try {
    const configuration = JSON.parse(
      localStorage.getItem(".configuration") || "null",
    );
    const methods = [
      "request",
      "registerMsgListener",
      "sendMessage",
      "listenOnce",
    ];
    label(
      "preload",
      "D1 preload " +
        configuration?.defaultAdaptiveColors +
        " " +
        window.process?.type +
        " bridge=" +
        methods.every((name) => typeof window.electron?.[name] === "function") +
        " bun=" +
        (typeof window.process?.versions?.bun === "string") +
        " require=" +
        typeof window.require,
    );
  } catch (error) {
    label("preload", "D1 preload failed " + String(error));
  }
  label(
    "inline",
    globalThis.__d1_inline_ran === undefined
      ? "D1 inline blocked"
      : "D1 inline executed",
  );
  try {
    const value = new URLSearchParams(location.search).get("outside");
    if (!value) throw new Error("outside URL missing");
    const outside = new URL(value);
    if (
      outside.protocol !== "file:" ||
      !outside.pathname.endsWith("/d1-outside.bin")
    )
      throw new Error("outside URL invalid");
    try {
      const response = await fetch(outside.href);
      label(
        "outside",
        "D1 outside " +
          (response.ok ? "readable" : "denied") +
          " requested=" +
          outside.href,
      );
    } catch {
      label("outside", "D1 outside denied requested=" + outside.href);
    }
  } catch (error) {
    label("outside", "D1 outside failed " + String(error));
  }
  try {
    const response = await fetch("index.html");
    if (!response.ok) throw new Error("index response " + response.status);
    label(
      "header",
      "D1 header " + response.headers.get("Content-Security-Policy"),
    );
  } catch (error) {
    label("header", "D1 header failed " + String(error));
  }
})();
