// Review-only facade receipt guards, not authenticated transport evidence.
import { expect, test } from "bun:test";
import { fixture, token, turn } from "./gh451-terminal-quit-draft-fixture";

test("AC11: missing Prepare consumption holds authorization until explicit release", async () => {
  const x = fixture();
  const consume = x.host.consumeDisposition!;
  delete x.host.consumeDisposition;
  const publicQuit = x.coordinator.quit();
  const t = token();
  const prepared = { kind: "prepared" as const, token: t };
  x.prepares[0]!.resolve(prepared);
  await turn();
  expect(x.trace).toEqual(["before", "prepare"]);
  expect(x.coordinator.heldDisposition).toBe(prepared);
  await expect(publicQuit).rejects.toThrow("missing receipt acknowledgement");
  expect(await x.coordinator.reconcile()).toBe("unknown");
  expect(x.trace).toEqual(["before", "prepare"]);
  x.host.consumeDisposition = consume;
  const recovery = x.coordinator.reconcile();
  expect(x.trace).toEqual(["before", "prepare", "will", "commit"]);
  x.commits[0]!.reply.resolve("accepted");
  expect(await recovery).toBe("committed");
});

test("AC11: missing Retained consumption blocks a fresh conditional attempt", async () => {
  const x = fixture();
  const consume = x.host.consumeDisposition!;
  delete x.host.consumeDisposition;
  const publicQuit = x.coordinator.quit();
  const retained = { kind: "retained" as const };
  x.prepares[0]!.resolve(retained);
  await turn();
  expect(x.coordinator.heldDisposition).toBe(retained);
  await expect(publicQuit).rejects.toThrow("missing receipt acknowledgement");
  expect(x.coordinator.quit()).toBe(publicQuit);
  expect(x.prepares).toHaveLength(1);
  x.host.consumeDisposition = consume;
  expect(await x.coordinator.reconcile()).toBe("retained");
  const retry = x.coordinator.quit();
  expect(retry).not.toBe(publicQuit);
  expect(x.prepares).toHaveLength(2);
  x.prepares[1]!.resolve({ kind: "retained" });
  await expect(retry).rejects.toThrow("KELD-COMPAT-012");
});

test("AC11: known terminal acceptance does not require consumption callback", async () => {
  const x = fixture();
  const publicQuit = x.coordinator.quit();
  await x.prepared(0);
  delete x.host.consumeDisposition;
  x.commits[0]!.reply.resolve("accepted");
  expect(await publicQuit).toBeUndefined();
  expect(x.coordinator.disposition).toBe("committed");
  expect(x.trace).toEqual(["before", "prepare", "will", "commit"]);
  expect(x.late).toEqual([]);
});
