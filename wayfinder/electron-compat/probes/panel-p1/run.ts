// PANEL-P1 runner: `bun --no-install run.ts <spec> [runs]` or `bun --no-install run.ts report`.
// Spawns the scratch Rust host (real keld-ipc writer) and one Bun client per run,
// collects JSON lines, evaluates the semantic exit criteria, and writes
// runs/<spec>/<i>/{host,client}.jsonl plus runs/<spec>/rows.json.
// Synchronization is by the host's "listening" line and process exit only.
import { mkdirSync, rmSync, writeFileSync, readFileSync, existsSync, readdirSync } from "node:fs";

const P1 = import.meta.dir;
const HOST = `${P1}/host/target/release/p1host`;
const HARD_TIMEOUT_MS = 60_000;

type J = Record<string, any>;
interface Spec {
  host: string[];
  client: string;
  args: string[];
  evaluate(h: J[], c: J[], x: { hung: boolean; hostExit: number | null; clientExit: number | null; ms: number }): J;
}

const ev = (lines: J[], name: string) => lines.find((l) => l.ev === name);
const STALL = "write or credit wait >= 250 ms, or a host write error";

function stallFree(s: J | undefined): boolean {
  return !!s && s.write_stalls_ge_250ms === 0 && s.credit_stalls_ge_250ms === 0 && s.error === null;
}

