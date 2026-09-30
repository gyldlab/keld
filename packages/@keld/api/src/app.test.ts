import { describe, expect, test } from "bun:test";

describe("@keld/api lifecycle floor", () => {
  test("last-window-closed subscription is locally idempotent and lazy", async () => {
    const { app } = await import("./app.ts");
    const unsubscribe = app.onLastWindowClosed(() => {});
    expect(typeof unsubscribe).toBe("function");
    expect(() => unsubscribe()).not.toThrow();
    expect(() => unsubscribe()).not.toThrow();
  });

  test("quit exposes missing host link as KELD-IPC-007", async () => {
    const previous = process.env.KELD_APP_LINK;
    delete process.env.KELD_APP_LINK;
    try {
      const { app } = await import("./app.ts");
      await expect(app.quit()).rejects.toThrow("KELD-IPC-007");
    } finally {
      if (previous === undefined) delete process.env.KELD_APP_LINK;
      else process.env.KELD_APP_LINK = previous;
    }
  });
});
