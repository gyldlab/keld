/**
 * W4 executable client contract over real Windows named pipes.
 *
 * Reuses the immutable S1 LifecycleLink, WorkerLink, codec and WindowSource.
 * The scripted Host is a test peer, not Windows Core admission or Host Ready.
 * Native sampling is opt-in and must run inside the root's serialized slot.
 */
import assert from "node:assert/strict";
import { describe, test } from "bun:test";
import {
  withFixture,
  type Fixture,
  type Notice,
} from "./subscribe_support/host.ts";

const root = process.env.W4_SHARED_ROOT ?? "";
const enabled = process.platform === "win32" && root.length > 0;

function modelFacts(dark = false) {
  return {
    primary_id: 17,
    displays: [
      {
        id: 17,
        bounds: { x: 0, y: 0, width: 1440, height: 900 },
        work_area: { x: 0, y: 24, width: 1440, height: 876 },
        scale_factor: 2,
      },
    ],
    should_use_dark_colors: dark,
  };
}
function value(notice: Notice): unknown {
  return (notice.snapshot as { value?: unknown }).value;
}
async function desktop(
  peer: Fixture,
  epoch: bigint,
  snapshot: unknown,
): Promise<void> {
  await peer.send(
    peer.t.FrameKind.Event,
    peer.t.WINDOW_CHANNEL,
    0,
    peer.codec.encodeWindowEvent({
      variant: "DesktopSnapshot",
      epoch,
      snapshot,
    }),
  );
}
async function accepted(peer: Fixture, correlation: number): Promise<void> {
  await peer.send(
    peer.t.FrameKind.Reply,
    peer.t.LIFECYCLE_CHANNEL,
    correlation,
    Uint8Array.of(1),
  );
}
async function rawReady(peer: Fixture): Promise<void> {
  await peer.send(
    peer.t.FrameKind.Event,
    peer.t.LIFECYCLE_CHANNEL,
    0,
    Uint8Array.of(0),
  );
}
async function assertPending(peer: Fixture): Promise<void> {
  await peer.fence();
  const state = await peer.request("checkpoint");
  assert.equal(
    state.settled,
    false,
    "valid Subscribe must remain pending before sample",
  );
  assert.equal(state.readyCount, 0, "no facade Ready before accepted replay");
  assert.equal(value(state), undefined, "no fabricated first desktop value");
}
async function firstSample(peer: Fixture, snapshot: unknown, epoch = 1n) {
  await peer.hello();
  const subscribe = await peer.subscribe();
  await assertPending(peer);
  await desktop(peer, epoch, snapshot);
  await accepted(peer, subscribe.header.corr);
  const settled = await peer.notice("settled");
  assert.deepEqual(
    value(settled),
    snapshot,
    "facts apply before Subscribe settles",
  );
  await rawReady(peer);
  assert.deepEqual(value(await peer.notice("ready")), snapshot);
  return subscribe;
}

