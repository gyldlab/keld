// Harness self-test: ESM *named* imports of every wrapped builtin must hit the recorder.
// If any canary event is missing from the trace, absence of such events in the app run proves nothing.
import { existsSync, readFileSync } from 'node:fs';
import { promises as fsp } from 'fs';
import { readFile } from 'fs/promises';
import { spawnSync, execFile } from 'child_process';
import { homedir, tmpdir } from 'os';
import http from 'node:http';
import net from 'node:net';
import * as nsFs from 'node:fs';

export async function runCanary() {
	existsSync('/nonexistent-keld-canary');
	nsFs.existsSync('/nonexistent-keld-canary-ns');
	try { readFileSync('/nonexistent-keld-canary-read'); } catch {}
	await fsp.readFile('/nonexistent-keld-canary-p').catch(() => {});
	await readFile('/nonexistent-keld-canary-p2').catch(() => {});
	spawnSync('/usr/bin/true', []);
	await new Promise((r) => execFile('/usr/bin/true', [], () => r()));
	homedir(); tmpdir();
	await new Promise((r) => { const q = http.get('http://canary.invalid/x', () => {}); q.on('error', () => r()); });
	await new Promise((r) => { const s = net.connect(9, 'canary.invalid'); s.on('error', () => r()); });
	await fetch('https://canary.invalid/fetch').catch(() => {});
	// write guard: must be refused, never reach disk
	try { nsFs.writeFileSync('/private/tmp/keld-harness-canary-must-not-exist', 'x'); } catch {}
}
