// Runner: imports draw.io's Electron main under Bun with the recording harness preloaded,
// plays "fake renderer" through the ipcMain handlers the app registered, and writes
// harness/run-result.json (step outcomes + faults) next to the trace.
const H = globalThis.__KELD_HARNESS__;
if (!H || !H.fe) throw new Error('run via: bun --preload harness/preload.cjs harness/run.mjs');

const fe = H.fe;
const { APP, P3, FAKEHOME } = H;
const FIX = P3 + '/fixtures';
const SAMPLE = FIX + '/sample.drawio';
const SAVE_AS = FIX + '/saved-as.drawio';
const EXPORT_OUT = FIX + '/export-out.drawio';
const CODE_URL = 'file://' + APP + '/drawio/src/main/webapp';
const results = { steps: [], importError: null, notes: [] };

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const orig = H.origFs; // pristine fs, never traced

function note(s) { results.notes.push(s); }
function driver(member, shape) { H.emit('electron', '<driver> ' + member, shape, { op: 'driver', driver: true }); }

async function settle(label, quietMs = 400, maxMs = 10000) {
	const t0 = Date.now();
	let last = -1, stable = 0;
	while (Date.now() - t0 < maxMs) {
		await sleep(100);
		if (H.seq === last) { stable += 100; if (stable >= quietMs) return true; } else { last = H.seq; stable = 0; }
	}
	note('settle(' + label + ') hit maxMs ' + maxMs);
	return false;
}

function record(name, ok, detail) {
	results.steps.push({ phase: H.phase, step: name, ok, detail: detail === undefined ? null : detail });
	process.stderr.write('[harness] ' + (ok ? 'ok   ' : 'FAIL ') + H.phase + '/' + name + (detail !== undefined ? ' :: ' + JSON.stringify(detail).slice(0, 300) : '') + '\n');
}

// ---- fake renderer ---------------------------------------------------------
let reqCounter = 0;
function ipcEvent(winRaw, onReply) {
	const ev = {
		senderFrame: { url: CODE_URL + '/index.html?mode=device', routingId: 1, processId: 1 },
		sender: winRaw.webContents, processId: 1, frameId: 1, // the tracked proxy: in Electron event.sender === win.webContents (FileWatcher keys on identity)
		reply(channel, payload) { onReply(channel, payload); },
		preventDefault() {},
	};
	return fe.tracked('ipcMainEvent', ev);
}

// Boot handshake the real renderer performs: did-finish-load, then 'app-load-finished'.
H.bus.on('loadURL', (winRaw, url) => {
	setTimeout(() => {
		driver('renderer:did-finish-load', { win: winRaw.id });
		winRaw._wc.emit('did-finish-load');
		driver('ipc:app-load-finished', { win: winRaw.id });
		fe.ipcRaw.emit('app-load-finished', ipcEvent(winRaw, () => {}));
	}, 0);
});

async function rendererReq(winRaw, msg, timeoutMs = 5000) {
	const reqId = ++reqCounter;
	const payload = Object.assign({}, msg, { reqId });
	const { action: _a, ...rest } = msg;
	driver('ipc:rendererReq', Object.assign({ action: msg.action }, H.shape(rest)));
	let resolveFn;
	const done = new Promise((r) => { resolveFn = r; });
	const ev = ipcEvent(winRaw, (channel, resp) => {
		if (channel === 'mainResp' && resp && resp.reqId === reqId) resolveFn(resp);
	});
	fe.ipcRaw.emit('rendererReq', ev, payload);
	const timer = new Promise((r) => setTimeout(() => r({ timeout: true }), timeoutMs));
	const resp = await Promise.race([done, timer]);
	if (resp.timeout) return { ok: false, timeout: true };
	if (resp.error) return { ok: false, error: resp.msg };
	return { ok: true, data: resp.data };
}