describe.skipIf(!enabled)(
  "W4 shared client contract; scripted Host, real named pipe",
  () => {
    test("valid early Subscribe stays pending and receives facts before correlated acceptance", async () => {
      await withFixture(root, async (peer) => {
        await firstSample(peer, modelFacts());
        const before = peer.incoming.filter(
          (f) => f.header.kind === peer.t.FrameKind.Call,
        ).length;
        const reads = await peer.request("getters", { count: 1000 });
        assert.equal(reads.hostCalls, 0);
        assert.equal(reads.callbacksDuringReads, 0);
        assert.deepEqual(reads.snapshot, modelFacts());
        await peer.fence();
        assert.equal(
          peer.incoming.filter((f) => f.header.kind === peer.t.FrameKind.Call)
            .length,
          before,
          "local getters cannot add a Host CALL frame",
        );
      });
    }, 10000);

    test("adversarial raw Ready before the sample cannot bypass pending Subscribe", async () => {
      await withFixture(root, async (peer) => {
        await peer.hello();
        const subscribe = await peer.subscribe();
        await rawReady(peer);
        await assertPending(peer);
        await desktop(peer, 1n, modelFacts());
        await accepted(peer, subscribe.header.corr);
        assert.deepEqual(value(await peer.notice("settled")), modelFacts());
        const ready = await peer.notice("ready");
        assert.equal(ready.readyCount, 1);
        assert.deepEqual(value(ready), modelFacts());
      });
    }, 10000);

    test("definitive source failure settles the pending correlation with the existing typed error", async () => {
      await withFixture(root, async (peer) => {
        await peer.hello();
        const subscribe = await peer.subscribe();
        await assertPending(peer);
        await peer.send(
          peer.t.FrameKind.Err,
          peer.t.LIFECYCLE_CHANNEL,
          subscribe.header.corr,
          peer.t.encodeCallError(
            "KELD-CORE-049",
            "KELD-CORE-049: native desktop source unavailable; inspect native sampling.",
          ),
        );
        assert.equal((await peer.notice("rejected")).code, "KELD-CORE-049");
        const retired = await peer.request("retire");
        assert.equal(retired.terminal, "KELD-IPC-022");
        const state = await peer.request("checkpoint");
        assert.equal(state.readyCount, 0);
        assert.equal(state.settledCode, "KELD-CORE-049");
      });
    }, 10000);

    test("the original 5000 ms caller deadline retires an unsatisfied subscription", async () => {
      await withFixture(root, async (peer) => {
        assert.equal(peer.t.APP_LINK_IO_DEADLINE_MS, 5000);
        await peer.hello();
        await peer.subscribe();
        await assertPending(peer);
        const rejected = await peer.notice("rejected");
        assert.equal(rejected.code, "KELD-IPC-006");
        assert.ok(
          typeof rejected.elapsedMs === "number" &&
            rejected.elapsedMs >= 4900 &&
            rejected.elapsedMs < 7000,
          "observe the original 5 s call bound, never a replacement or renewed deadline",
        );
        assert.equal((await peer.request("retire")).terminal, "KELD-IPC-022");
        const state = await peer.request("checkpoint");
        assert.equal(state.readyCount, 0);
        assert.equal(state.settledCode, "KELD-IPC-006");
      });
    }, 12000);

    test("a regressing desktop epoch on the real link fails closed without replacing the latest value", async () => {
      await withFixture(
        root,
        async (peer) => {
          await firstSample(peer, modelFacts(true), 2n);
          await peer.fence();
          const before = (await peer.request("checkpoint")).factCount;
          await desktop(peer, 1n, modelFacts(false));
          assert.equal((await peer.notice("dead")).code, "KELD-IPC-005");
          const state = await peer.request("checkpoint");
          assert.equal(
            (state.snapshot as { error: string }).error,
            "KELD-IPC-005",
          );
          assert.equal(state.factCount, before, "stale fact must not dispatch");
          assert.equal(state.readyCount, 1);
        },
        true,
        "kipc WorkerLink: KELD-IPC-005: the state applier for channel 5 threw: mirror decode or apply failed. Check the source codec and state contract before restarting the role.\n",
      );
    }, 10000);

    test("ordered update survives role retirement and a fresh role receives the latest replay", async () => {
      const latest = modelFacts(true);
      await withFixture(root, async (peer) => {
        await firstSample(peer, modelFacts(false));
        await desktop(peer, 2n, latest);
        await peer.fence();
        assert.deepEqual(value(await peer.request("checkpoint")), latest);
        const retired = await peer.request("retire");
        assert.equal(retired.terminal, "KELD-IPC-022");
        assert.equal(
          retired.staleApply,
          "KELD-IPC-022",
          "old role cannot revive its retired source",
        );
      });
      // Fresh process = fresh WorkerLink generation. The test Host retains only
      // this scripted latest value; this does not exercise Core install_generation.
      await withFixture(root, async (peer) => {
        await peer.hello();
        const subscribe = await peer.subscribe();
        await assertPending(peer);
        await desktop(peer, 2n, latest);
        await peer.send(
          peer.t.FrameKind.Event,
          peer.t.WINDOW_CHANNEL,
          0,
          peer.codec.encodeWindowEvent({
            variant: "Created",
            window: { id: 7, incarnation: 300n },
            state: {
              bounds: { x: 20, y: 30, width: 640, height: 480 },
              focused: false,
              maximized: false,
              full_screen: false,
            },
            pending_close: null,
          }),
        );
        await accepted(peer, subscribe.header.corr);
        assert.deepEqual(value(await peer.notice("settled")), latest);
        await rawReady(peer);
        assert.deepEqual(value(await peer.notice("ready")), latest);
        await peer.fence();
        const facts = peer.inbox.history.filter((n) => n.kind === "fact");
        assert.deepEqual(
          facts.map((n) => n.variant),
          ["DesktopSnapshot", "Created"],
        );
        assert.equal(facts[0]!.epoch, "2");
      });
    }, 12000);

    test("an unselected role refuses a window event through its unchanged receive table", async () => {
      await withFixture(
        root,
        async (peer) => {
          await peer.hello();
          await peer.notice("settled");
          assert.equal(
            peer.incoming.filter((f) => f.header.kind === peer.t.FrameKind.Call)
              .length,
            0,
          );
          await desktop(peer, 1n, modelFacts());
          assert.equal((await peer.notice("dead")).code, "KELD-IPC-005");
          const state = await peer.request("checkpoint");
          assert.equal(
            (state.snapshot as { hasSource: boolean }).hasSource,
            false,
          );
          assert.equal(state.factCount, 0);
        },
        false,
        "kipc transport Worker: KELD-IPC-022: KELD-IPC-005: frame kind is not declared by the session policy\n",
      );
    }, 10000);

    test("a wrong HELLO token fails before any subscription or mirror is installed", async () => {
      await withFixture(root, async (peer) => {
        await peer.hello(false);
        assert.equal((await peer.notice("rejected")).code, "KELD-IPC-007");
        const state = await peer.request("checkpoint");
        assert.equal(
          (state.snapshot as { hasSource: boolean }).hasSource,
          false,
        );
        assert.equal(
          peer.incoming.filter((f) => f.header.kind === peer.t.FrameKind.Call)
            .length,
          0,
        );
      });
    }, 10000);
  },
);

