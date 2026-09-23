// Native WebView2 fixture only. Rust supplies run-scoped names/write values and
// independently asserts every before/after result. This page never decides pass.
const config = globalThis.keldProfileState;
const key = `keld135.${config.nonce}`;
const databaseName = `${key}.idb`;
const cacheName = `${key}.cache`;
const cacheUrl = new URL(`/cache-entry/${config.nonce}`, location.origin).href;
const workerScope = new URL(`/worker-scope/${config.nonce}/`, location.origin).href;
let phase = "capabilities";

async function markPhase(name) {
  phase = name;
  await report({ progress: name });
}


function cookieValue() {
  const prefix = `${key}=`;
  return document.cookie.split(";").map((part) => part.trim())
    .find((part) => part.startsWith(prefix))?.slice(prefix.length) ?? "";
}

function openDatabase() {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(databaseName, 1);
    request.onupgradeneeded = () => request.result.createObjectStore("state");
    request.onerror = () => reject(request.error);
    request.onblocked = () => reject(new Error("database-blocked"));
    request.onsuccess = () => resolve(request.result);
  });
}

async function databaseValue(writeValue) {
  const database = await openDatabase();
  try {
    return await new Promise((resolve, reject) => {
      const transaction = database.transaction(
        "state", writeValue === undefined ? "readonly" : "readwrite",
      );
      let value = "";
      transaction.onabort = () => reject(transaction.error ?? new Error("transaction-aborted"));
      transaction.onerror = () => reject(transaction.error ?? new Error("transaction-error"));
      // A request's success can precede transaction failure. Only this event
      // permits the native controller to close/restart the app.
      transaction.oncomplete = () => resolve(value);
      const store = transaction.objectStore("state");
      const request = writeValue === undefined ? store.get("nonce") : store.put(writeValue, "nonce");
      request.onsuccess = () => {
        if (writeValue === undefined) {
          if (request.result !== undefined && typeof request.result !== "string") {
            transaction.abort();
          } else {
            value = request.result ?? "";
          }
        }
      };
    });
  } finally {
    database.close();
  }
}

function deleteDatabase() {
  return new Promise((resolve, reject) => {
    const request = indexedDB.deleteDatabase(databaseName);
    request.onsuccess = () => resolve();
    request.onerror = () => reject(request.error);
    request.onblocked = () => reject(new Error("delete-database-blocked"));
  });
}

async function cacheValue() {
  const cache = await caches.open(cacheName);
  const response = await cache.match(cacheUrl);
  return response === undefined ? "" : await response.text();
}

function activatedWorker(registration) {
  const worker = registration.active ?? registration.waiting ?? registration.installing;
  if (worker === null) return Promise.reject(new Error("worker-missing"));
  return new Promise((resolve, reject) => {
    const changed = () => {
      if (worker.state === "activated" || worker.state === "redundant") {
        worker.removeEventListener("statechange", changed);
        if (worker.state === "activated") resolve(worker);
        else reject(new Error("worker-redundant"));
      }
    };
    worker.addEventListener("statechange", changed);
    changed();
  });
}

async function workerValue() {
  const registration = await navigator.serviceWorker.getRegistration(workerScope);
  if (registration === undefined) return "";
  const worker = await activatedWorker(registration);
  return await new Promise((resolve, reject) => {
    const channel = new MessageChannel();
    // A kill switch, never synchronization. Success requires a worker reply.
    const timer = setTimeout(() => {
      channel.port1.close();
      reject(new Error("worker-message-timeout"));
    }, 5000);
    channel.port1.onmessage = (event) => {
      clearTimeout(timer);
      channel.port1.close();
      if (typeof event.data !== "string") reject(new Error("worker-value-type"));
      else resolve(event.data);
    };
    worker.postMessage("read-nonce", [channel.port2]);
  });
}

async function readStores() {
  await markPhase("cookie-read");
  const cookie = cookieValue();
  await markPhase("localStorage-read");
  const local = localStorage.getItem(key) ?? "";
  await markPhase("indexedDB-read");
  const indexed = await databaseValue();
  await markPhase("CacheStorage-read");
  const cache = await cacheValue();
  await markPhase("serviceWorker-read");
  const worker = await workerValue();
  return { cookie, local, indexed, cache, worker };
}

async function seedStores(before) {
  await markPhase("cookie-write");
  if (before.cookie === "") document.cookie = `${key}=${config.value}; Max-Age=3600; Path=/; SameSite=Lax`;
  await markPhase("localStorage-write");
  if (before.local === "") localStorage.setItem(key, config.value);
  await markPhase("indexedDB-write");
  if (before.indexed === "") await databaseValue(config.value);
  await markPhase("CacheStorage-write");
  if (before.cache === "") {
    const cache = await caches.open(cacheName);
    await cache.put(cacheUrl, new Response(config.value));
  }
  await markPhase("serviceWorker-write");
  if (before.worker === "") {
    const query = new URLSearchParams({ nonce: config.nonce, value: config.value });
    const registration = await navigator.serviceWorker.register(`/profile-worker.js?${query}`, {
      scope: workerScope, updateViaCache: "none",
    });
    await activatedWorker(registration);
  }
}

async function clearStores() {
  await markPhase("cookie-clear");
  document.cookie = `${key}=; Max-Age=0; Path=/; SameSite=Lax`;
  await markPhase("localStorage-clear");
  localStorage.removeItem(key);
  await markPhase("indexedDB-clear");
  await deleteDatabase();
  await markPhase("CacheStorage-clear");
  await caches.delete(cacheName);
  await markPhase("serviceWorker-clear");
  const registration = await navigator.serviceWorker.getRegistration(workerScope);
  if (registration !== undefined && !await registration.unregister()) {
    throw new Error("worker-unregister-failed");
  }
}

async function report(fields) {
  const query = new URLSearchParams({ case: config.caseName, nonce: config.nonce, ...fields });
  const response = await fetch(`/state?${query}`, { cache: "no-store" });
  if (!response.ok) throw new Error("state-report-rejected");
}

async function main() {
  await markPhase("capabilities");
  if (!isSecureContext || !globalThis.indexedDB || !globalThis.caches || !navigator.serviceWorker) {
    throw new Error("required-storage-api-unavailable");
  }
  const before = await readStores();
  if (config.value === "") await clearStores();
  else await seedStores(before);
  const after = await readStores();
  // Reading absent IDB/cache entries creates empty containers. Remove only the
  // current run's containers again after verifying their contents are empty.
  if (config.value === "") await clearStores();
  await markPhase("report");
  await report({
    before: before.local, after: after.local,
    cookie_before: before.cookie, cookie_after: after.cookie,
    indexed_db_before: before.indexed, indexed_db_after: after.indexed,
    cache_before: before.cache, cache_after: after.cache,
    worker_before: before.worker, worker_after: after.worker,
  });
}

main().catch(async (error) => {
  document.body.textContent = `Storage fixture failed during ${phase}: ${String(error)}`;
  await report({ error: `${phase}:${error?.name ?? "Error"}` });
});
