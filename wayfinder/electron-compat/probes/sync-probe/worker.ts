declare var self: Worker;
self.onmessage = (e) => { const i32 = new Int32Array(e.data as SharedArrayBuffer); setTimeout(() => { Atomics.store(i32, 0, 42); Atomics.notify(i32, 0); }, 50); };
