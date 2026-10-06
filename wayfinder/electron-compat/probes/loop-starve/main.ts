// Does Atomics.wait on Bun's main thread starve timers/sockets (i.e. the kipc reader)?
const sab = new SharedArrayBuffer(8); const i32 = new Int32Array(sab);
const w = new Worker(new URL("./worker.ts", import.meta.url).href); w.postMessage(sab);
const t0 = performance.now(); let timerAt = -1;
setTimeout(() => { timerAt = performance.now() - t0; }, 10);
const srv = Bun.listen({ hostname: "127.0.0.1", port: 0, socket: { data(_s, d) { console.log(JSON.stringify({ event: "socket-data", at: +(performance.now()-t0).toFixed(1), bytes: d.byteLength })); } } });
const cli = await Bun.connect({ hostname: "127.0.0.1", port: srv.port, socket: { data() {} } });
setTimeout(() => cli.write("ping"), 20); // write scheduled during the park -> cannot run until unpark either
const r = Atomics.wait(i32, 0, 0, 1000);
const waited = +(performance.now() - t0).toFixed(1);
await new Promise(res => setTimeout(res, 50));
console.log(JSON.stringify({ bun: Bun.version, wait: r, waitedMs: waited, timer10msFiredAt: +timerAt.toFixed(1) }));
srv.stop(); cli.end(); w.terminate();
