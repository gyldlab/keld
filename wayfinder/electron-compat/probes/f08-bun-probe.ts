const mu = process.memoryUsage();
console.log(JSON.stringify({ memoryUsage: mu, rss: process.memoryUsage.rss?.(), cpu: process.cpuUsage(), resourceUsage: process.resourceUsage?.() }));
try {
  const jsc = await import("bun:jsc");
  const hs = jsc.heapStats();
  console.log("heapStats keys:", Object.keys(hs).join(","), "heapSize", hs.heapSize, "heapCapacity", hs.heapCapacity);
  console.log("memoryUsage(bun:jsc):", JSON.stringify(jsc.memoryUsage?.()));
} catch (e) { console.log("bun:jsc error", String(e)); }
try { const v8 = await import("node:v8"); console.log("v8.getHeapStatistics:", JSON.stringify(v8.getHeapStatistics())); } catch (e) { console.log("v8 error", String(e)); }
console.log("process.report:", typeof process.report, "process.abort:", typeof process.abort, "process.type:", (process as any).type, "versions:", JSON.stringify(process.versions));
