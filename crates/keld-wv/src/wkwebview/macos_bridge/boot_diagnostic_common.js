function makeBootDiagnostic(onChange) {
  "use strict";
  const apply = Reflect.apply,
    descriptor = Object.getOwnPropertyDescriptor;
  const parse = JSON.parse;
  const getter = (type, key) => type && descriptor(type.prototype, key)?.get;
  const getters = {
    message: getter(globalThis.ErrorEvent, "message"),
    filename: getter(globalThis.ErrorEvent, "filename"),
    line: getter(globalThis.ErrorEvent, "lineno"),
    column: getter(globalThis.ErrorEvent, "colno"),
    directive: getter(
      globalThis.SecurityPolicyViolationEvent,
      "effectiveDirective",
    ),
    blocked: getter(globalThis.SecurityPolicyViolationEvent, "blockedURI"),
  };
  const read = (get, event) => {
    if (!get) return undefined;
    try {
      return apply(get, event, []);
    } catch {
      return undefined;
    }
  };
  const records = [];
  let dropped = 0,
    active = true;
  const number = (value) =>
    Number.isInteger(value) && value >= 0 && value <= 0xffffffff ? value : 0;
  const resource = (value) => {
    if (typeof value !== "string") return "other";
    const file = value.split(/[?#]/, 1)[0];
    if (file.endsWith("/boot-error-control.js")) return "inert-control";
    if (file.endsWith("/js/bootstrap.js")) return "bootstrap";
    if (file.endsWith("/js/app.min.js")) return "app-bundle";
    if (file.endsWith("/js/diagramly/ElectronApp.js")) return "electron-app";
    return "other";
  };
  const record = (
    kind,
    errorClass,
    where,
    line = 0,
    column = 0,
    directive = "other",
    blocked = "other",
  ) => {
    if (!active) return;
    if (records.length === 8) {
      dropped = Math.min(0xffffffff, dropped + 1);
      if (dropped === 1) onChange(kind);
      return;
    }
    records.push(
      Object.freeze({
        kind,
        class: errorClass,
        resource: where,
        line: number(line),
        column: number(column),
        directive,
        blocked,
      }),
    );
    onChange(kind);
  };
  const closed = (value, keys) =>
    value !== null &&
    typeof value === "object" &&
    !Array.isArray(value) &&
    Object.keys(value).length === keys.length &&
    Object.keys(value).every((key) => keys.includes(key));
  const choices = (value, allowed) =>
    typeof value === "string" && allowed.includes(value);
  const bounded = (value) =>
    Number.isInteger(value) && value >= 0 && value <= 0xffffffff;
  const clean = (row) => {
    if (
      !closed(row, [
        "kind",
        "class",
        "resource",
        "line",
        "column",
        "directive",
        "blocked",
      ]) ||
      !choices(row.kind, ["js-error", "resource-error", "rejection", "csp"]) ||
      !choices(row.class, [
        "TypeError",
        "ReferenceError",
        "SyntaxError",
        "RangeError",
        "Error",
        "unknown",
      ]) ||
      !choices(row.resource, [
        "bootstrap",
        "app-bundle",
        "electron-app",
        "inert-control",
        "other",
      ]) ||
      !bounded(row.line) ||
      !bounded(row.column) ||
      !choices(row.directive, ["script", "style", "connect", "other"]) ||
      !choices(row.blocked, ["inline", "eval", "source", "other"])
    )
      return undefined;
    return {
      kind: row.kind,
      class: row.class,
      resource: row.resource,
      line: row.line,
      column: row.column,
      directive: row.directive,
      blocked: row.blocked,
    };
  };
  return Object.freeze({
    listen() {
      window.addEventListener(
        "error",
        (event) => {
          const message = read(getters.message, event);
          const errorClass =
            typeof message === "string"
              ? (/^(?:Uncaught\s+)?(TypeError|ReferenceError|SyntaxError|RangeError|Error)(?::|\b)/.exec(
                  message,
                )?.[1] ?? "unknown")
              : "unknown";
          record(
            typeof message === "string" ? "js-error" : "resource-error",
            errorClass,
            resource(read(getters.filename, event)),
            read(getters.line, event),
            read(getters.column, event),
          );
        },
        true,
      );
      window.addEventListener(
        "unhandledrejection",
        () => record("rejection", "unknown", "other"),
        true,
      );
      document.addEventListener(
        "securitypolicyviolation",
        (event) => {
          const value = read(getters.directive, event),
            uri = read(getters.blocked, event);
          const directive =
            typeof value === "string" && value.startsWith("script-src")
              ? "script"
              : typeof value === "string" && value.startsWith("style-src")
                ? "style"
                : value === "connect-src"
                  ? "connect"
                  : "other";
          const blocked =
            uri === "inline"
              ? "inline"
              : uri === "eval"
                ? "eval"
                : typeof uri === "string" && uri.startsWith("keld-resource:")
                  ? "source"
                  : "other";
          record("csp", "unknown", "other", 0, 0, directive, blocked);
        },
        true,
      );
      window.addEventListener(
        "pagehide",
        () => {
          active = false;
        },
        { once: true },
      );
    },
    state() {
      return { v: 1, records: records.slice(), dropped };
    },
    readAttribute(raw, parseInput = parse) {
      const empty = (status) => ({ status, records: [], dropped: 0 });
      if (raw === null) return empty("absent");
      if (typeof raw !== "string") return empty("malformed");
      if (raw.length > 4096) return empty("oversize");
      let value;
      try {
        value = parseInput(raw);
      } catch {
        return empty("malformed");
      }
      if (
        !closed(value, ["v", "records", "dropped"]) ||
        value.v !== 1 ||
        !Array.isArray(value.records) ||
        value.records.length > 8 ||
        !bounded(value.dropped)
      )
        return empty("malformed");
      const rows = [];
      for (const row of value.records) {
        const copied = clean(row);
        if (!copied) return empty("malformed");
        rows.push(copied);
      }
      return { status: "ok", records: rows, dropped: value.dropped };
    },
  });
}