const specs: Record<string, Spec> = {
  A: {
    host: ["--arm", "a"],
    client: "arm-a.ts",
    args: ["--arm", "a", "--park-ms", "12000"],
    evaluate(h, c, x) {
      const s = ev(h, "summary"), we = ev(h, "write_error"), p = ev(c, "parked"), aw = ev(c, "after_wake"), st = ev(h, "scenario_start");
      return {
        zero_link_stall: stallFree(s),
        no_ipc006: !s?.ipc006,
        events_in_order_after_wake: !!aw?.order?.inOrder && aw.order.received >= 10000,
        sync_reply_while_parked: false,
        bytes_accepted_before_stall: we?.bytes_before ?? null,
        frames_before_stall: we?.frames_before ?? null,
        writer_error: we?.error?.slice(0, 40) ?? null,
        writer_blocked_ms: we?.blocked_ms ?? null,
        error_after_scenario_start_ms: we && st ? (we.wall_us - st.wall_us) / 1000 : null,
        client_bytes_in_during_park: p?.bytes_in_during_park ?? null,
        client_events_after_wake: aw?.order?.received ?? null,
        client_reply_received: aw?.reply_received ?? null,
        no_hang: !x.hung,
      };
    },
  },
  B: {
    host: ["--arm", "b"],
    client: "arm-b-main.ts",
    args: ["--mode", "scenario", "--expect-events", "11000", "--second-link"],
    evaluate(h, c, x) {
      const s = ev(h, "summary"), w = ev(c, "woke"), rw = ev(h, "reply_written"), wr = ev(c, "worker_reply"), sl = ev(c, "second_link"), bd = ev(h, "burst_done"), e = ev(c, "end");
      return {
        zero_link_stall: stallFree(s) && s.events === 11000,
        no_ipc006: !!s && !s.ipc006,
        events_in_order_after_wake: !!w?.order?.inOrder && w.order.received === s?.events && w.order.received >= 10000,
        sync_reply_while_parked: !!w?.reply_ok,
        host_events_written: s?.events ?? null,
        host_bytes_written: s?.bytes ?? null,
        host_max_write_block_us: s?.max_block_us ?? null,
        burst_10k_write_ms: bd?.burst_ms ?? null,
        parked_ms: w?.parked_ms ?? null,
        worker_had_all_events_before_wake: !!wr && !!w && wr.events_in === 11000 && wr.last_event_wall_us <= w.wake_wall_us,
        reply_written_to_main_wake_ms: rw && w ? (w.wake_wall_us - rw.wall_us) / 1000 : null,
        ring_high_water_bytes: w?.ring_high_water_bytes ?? null,
        replay: w?.order ?? null,
        second_link_refused: sl?.refused ?? null,
        quit_after: e?.quit_reply ?? null,
        host_serve_returned: !!s?.serve_returned && !!s?.quit_replied,
        no_hang: !x.hung && x.clientExit === 0 && x.hostExit === 0,
      };
    },
  },
  C: {
    host: ["--arm", "c", "--credit-window", "64"],
    client: "arm-a.ts",
    args: ["--arm", "c", "--park-ms", "12000", "--credit", "64"],
    evaluate(h, c, x) {
      const s = ev(h, "summary"), aw = ev(c, "after_wake"), p = ev(c, "parked");
      return {
        zero_link_stall: stallFree(s),
        no_ipc006: !!s && !s.ipc006,
        events_in_order_after_wake: !!aw?.order?.inOrder && aw.order.received === s?.events && aw.order.received >= 10000,
        sync_reply_while_parked: false,
        host_events_written: s?.events ?? null,
        credit_waits: s?.credit_waits ?? null,
        credit_wait_max_ms: s ? s.credit_wait_max_us / 1000 : null,
        credit_stalls_ge_250ms: s?.credit_stalls_ge_250ms ?? null,
        host_max_write_block_us: s?.max_block_us ?? null,
        client_bytes_in_during_park: p?.bytes_in_during_park ?? null,
        reply_after_wake_ms: aw?.reply_after_wake_ms ?? null,
        replay: aw?.order ?? null,
        no_hang: !x.hung && x.clientExit === 0 && x.hostExit === 0,
      };
    },
  },
  "E-retire": {
    host: ["--arm", "e-retire", "--retire-after-ms", "2000"],
    client: "arm-b-main.ts",
    args: ["--mode", "retire"],
    evaluate(h, c, x) {
      const s = ev(h, "summary"), w = ev(c, "woke"), r = ev(h, "retire");
      return {
        zero_link_stall: stallFree(s),
        no_ipc006: !!s && !s.ipc006,
        events_in_order_after_wake: !!w?.order?.inOrder && w.order.received === r?.events_written && w.order.received >= 10000,
        retire_wakes_typed_error: w?.error?.code === "KELD-IPC-001",
        no_fabricated_reply: w?.fabricated_reply === false,
        retire_to_wake_ms: r && w ? (w.wake_wall_us - r.wall_us) / 1000 : null,
        host_events_written: r?.events_written ?? null,
        host_serve_returned: !!s?.serve_returned,
        no_hang: !x.hung && x.clientExit === 0 && x.hostExit === 0,
      };
    },
  },
  "E-quit": {
    host: ["--arm", "e-quit"],
    client: "arm-b-main.ts",
    args: ["--mode", "quit", "--expect-events", "10000"],
    evaluate(h, c, x) {
      const s = ev(h, "summary"), w = ev(c, "woke"), q = ev(h, "quit_replied");
      return {
        zero_link_stall: stallFree(s),
        no_ipc006: !!s && !s.ipc006,
        events_in_order_after_wake: !!w?.order?.inOrder && w.order.received === 10000,
        real_quit_reply: !!w?.quit_reply,
        no_fabricated_reply: w?.error === null,
        link_closed_after_quit: !!w?.link_closed_after_quit,
        quit_reply_to_wake_ms: q && w ? (w.wake_wall_us - q.wall_us) / 1000 : null,
        host_serve_returned: !!s?.serve_returned && !!s?.quit_replied,
        no_hang: !x.hung && x.clientExit === 0 && x.hostExit === 0,
      };
    },
  },
  "BC-credit-ring-64KiB": {
    host: ["--arm", "bc", "--credit-window", "1"],
    client: "arm-b-main.ts",
    args: ["--mode", "scenario", "--expect-events", "11000", "--ring", "65536", "--credit-ring"],
    evaluate(h, c, x) {
      const s = ev(h, "summary"), w = ev(c, "woke"), pw = ev(c, "post_wake_delivery"), rw = ev(h, "reply_written"), bd = ev(h, "backlog_done");
      return {
        zero_link_stall: stallFree(s),
        no_ipc006: !!s && !s.ipc006,
        events_in_order_after_wake: !!pw?.order?.inOrder && pw.order.received === 11000,
        sync_reply_while_parked: !!w?.reply_ok,
        host_writer_max_block_us: s?.max_block_us ?? null,
        host_write_stalls_ge_250ms: s?.write_stalls_ge_250ms ?? null,
        producer_backlog_at_reply: rw?.backlog ?? null,
        producer_resume_after_wake_ms: bd?.resume_ms ?? null,
        replayed_at_wake: w?.order?.received ?? null,
        ring_high_water_bytes: w?.ring_high_water_bytes ?? null,
        post_wake_delivery_ms: pw?.ms ?? null,
        no_hang: !x.hung && x.clientExit === 0 && x.hostExit === 0,
      };
    },
  },
  "E-worker-death": {
    host: ["--arm", "b"],
    client: "arm-b-main.ts",
    args: ["--mode", "scenario", "--worker-die-ms", "500", "--call-deadline-ms", "3000"],
    evaluate(h, c, x) {
      const s = ev(h, "summary"), w = ev(c, "woke"), d = ev(c, "worker_dying");
      return {
        main_error: w?.error?.code ?? null,
        main_woke_after_death_ms: w && d ? (w.wake_wall_us - d.wall_us) / 1000 : null,
        typed_close_wake: w?.error?.code === "KELD-IPC-001",
        no_fabricated_reply: w?.reply_ok === false,
        host_writer_error: s?.error?.slice(0, 60) ?? null,
        host_ended: s?.ended?.slice(0, 60) ?? null,
        host_ipc006: s?.ipc006 ?? null,
        no_hang: !x.hung,
      };
    },
  },
  "NC2-no-notify": {
    host: ["--arm", "b"],
    client: "arm-b-main.ts",
    args: ["--mode", "nc-notify", "--no-notify", "--call-deadline-ms", "2000"],
    evaluate(h, c) {
      const r = ev(c, "rtt");
      return { call_timed_out: typeof r?.error === "string" && r.error.startsWith("KELD-IPC-006"), elapsed_ms: r?.max_ms ?? null, error: r?.error ?? null, negative_control_holds: typeof r?.error === "string" && r.error.startsWith("KELD-IPC-006") };
    },
  },
  "NC3-drop-one": {
    host: ["--arm", "b"],
    client: "arm-b-main.ts",
    args: ["--mode", "scenario", "--expect-events", "11000", "--drop-seq", "5000"],
    evaluate(h, c) {
      const w = ev(c, "woke");
      return { order: w?.order ?? null, negative_control_holds: w?.order?.inOrder === false };
    },
  },
  "B-overflow-64KiB": {
    host: ["--arm", "b"],
    client: "arm-b-main.ts",
    args: ["--mode", "scenario", "--expect-events", "11000", "--ring", "65536"],
    evaluate(h, c, x) {
      const s = ev(h, "summary"), w = ev(c, "woke"), wc = ev(c, "worker_closed");
      return {
        typed_overflow_error: w?.error?.code ?? null,
        no_fabricated_reply: w?.reply_ok === false,
        events_replayed_before_overflow: w?.order?.received ?? null,
        replayed_prefix_in_order: w?.order ? w.order.dupes === 0 && w.order.gaps === 0 && w.order.outOfOrder === 0 : null,
        worker_events_in_at_overflow: wc?.events_in ?? null,
        host_writer_error: s?.error?.slice(0, 60) ?? null,
        host_ipc006: s?.ipc006 ?? null,
        host_max_write_block_us: s?.max_block_us ?? null,
        no_hang: !x.hung,
      };
    },
  },
  "B-rtt": {
    host: ["--arm", "b"],
    client: "arm-b-main.ts",
    args: ["--mode", "rtt", "--calls", "1000"],
    evaluate(h, c) {
      const r = ev(c, "rtt");
      return { diagnostic_only: true, calls: r?.calls, error: r?.error, p50_ms: r?.p50_ms, p99_ms: r?.p99_ms, max_ms: r?.max_ms };
    },
  },
};

