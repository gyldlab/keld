'use strict';
const path = require('path');
const { app, BrowserWindow, dialog, ipcMain } = require('electron');
const { log, tomb, scheduleDismiss } = require('../lib');

app.setPath('userData', path.join(process.env.ORACLE_USERDATA, 'e3'));
const PARENT = process.env.E3_PARENT === '1';        // 1: sheet on window, 0: app-modal alert
const MODE = process.env.E3_MODE || 'plain';         // plain: dialog from a timer; inclose: dialog inside 'close' handler
log('app:start', { electron: process.versions.electron, parent: PARENT, mode: MODE });

app.on('before-quit', () => log('app:before-quit'));
app.on('will-quit', () => log('app:will-quit'));
app.on('quit', () => log('app:quit'));
app.on('window-all-closed', () => { log('app:window-all-closed'); app.quit(); });

app.whenReady().then(() => {
  log('app:ready');
  const win = new BrowserWindow({ width: 420, height: 300, show: true,
    webPreferences: { nodeIntegration: true, contextIsolation: false } });
  const wc = win.webContents;
  let tickN = 0, started = false, modified = true, vetoed = false, iv;

  const box = (answerKey) => {
    scheduleDismiss(1500, answerKey);
    const opts = { type: 'question', message: 'Discard changes?', buttons: ['Cancel', 'Discard'], defaultId: 1, cancelId: 0 };
    log('modal:before', { parent: PARENT });
    const r = PARENT ? dialog.showMessageBoxSync(win, opts) : dialog.showMessageBoxSync(opts);
    log('modal:after', { result: r });
    return r;
  };
  const tryClose = (src) => {
    if (win.isDestroyed()) { log('main:close-skipped-destroyed', { src }); return; }
    try { log('main:win.close-call', { src }); win.close(); log('main:win.close-returned', { src }); }
    catch (e) { log('main:close-threw', { src, msg: e.message }); }
  };

  ipcMain.on('ping', (_e, p) => log('main:ipc-ping-received', { n: p.n }));
  ipcMain.handle('rpc', (_e, n) => { log('main:ipc-rpc-handled', { n }); return n; });
  ipcMain.on('request-close', () => { log('main:ipc-request-close-received'); tryClose('ipc'); });
  ipcMain.on('ready', () => {
    log('ipc:ready');
    iv = setInterval(() => { if (tickN >= 60) { clearInterval(iv); return; } log('main:interval-tick', { n: ++tickN }); }, 100); // cap 60 ticks
    setTimeout(start, 500);
  });

  function arm() {
    setTimeout(() => log('main:timeout-250ms-fired'), 250);
    setImmediate(() => log('main:setImmediate-fired'));
    log('main:timers-armed');
  }

  function start() {
    if (started) return; started = true;
    arm();
    if (MODE === 'plain') {
      box('return');
      setTimeout(() => { log('main:cleanup'); clearInterval(iv); app.quit(); }, 600);
    } else {
      tryClose('start');
    }
  }

  win.on('close', (event) => {
    log('win:close', Object.assign({ vetoed }, tomb(win, wc)));
    if (MODE === 'inclose' && !vetoed) {
      vetoed = true;
      event.preventDefault();
      wc.send('arm-close-request');
      setTimeout(() => { log('main:timeout-300ms-fired'); tryClose('timer-during-modal'); }, 300);
      const r = box('escape');
      log('close-handler:after-modal', { result: r });
    } else {
      log('win:close:allowed');
    }
  });
  win.on('closed', () => { log('win:closed', tomb(win, wc)); clearInterval(iv); });
  win.loadFile(path.join(__dirname, 'index.html'));
});
