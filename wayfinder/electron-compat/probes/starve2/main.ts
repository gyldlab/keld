const sab = new SharedArrayBuffer(8); const i32 = new Int32Array(sab);
const w = new Worker(new URL("./worker.ts", import.meta.url).href); w.postMessage(sab);
const srv = Bun.listen({ hostname: "127.0.0.1", port: 0, socket: { data(_s, d) { log.push(["server-data", +(performance.now()-t0).toFixed(1)]); } } });
const cli = await Bun.connect({ hostname: "127.0.0.1", port: srv.port, socket: { data() {} } });
const log: any[] = []; const t0 = performance.now();
setTimeout(() => log.push(["timer10", +(performance.now()-t0).toFixed(1)]), 10);
cli.write("ping"); // written BEFORE park; server data callback can only run when loop runs
const parkStart = +(performance.now()-t0).toFixed(1);
const r = Atomics.wait(i32, 0, 0, 1000);
const parkEnd = +(performance.now()-t0).toFixed(1);
await new Promise(res => setTimeout(res, 50));
console.log(JSON.stringify({ bun: Bun.version, wait: r, parkStart, parkEnd, log }));
srv.stop(); cli.end(); w.terminate();
