import { describe, expect, test } from "bun:test";

describe("@keld/api lifecycle floor", () => {
  test("last-window-closed subscription is locally idempotent and lazy", async () => {
    const { app } = await import("./app.ts");
    const unsubscribe = app.onLastWindowClosed(() => {});
    expect(typeof unsubscribe).toBe("function");
    expect(() => unsubscribe()).not.toThrow();
    expect(() => unsubscribe()).not.toThrow();
  });

  test("quit exposes an unset host link as KELD-CLI-010, not the HELLO-token code", async () => {
    const previous = process.env.KELD_APP_LINK;
    delete process.env.KELD_APP_LINK;
    try {
      const { app } = await import("./app.ts");
      const rejection: unknown = await app.quit().then(
        () => undefined,
        (err: unknown) => err,
      );
      expect(rejection).toBeInstanceOf(Error);
      const message = (rejection as Error).message;
      // GH-1118: the registry meaning of KELD-CLI-010 is exactly this condition.
      expect(message).toContain("KELD-CLI-010");
      expect(message).toContain("KELD_APP_LINK is unset");
      // Negative control: KELD-IPC-007 is reserved for a rejected HELLO session token.
      expect(message).not.toContain("KELD-IPC-007");
    } finally {
      if (previous === undefined) delete process.env.KELD_APP_LINK;
      else process.env.KELD_APP_LINK = previous;
    }
  });
});