// ---------------------------------------------------------------------------
async function main() {
	process.argv = [H.ELECTRON_BIN, '.'];
	process.chdir(APP);

	// ===== phase: import ======================================================
	H.setPhase('import');
	H.setStep('canary');
	const { runCanary } = await import(H.HARNESS + '/canary.mjs');
	await runCanary();
	H.setStep('import');
	try {
		await import(APP + '/src/main/electron.js');
		record('import src/main/electron.js', true);
	} catch (e) {
		results.importError = { name: e && e.name, message: e && e.message, stack: String((e && e.stack) || '').split('\n').slice(0, 12) };
		record('import src/main/electron.js', false, results.importError.message);
		return;
	}
	// Anything the app scheduled during evaluation (timers, queued microtasks) still belongs to import.
	await sleep(50);

	// ===== phase: boot ========================================================
	H.setPhase('boot');
	H.setStep('boot.ready');
	driver('app:will-finish-launching+ready', {});
	fe.signalReady();
	await settle('boot');
	const win = fe.windows[0];
	if (!win) { record('first window constructed', false); return; }
	record('first window constructed', true, { winId: win.id, seq: H.firstWindowSeq });
	const sawArgs = H.sent.some((s) => s.channel === 'args-obj');
	record("main sent 'args-obj' (boot handshake complete)", sawArgs);

	// Policy probes on handlers the app registered during boot (no authority; shows decisions)
	H.setStep('boot.probe-webRequest');
	const wr = fe.webRequestHandlers;
	if (wr.onBeforeRequest) {
		for (const u of [CODE_URL + '/index.html', 'file://' + FAKEHOME + '/secret']) {
			await new Promise((res) => {
				driver('webRequest.onBeforeRequest', { url: u });
				wr.onBeforeRequest({ url: u }, (r) => { record('onBeforeRequest ' + (u.startsWith(CODE_URL) ? 'in-app file' : 'outside file'), true, r); res(); });
			});
		}
	}
	H.setStep(null);

	// ===== phase: open ========================================================
	H.setPhase('open');
	H.setStep('open.dialog');
	H.stubs.openFilePaths.push([SAMPLE]);
	let r = await rendererReq(win, { action: 'getDocumentsFolder' });
	record('getDocumentsFolder', r.ok, r.data);
	r = await rendererReq(win, { action: 'showOpenDialog', defaultPath: r.data, filters: [{ name: 'draw.io Diagrams', extensions: ['drawio', 'xml'] }], properties: ['openFile'] });
	record('showOpenDialog -> fixture', r.ok && Array.isArray(r.data) && r.data[0] === SAMPLE, r.ok ? r.data : r.error);
	const opened = r.ok ? r.data[0] : SAMPLE;
	r = await rendererReq(win, { action: 'dirname', path: opened });
	H.setStep('open.readFile');
	r = await rendererReq(win, { action: 'readFile', filename: opened, encoding: 'utf-8' });
	record('readFile (blessed fixture)', r.ok && typeof r.data === 'string' && r.data.startsWith('<mxfile'), r.ok ? { len: r.data.length } : r.error);
	const fileData = r.ok ? r.data : '';
	r = await rendererReq(win, { action: 'fileStat', file: opened });
	record('fileStat', r.ok, r.ok ? { size: r.data.size, mtimeMs: !!r.data.mtimeMs } : r.error);
	const origStat = r.ok ? r.data : null;
	r = await rendererReq(win, { action: 'isFileWritable', file: opened });
	record('isFileWritable', r.ok, r.data);
	r = await rendererReq(win, { action: 'getFileDrafts', fileObject: { path: opened, name: 'sample.drawio', type: 'utf-8' } });
	record('getFileDrafts', r.ok, r.ok ? { count: r.data.length } : r.error);
	r = await rendererReq(win, { action: 'watchFile', path: opened });
	record('watchFile', r.ok, r.error);

	H.setStep('open.edit');
	driver('edit (renderer-only: mxGraph model change, no IPC, no main-process work)', {});
	record('edit is renderer-only (no main-process events expected)', true);

	// Negative controls: the app's own path gate
	H.setStep('open.probe-unblessed-read');
	r = await rendererReq(win, { action: 'readFile', filename: FIX + '/secret.txt', encoding: 'utf-8' });
	record('NEG readFile of unblessed file is refused', !r.ok && /not authorised/.test(r.error || ''), r.ok ? 'READ SUCCEEDED' : r.error);
	r = await rendererReq(win, { action: 'readFile', filename: opened + '/../secret.txt', encoding: 'utf-8' });
	record('NEG readFile via ../ traversal is refused', !r.ok && /not authorised/.test(r.error || ''), r.ok ? 'READ SUCCEEDED' : r.error);
	r = await rendererReq(win, { action: 'readFile', filename: APP + '/package.json', encoding: 'utf-8' });
	record('NEG readFile inside app bundle is refused', !r.ok && /not authorised/.test(r.error || ''), r.ok ? 'READ SUCCEEDED' : r.error);
	H.setStep(null);
	await settle('open');

	// ===== phase: save ========================================================
	H.setPhase('save');
	H.setStep('save.autosave-draft');
	r = await rendererReq(win, { action: 'saveDraft', fileObject: { path: opened, name: 'sample.drawio', type: 'utf-8' }, data: fileData });
	record('saveDraft (autosave while editing)', r.ok, r.ok ? H.abbr(String(r.data)) : r.error);
	const draftPath = r.ok ? r.data : null;

	H.setStep('save.saveFile-existing');
	const edited = fileData.replace('Hello', 'Hello (edited)');
	r = await rendererReq(win, { action: 'saveFile', fileObject: { path: opened, name: 'sample.drawio', type: 'utf-8' }, defEnc: 'utf-8', data: edited, origStat, overwrite: false });
	record('saveFile to opened file (backup + write + verify)', r.ok, r.ok ? { size: r.data.size } : r.error);
	H.setStep('save.removeDraft');
	if (draftPath) {
		r = await rendererReq(win, { action: 'deleteFile', file: draftPath });
		record('deleteFile (draft removed after save)', r.ok, r.error);
	}

	H.setStep('save.saveAs');
	H.stubs.saveFilePaths.push(SAVE_AS);
	r = await rendererReq(win, { action: 'getDocumentsFolder' });
	r = await rendererReq(win, { action: 'showSaveDialog', defaultPath: r.data + '/sample.drawio', filters: [{ name: 'draw.io', extensions: ['drawio'] }] });
	record('showSaveDialog -> new path', r.ok && r.data === SAVE_AS, r.ok ? H.abbr(String(r.data)) : r.error);
	const saveAsPath = r.ok ? r.data : SAVE_AS;
	r = await rendererReq(win, { action: 'dirname', path: saveAsPath });
	r = await rendererReq(win, { action: 'saveFile', fileObject: { path: saveAsPath, name: 'saved-as.drawio', type: 'utf-8' }, defEnc: 'utf-8', data: edited, origStat: null, overwrite: false });
	record('saveFile to new path (Save As)', r.ok, r.ok ? { size: r.data.size } : r.error);

	H.setStep('save.writeFile-export');
	H.stubs.saveFilePaths.push(EXPORT_OUT);
	r = await rendererReq(win, { action: 'showSaveDialog', defaultPath: FIX + '/export', filters: [] });
	r = await rendererReq(win, { action: 'writeFile', path: EXPORT_OUT, data: edited, enc: 'utf-8' });
	record('writeFile to dialog-blessed path', r.ok, r.error);

	H.setStep('save.probe-unblessed-write');
	r = await rendererReq(win, { action: 'writeFile', path: FIX + '/unblessed.drawio', data: edited, enc: 'utf-8' });
	record('NEG writeFile to unblessed path is refused', !r.ok && /not authorised/.test(r.error || ''), r.ok ? 'WRITE SUCCEEDED' : r.error);
	r = await rendererReq(win, { action: 'saveFile', fileObject: { path: FAKEHOME + '/Library/Application Support/draw.io/config.json', type: 'utf-8' }, defEnc: 'utf-8', data: edited, origStat: null, overwrite: true });
	record('NEG saveFile into userData is refused', !r.ok && /not authorised/.test(r.error || ''), r.ok ? 'WRITE SUCCEEDED' : r.error);
	r = await rendererReq(win, { action: 'saveFile', fileObject: { path: '/private/tmp/keld-harness-should-never-exist.drawio', type: 'utf-8' }, defEnc: 'utf-8', data: edited, origStat: null, overwrite: true });
	record('NEG saveFile outside p3 is refused by the app', !r.ok && /not authorised/.test(r.error || ''), r.ok ? 'WRITE SUCCEEDED' : r.error);
	H.setStep(null);
	await settle('save');

	// ===== phase: close =======================================================
	H.setPhase('close');
	H.setStep('close.setup-draft');
	r = await rendererReq(win, { action: 'saveDraft', fileObject: { path: opened, name: 'sample.drawio', type: 'utf-8' }, data: edited });
	const draft2 = r.ok ? r.data : null;
	record('saveDraft (unsaved changes pending at close)', r.ok, r.ok ? H.abbr(String(r.data)) : r.error);

	// mac button order is ['Save','Cancel','Discard Changes'] -> Cancel=1, Discard=2
	async function closeWith(answer, label) {
		H.setStep('close.' + label);
		const before = H.sent.length;
		const ev = { defaultPrevented: false, preventDefault() { this.defaultPrevented = true; } };
		driver("win.emit('close') [user closes window with unsaved changes]", { win: win.id });
		win.emit('close', ev);
		const ask = H.sent.slice(before).find((s) => s.channel === 'isModified');
		record(label + ": 'close' prevented and isModified requested", ev.defaultPrevented && !!ask, ask ? { channel: ask.channel } : 'no isModified send');
		if (!ask) return false;
		H.stubs.messageBox.push(answer);
		driver('ipc:isModified-result', { uniqueId: '<id from isModified>', isModified: true, draftPath: draft2 && H.abbr(draft2) });
		fe.ipcRaw.emit('isModified-result', ipcEvent(win, () => {}), { uniqueId: ask.args[0], isModified: true, draftPath: draft2 });
		await settle('close.' + label, 300, 5000);
		return true;
	}
	await closeWith(1, 'cancel');
	record('window survives Cancel', !win._destroyed);
	await closeWith(2, 'discard');
	record('window destroyed after Discard', win._destroyed);
	await settle('close-final', 600, 5000);
	// On macOS the app's 'window-all-closed' handler only quits when cmdQPressed is set, so no
	// app.quit() is the expected (and observed) behaviour here, not a harness gap.
	record("'window-all-closed' handled; macOS keeps the app resident (no app.quit expected)",
		fe.windows.length === 0 && !H.quitRequested, { openWindows: fe.windows.length, quitRequested: H.quitRequested || 0 });
	H.setStep(null);
}

