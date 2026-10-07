'use strict';
// Shared transcript logger. One JSON line per event, appended synchronously so that
// file order is the true cross-process order (O_APPEND, one write() per line).
const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');

const OUT = process.env.ORACLE_OUT;
const WHO = process.type === 'renderer' ? 'renderer' : 'main';
const CAP = 2000;          // hard per-process line cap (a run is ~20-170 lines); drops are counted, never thrown
let written = 0, dropped = 0;

function log(ev, data) {
  const rec = Object.assign({
    proc: WHO,
    pid: process.pid,
    ev,
    perf_ms: performance.now(),               // per-process time origin
    hr_ns: process.hrtime.bigint().toString(), // process.hrtime clock, ns
    epoch_ms: performance.timeOrigin + performance.now(),
  }, data);
  if (written >= CAP) { dropped++; return; }
  try { fs.appendFileSync(OUT, JSON.stringify(rec) + '\n'); written++; }
  catch (e) { dropped++; }   // e.g. ENOSPC: never raise an uncaught exception (it opens a modal error box)
}

function tomb(win, wc) {
  const r = {};
  try { r.win_destroyed = win.isDestroyed(); } catch (e) { r.win_destroyed = 'throw:' + e.message; }
  try { r.wc_destroyed = wc.isDestroyed(); } catch (e) { r.wc_destroyed = 'throw:' + e.message; }
  return r;
}

// Human-free modal answer: dialog.showMessageBoxSync has no `signal`/timeout option in
// v44.4.5 (docs/api/dialog.md lists `signal` only for showMessageBox), so a detached
// shell sleeps delayMs then presses the button via osascript/System Events (see dismiss.sh).
function scheduleDismiss(delayMs, key) {
  const sh = path.join(__dirname, 'dismiss.sh');
  const child = spawn('/bin/sh', [sh, String(delayMs / 1000), key, String(process.pid), OUT + '.dismiss'],
    { detached: true, stdio: 'ignore' });
  child.unref();
  log('dismiss:scheduled', { method: 'osascript System Events AX button click (retry until modal returns)', key, delay_ms: delayMs, child_pid: child.pid });
}

module.exports = { log, tomb, scheduleDismiss };