function lines(text: string): J[] {
  const out: J[] = [];
  for (const l of text.split("\n")) {
    const t = l.trim();
    if (!t.startsWith("{")) continue;
    try {
      out.push(JSON.parse(t));
    } catch {
      /* non-JSON line kept in raw log only */
    }
  }
  return out;
}

async function runOnce(name: string, spec: Spec, i: number): Promise<J> {
  const dir = `${P1}/runs/${name}/${i}`;
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  const t0 = performance.now();
  const host = Bun.spawn([HOST, ...spec.host, "--sock", "s.sock"], { cwd: dir, stdout: "pipe", stderr: "pipe" });
  let hostText = "";
  let resolveLink: (l: string) => void = () => undefined;
  const linkP = new Promise<string>((r) => (resolveLink = r));
  const hostRead = (async () => {
    const dec = new TextDecoder();
    for await (const chunk of host.stdout) {
      hostText += dec.decode(chunk, { stream: true });
      const first = hostText.split("\n")[0];
      if (hostText.includes("\n") && first) {
        try {
          const j = JSON.parse(first);
          if (j.link) resolveLink(j.link);
        } catch {
          /* wait for more */
        }
      }
    }
  })();
  let hung = false;
  const kill = setTimeout(() => {
    hung = true;
    host.kill();
    client?.kill();
  }, HARD_TIMEOUT_MS);
  let client: ReturnType<typeof Bun.spawn> | undefined;
  const link = await Promise.race([linkP, host.exited.then(() => "")]);
  let clientText = "";
  let clientErr = "";
  if (link) {
    const env = { ...process.env, KELD_APP_LINK: link };
    client = Bun.spawn(["bun", "--no-install", `${P1}/client/${spec.client}`, ...spec.args], { cwd: dir, env, stdout: "pipe", stderr: "pipe" });
    [clientText, clientErr] = await Promise.all([new Response(client.stdout as ReadableStream).text(), new Response(client.stderr as ReadableStream).text()]);
    await client.exited;
  }
  await host.exited;
  await hostRead;
  clearTimeout(kill);
  const hostErr = await new Response(host.stderr).text();
  writeFileSync(`${dir}/host.jsonl`, hostText);
  writeFileSync(`${dir}/client.jsonl`, clientText);
  if (clientErr || hostErr) writeFileSync(`${dir}/stderr.txt`, `--host--\n${hostErr}\n--client--\n${clientErr}`);
  const x = { hung, hostExit: host.exitCode, clientExit: client?.exitCode ?? null, ms: performance.now() - t0 };
  const row = { spec: name, run: i, ...spec.evaluate(lines(hostText), lines(clientText), x), host_exit: x.hostExit, client_exit: x.clientExit, wall_ms: Math.round(x.ms) };
  return row;
}

const what = process.argv[2] ?? "report";
if (what === "report") {
  const all: J = {};
  for (const name of readdirSync(`${P1}/runs`)) {
    const f = `${P1}/runs/${name}/rows.json`;
    if (existsSync(f)) all[name] = JSON.parse(readFileSync(f, "utf8"));
  }
  writeFileSync(`${P1}/s0-link-drain.json`, JSON.stringify({ schema: "keld.execution-artifact/v1 (scratch)", ticket: "gyldlab/keld#418 PANEL-P1", bun: Bun.version, platform: `${process.platform}-${process.arch}`, generated_wall: new Date().toISOString(), rows: all }, null, 1));
  console.log(JSON.stringify(all, null, 1));
} else {
  const spec = specs[what];
  if (!spec) throw new Error(`unknown spec ${what}; known: ${Object.keys(specs).join(", ")}`);
  const runs = Number(process.argv[3] ?? "1");
  const rows: J[] = [];
  for (let i = 1; i <= runs; i += 1) {
    const row = await runOnce(what, spec, i);
    rows.push(row);
    console.log(JSON.stringify(row));
  }
  writeFileSync(`${P1}/runs/${what}/rows.json`, JSON.stringify(rows, null, 1));
}
