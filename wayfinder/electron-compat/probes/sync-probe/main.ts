const sab = new SharedArrayBuffer(8);
const i32 = new Int32Array(sab);
const w = new Worker(new URL("./worker.ts", import.meta.url).href);
w.postMessage(sab);
const t0 = performance.now();
const r = Atomics.wait(i32, 0, 0, 2000);
console.log(JSON.stringify({ bun: Bun.version, wait: r, value: Atomics.load(i32, 0), ms: +(performance.now() - t0).toFixed(2) }));
w.terminate();
