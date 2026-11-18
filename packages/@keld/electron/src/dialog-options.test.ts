import { expect, test } from "bun:test";
import * as electron from "./index.ts";

function messageMethod(): (...args: unknown[]) => unknown {
  const facade = Reflect.get(electron, "dialog");
  expect(facade).toBeDefined();
  const method = Reflect.get(facade, "showMessageBox");
  expect(typeof method).toBe("function");
  // The public method is overloaded; retain its actual dynamic invocation
  // while the hostile-input fixture deliberately passes unknown values.
  return (...args: unknown[]) => Reflect.apply(method, facade, args);
}

test("Message synchronously rejects unsupported option shapes", () => {
  const show = messageMethod();
  expect(() => show({ message: "close?", bogus: undefined })).toThrow();
  expect(() => show({ message: "close?", noLink: null })).toThrow();
  expect(() => show({ message: "close?", checkboxChecked: false })).toThrow();
});

test("Message accepts source-exact options and present-undefined synchronously", () => {
  const show = messageMethod();
  const result = show({
    type: "question",
    title: "Confirm",
    message: "Unsaved changes",
    noLink: true,
    buttons: ["Save", "Cancel", "Discard Changes"],
    defaultId: 0,
    cancelId: 1,
    detail: undefined,
    checkboxLabel: undefined,
  });
  expect(result).toBeInstanceOf(Promise);
  void (result as Promise<unknown>).catch(() => {});
});

test("explicit null or undefined parent is absent, not a forged BrowserWindow", () => {
  const show = messageMethod();
  for (const missing of [null, undefined]) {
    const result = show(missing, { message: "No parent", buttons: ["OK"] });
    expect(result).toBeInstanceOf(Promise);
    void (result as Promise<unknown>).catch(() => {});
  }
  for (const nonparent of [{ id: 1 }, 0, false]) {
    expect(() => show(nonparent, { message: "forged parent" })).toThrow(
      TypeError,
    );
  }
});
