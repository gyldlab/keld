import { expect, test } from "bun:test";

import * as windowModule from "./browser-window.ts";
import * as electron from "./index.ts";

test("dialog parent reader rejects forged and revoked objects without consulting properties", () => {
  const reader = Reflect.get(windowModule, "readDialogParentRef");
  expect(typeof reader).toBe("function");
  expect(Reflect.get(electron, "readDialogParentRef")).toBeUndefined();
  expect(Reflect.get(electron.default, "readDialogParentRef")).toBeUndefined();
  let propertyReads = 0;
  const forged = Object.create(windowModule.BrowserWindow.prototype);
  Object.defineProperty(forged, "id", {
    get() {
      propertyReads += 1;
      return 1;
    },
  });
  const revoked = Proxy.revocable(forged, {});
  revoked.revoke();
  for (const candidate of [
    forged,
    new Proxy(forged, {}),
    revoked.proxy,
    {},
    1,
  ]) {
    expect(() => reader(candidate)).toThrow();
  }
  expect(propertyReads).toBe(0);
});