try {
	await main();
} catch (e) {
	results.fatal = { name: e && e.name, message: e && e.message, stack: String((e && e.stack) || '').split('\n').slice(0, 12) };
	process.stderr.write('[harness] FATAL ' + ((e && e.stack) || e) + '\n');
}

H.flushPending();
try {
	const { createRequire } = await import('node:module');
	const req = createRequire(import.meta.url);
	results.loadedModules = Object.keys(req.cache || {}).length;
	const pk = new Set();
	for (const k of Object.keys(req.cache || {})) {
		const m = k.match(/node_modules\/\.bun\/[^/]+\/node_modules\/((?:@[^/]+\/)?[^/]+)\//);
		if (m) pk.add(m[1]); else if (k.startsWith(APP + '/src/')) pk.add('app:' + k.slice(APP.length + 1));
	}
	results.loadedPackages = Array.from(pk).sort();
} catch (e) { results.loadedModulesError = String(e && e.message); }
results.counts = { seq: H.seq };
results.unresolved = Array.from(H.unresolved.entries()).map(([member, v]) => ({ member, ...v }));
results.thrown = H.thrown;
results.console = H.console;
results.processShape = H.processShape;
results.envProxyInstalled = H.envProxyInstalled;
results.quit = { quitRequested: H.quitRequested || 0, exitRequested: H.exitRequested === undefined ? null : H.exitRequested };
results.sentToRenderer = H.sent.map((s) => ({ win: s.win, channel: s.channel, args: H.shape(s.args) }));
results.variant = { packaged: H.PACKAGED, suffix: H.SUFFIX };
orig.writeFileSync(H.RESULT, JSON.stringify(results, null, 2));
orig.closeSync(H.fd);
process.exit(results.importError || results.fatal ? 2 : 0);
