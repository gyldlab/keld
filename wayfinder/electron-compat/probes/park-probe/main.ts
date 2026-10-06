// Park-probe: how many bytes can a server push into a Bun client whose main
// thread is parked in Atomics.wait (no `data` callbacks can run)?
const path = `/tmp/keld-park-probe-${process.pid}.sock`;
try { require("node:fs").unlinkSync(path); } catch {}
let clientGot = 0;
const chunk = new Uint8Array(1024);
let accepted = 0, writes = 0, stalledAt = -1;
const server = Bun.listen({
  unix: path,
  socket: {
    open(sock) {
      // Give the client time to park, then flood.
      setTimeout(() => {
        for (let i = 0; i < 4096; i++) {
          const n = sock.write(chunk);
          writes++;
          if (n <= 0 || n < chunk.length) { stalledAt = accepted + Math.max(n, 0); accepted += Math.max(n, 0); break; }
          accepted += n;
        }
        console.log(JSON.stringify({ phase: "server-flood", writes, acceptedBytes: accepted, stalledAtBytes: stalledAt }));
      }, 300);
    },
    data() {}, drain() {}, error(_s, e) { console.log("server error", e.message); }, close() {},
  },
});
const sab = new SharedArrayBuffer(4);
const i32 = new Int32Array(sab);
const client = await Bun.connect({
  unix: path,
  socket: {
    data(_s, d) { clientGot += d.byteLength; },
    open() {
      // Park main thread for 1500 ms: simulates showMessageBoxSync via Atomics.wait.
      const t0 = performance.now();
      const r = Atomics.wait(i32, 0, 0, 1500);
      console.log(JSON.stringify({ phase: "client-parked", wait: r, ms: +(performance.now() - t0).toFixed(1), bytesSeenDuringPark: clientGot }));
      setTimeout(() => {
        console.log(JSON.stringify({ phase: "client-drained", bytesAfterPark: clientGot }));
        server.stop(true); process.exit(0);
      }, 500);
    },
    drain() {}, error() {}, close() {},
  },
});
