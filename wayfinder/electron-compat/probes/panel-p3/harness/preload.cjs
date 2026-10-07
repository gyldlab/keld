'use strict';
// bun --no-install --preload harness/preload.cjs harness/run.mjs
// Order matters: recorder (captures pristine fs) -> builtin wrappers -> fake electron.
const H = require('./lib/recorder.cjs');
require('./lib/wrap-builtins.cjs');
const exportsObj = require('./lib/fake-electron.cjs');

// Present an Electron-main-like `process` to code that probes it. Each assignment is
// attempted separately and any failure is recorded for the summary.
H.processShape = {};
function trySet(label, fn) {
	try { fn(); H.processShape[label] = 'ok'; } catch (e) { H.processShape[label] = 'FAILED: ' + e.message; }
}
trySet('process.type', () => { Object.defineProperty(process, 'type', { value: 'browser', configurable: true, writable: true }); });
trySet('process.defaultApp', () => { Object.defineProperty(process, 'defaultApp', { value: true, configurable: true, writable: true }); });
trySet('process.resourcesPath', () => { Object.defineProperty(process, 'resourcesPath', { value: H.APP + '/node_modules/electron/dist/Electron.app/Contents/Resources', configurable: true, writable: true }); });
trySet('process.versions.electron', () => { Object.defineProperty(process.versions, 'electron', { value: '44.5.1', configurable: true, enumerable: true, writable: true }); });
trySet('process.versions.chrome', () => { Object.defineProperty(process.versions, 'chrome', { value: '146.0.0.0', configurable: true, enumerable: true, writable: true }); });

const { plugin } = require('bun');
plugin({
	name: 'keld-fake-electron',
	setup(build) {
		// Bun 1.4.2: onResolve is not consulted for bare specifiers of static imports, but
		// build.module() virtual modules are, for ESM import and CJS require alike.
		build.module('electron', () => ({ exports: Object.assign({}, exportsObj, { default: exportsObj }), loader: 'object' }));
	},
});

// Capture app console output and process-level faults for the summary (output still goes through).
H.console = [];
for (const level of ['log', 'info', 'warn', 'error', 'debug']) {
	const o = console[level];
	console[level] = function (...args) {
		if (H.console.length < 300) {
			let text;
			try { text = args.map((a) => (typeof a === 'string' ? a : (a && a.stack) || JSON.stringify(a))).join(' '); } catch { text = '<unprintable>'; }
			H.console.push({ phase: H.phase, step: H.step, level, text: H.abbr(text).slice(0, 400) });
		}
		return o.apply(this, args);
	};
}
process.on('uncaughtException', (e) => {
	H.thrown.push({ phase: H.phase, kind: 'process', op: 'uncaughtException', error: (e && e.name) || 'Error', message: H.abbr(String((e && e.stack) || e)).slice(0, 600) });
	process.stderr.write('[harness] uncaughtException: ' + ((e && e.stack) || e) + '\n');
});
process.on('unhandledRejection', (e) => {
	H.thrown.push({ phase: H.phase, kind: 'process', op: 'unhandledRejection', error: (e && e.name) || 'Error', message: H.abbr(String((e && e.stack) || e)).slice(0, 600) });
	process.stderr.write('[harness] unhandledRejection: ' + ((e && e.stack) || e) + '\n');
});