test.skipIf(!enabled || process.env.W4_RUN_NATIVE !== "1")(
  "W4 native first sample is taken after a valid held Subscribe and precedes facade Ready",
  async () => {
    const { sampleNative } = await import("./native_adapter.ts");
    await withFixture(root, async (peer) => {
      await peer.hello();
      const subscribe = await peer.subscribe();
      const subscribedAt = performance.now();
      await assertPending(peer);
      const sampleStartedAt = performance.now();
      const captured = await sampleNative();
      const sampleCompletedAt = performance.now();
      await desktop(peer, 1n, captured.facts);
      await accepted(peer, subscribe.header.corr);
      const settled = await peer.notice("settled");
      assert.deepEqual(value(settled), captured.facts);
      const settledAt = performance.now();
      await rawReady(peer);
      const ready = await peer.notice("ready");
      assert.deepEqual(value(ready), captured.facts);
      const readyAt = performance.now();
      assert.ok(
        subscribedAt <= sampleStartedAt && sampleStartedAt <= sampleCompletedAt,
      );
      assert.ok(sampleCompletedAt <= settledAt && settledAt <= readyAt);
      const receipt = {
        proof:
          "Windows native sampling plus existing client transport/mirror; scripted Host",
        coreAdmissionProven: false,
        globalSettingsChanged: false,
        pid: process.pid,
        monotonicMs: {
          subscribedAt,
          sampleStartedAt,
          sampleCompletedAt,
          settledAt,
          readyAt,
        },
        originalCallerDeadlineMs: peer.t.APP_LINK_IO_DEADLINE_MS,
        sample: captured,
      };
      if (process.env.W4_SUBSCRIBE_NATIVE_RECEIPT) {
        await Bun.write(
          process.env.W4_SUBSCRIBE_NATIVE_RECEIPT,
          JSON.stringify(receipt, null, 2) + "\n",
        );
      }
      console.log(
        "W4_NATIVE_SUBSCRIBE " +
          JSON.stringify({
            order:
              "Subscribe -> native sample -> DesktopSnapshot -> Subscribed -> raw Ready -> facade Ready",
            originalCallerDeadlineMs: peer.t.APP_LINK_IO_DEADLINE_MS,
            sampleMs: sampleCompletedAt - sampleStartedAt,
            coreAdmissionProven: false,
          }),
      );
    });
  },
  15000,
);
