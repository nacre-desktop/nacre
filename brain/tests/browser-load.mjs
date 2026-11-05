// U12 browser-load regression: synthetic cold vault, real launcher,
// bounded first paint, timeout/retry, background discovery and persisted index.
// Run only against a private HOME/vault and headless Chromium exposing CDP.
// BRAIN_TEST_CDP=0 BRAIN_TEST_PROFILE=<private-profile> node brain/tests/browser-load.mjs
import fs from 'node:fs';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import { once } from 'node:events';
import { spawn } from 'node:child_process';

const binary = path.resolve(process.env.BRAIN_TEST_NATIVE || 'target/debug/nacre-helper');
const webSource = path.resolve(process.env.BRAIN_TEST_WEB || 'brain/web');
const cdpPort = Number(process.env.BRAIN_TEST_CDP || 17946);
const browserProfile = path.resolve(process.env.BRAIN_TEST_PROFILE || path.join(os.homedir(), '.cache/nacre-brain-cdp'));
const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'nacre-brain-load-'));
const home = path.join(temp, 'home');
const vault = path.join(temp, 'vault');
const installedWeb = path.join(home, '.local/share/nacre/brain/web');
const fixtureBin = path.join(temp, 'bin');
const runtime = path.join(temp, 'runtime');
const compositorSocket = path.join(runtime, 'hypr', '.socket.sock');
const launchCapture = path.join(temp, 'launch.json');
fs.mkdirSync(home, { recursive: true, mode: 0o700 });
fs.mkdirSync(vault, { recursive: true, mode: 0o700 });
fs.mkdirSync(path.dirname(installedWeb), { recursive: true, mode: 0o700 });
fs.mkdirSync(path.dirname(compositorSocket), { recursive: true, mode: 0o700 });
fs.mkdirSync(fixtureBin, { recursive: true, mode: 0o700 });
fs.cpSync(webSource, installedWeb, { recursive: true });

const subjects = ['Climbing', 'Electronics', 'Research', 'Music', 'Software'];
for (let i = 0; i < 2000; i += 1) {
  const subject = subjects[i % subjects.length];
  fs.writeFileSync(path.join(vault, `${String(i).padStart(4, '0')}.md`),
    `# ${subject} fixture note ${i}\nRecorded: 2026-11-05T12:00:00+00:00\nConfidence: verified\nWorlds: ${subject}\n\nSynthetic browser-load fixture ${i}.\n`,
    { mode: 0o600 });
}

function freePort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const port = server.address().port;
      server.close(error => error ? reject(error) : resolve(port));
    });
  });
}
async function cdpDiscoveryPort() {
  if (cdpPort > 0) return cdpPort;
  const activePortFile = path.join(browserProfile, 'DevToolsActivePort');
  for (let i = 0; i < 100; i += 1) {
    try {
      const port = Number(fs.readFileSync(activePortFile, 'utf8').split(/\r?\n/, 1)[0]);
      if (port > 0 && port <= 65535) return port;
    } catch {}
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  throw new Error(`Chromium did not publish DevToolsActivePort under ${browserProfile}`);
}

const brainPort = await freePort();
const nativeUrl = `http://127.0.0.1:${brainPort}`;
const wrapper = path.join(temp, 'capture-browser.cjs');
fs.writeFileSync(wrapper, `const fs=require('node:fs');fs.writeFileSync(${JSON.stringify(launchCapture)},JSON.stringify({args:process.argv.slice(2)}),{mode:0o600});\n`, { mode: 0o600 });
const fakeBrowser = path.join(fixtureBin, 'brave-origin');
fs.writeFileSync(fakeBrowser, `#!/bin/sh\nexec ${process.execPath} ${wrapper} "$@"\n`, { mode: 0o700 });
fs.writeFileSync(path.join(fixtureBin, 'xdg-settings'), '#!/bin/sh\nexit 1\n', { mode: 0o700 });
const env = { ...process.env, HOME: home, PATH: `${fixtureBin}:${process.env.PATH || ''}`, BROWSER: 'brave-origin', XDG_RUNTIME_DIR: runtime, NACRE_COMPOSITOR: 'hyprland', NACRE_BRAIN_DIRECT: '1', NACRE_BRAIN_DIR: vault, NACRE_BRAIN_SEMANTICS: '0', NACRE_BRAIN_PORT: String(brainPort), NACRE_BRAIN_IDLE_SECONDS: '900' };
for (const key of ['NACRE_BRAIN_DIR', 'NIRI_SOCKET', 'HYPRLAND_INSTANCE_SIGNATURE', 'WAYLAND_DISPLAY', 'DISPLAY', 'DBUS_SESSION_BUS_ADDRESS']) delete env[key];
env.NACRE_BRAIN_DIR = vault;
env.NACRE_COMPOSITOR = 'hyprland';
env.NACRE_BRAIN_DIRECT = '1';
function startBrain() { return spawn(binary, ['brain-control', 'serve'], { env, stdio: 'ignore' }); }
async function health() {
  for (let i = 0; i < 200; i += 1) {
    try { const r = await fetch(`${nativeUrl}/api/health`); if (r.ok && (await r.json()).ready) return; } catch {}
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  throw new Error('Brain server did not become ready');
}
const compositor = net.createServer(connection => {
  let request = '';
  connection.on('data', chunk => { request += chunk.toString(); });
  connection.on('end', () => {
    let response = 'ok';
    if (request.includes('clients')) response = fs.existsSync(launchCapture)
      ? JSON.stringify([{ address: '0x1', class: 'nacre-brain', workspace: { id: 1 }, pid: 1 }])
      : '[]';
    else if (request.includes('activeworkspace')) response = '{"id":1}';
    else if (request.startsWith('j/')) response = '{}';
    connection.end(response);
  });
});
await new Promise((resolve, reject) => compositor.listen(compositorSocket, error => error ? reject(error) : resolve()));
async function launchUrl() {
  await fs.promises.rm(launchCapture, { force: true });
  const child = spawn(binary, ['brain-launch', 'brain'], { env, stdio: 'ignore' });
  const [code] = await once(child, 'exit');
  if (code !== 0) throw new Error(`real Brain launcher exited with ${code}`);
  await until(() => Promise.resolve(fs.existsSync(launchCapture)), 'real launcher browser invocation');
  const args = JSON.parse(fs.readFileSync(launchCapture, 'utf8')).args;
  const app = args.find(argument => argument.startsWith('--app='));
  if (!app) throw new Error('real launcher did not pass a browser app URL');
  return app.slice('--app='.length);
}

let brain;
let socket;
let target;
let seq = 0;
let pausedSummary;
let stallEnabled = false;
let auth403Pending = true;
let injectedAuth403 = false;
const pending = new Map();
const events = new Map();
function cdp(method, params = {}) {
  return new Promise((resolve, reject) => {
    const id = ++seq;
    pending.set(id, { resolve, reject });
    socket.send(JSON.stringify({ id, method, params }));
  });
}
function evaluate(expression) {
  return cdp('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true, userGesture: true }).then(result => {
    if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text);
    return result.result.value;
  });
}
async function until(predicate, label, timeout = 10000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (await predicate()) return;
    await new Promise(resolve => setTimeout(resolve, 50));
  }
  throw new Error(`timed out waiting for ${label}`);
}
function waitEvent(name, timeout = 15000) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`timed out waiting for ${name}`)), timeout);
    events.set(name, value => { clearTimeout(timer); resolve(value); });
  });
}

try {
  if (!fs.statSync(binary).isFile()) throw new Error(`native helper missing: ${binary}`);
  const port = await cdpDiscoveryPort();
  const response = await fetch(`http://127.0.0.1:${port}/json/new?about:blank`, { method: 'PUT' });
  if (!response.ok) throw new Error(`CDP target creation failed: ${response.status}`);
  target = await response.json();
  socket = new WebSocket(target.webSocketDebuggerUrl);
  await once(socket, 'open');
  socket.addEventListener('message', event => {
    const message = JSON.parse(event.data);
    if (message.id) {
      const item = pending.get(message.id);
      if (item) { pending.delete(message.id); message.error ? item.reject(message.error) : item.resolve(message.result); }
    } else if (message.method === 'Fetch.requestPaused') {
      const request = message.params;
      if (new URL(request.request.url).pathname === '/api/summary' && auth403Pending) {
        auth403Pending = false;
        injectedAuth403 = true;
        cdp('Fetch.fulfillRequest', {
          requestId: request.requestId,
          responseCode: 403,
          responseHeaders: [{ name: 'Content-Type', value: 'application/json' }],
          body: Buffer.from('{"error":"unauthorized"}').toString('base64'),
        }).catch(() => {});
      } else if (stallEnabled && new URL(request.request.url).pathname === '/api/summary') {
        stallEnabled = false;
        pausedSummary = request.requestId;
        events.get('summary-paused')?.(request);
      } else cdp('Fetch.continueRequest', { requestId: request.requestId }).catch(() => {});
    }
  });
  await cdp('Runtime.enable');
  await cdp('Page.enable');
  await cdp('Page.bringToFront');
  await cdp('Fetch.enable', { patterns: [{ urlPattern: '*://127.0.0.1:*/api/summary', requestStage: 'Request' }] });

  const started = Date.now();
  brain = startBrain();
  await health();
  await cdp('Page.navigate', { url: 'about:blank' });
  await cdp('Page.navigate', { url: await launchUrl() });
  await until(() => evaluate('document.querySelector("#loading")?.hidden'), 'initial summary');
  if (!injectedAuth403) throw new Error('first-launch API request did not exercise the one-shot 403 auth-ready retry');
  await evaluate('window.__u12PageId="first-launch"');
  const initial = await evaluate(`({count:graph.noteCount,subjects:subjects().length,recent:graph.nodes.filter(node=>node.kind==="note").length,content:document.querySelector("#content").textContent,subjectRail:document.querySelector("#subjects").textContent,state:document.querySelector("#connections-status")?.dataset.state})`);
  if (initial.count !== 2000 || initial.subjects < 1) throw new Error(`summary did not load 2000-note fixture: ${JSON.stringify(initial)}`);
  if (initial.recent < 1 || !initial.content.includes('fixture note') || !initial.subjectRail.trim()) throw new Error(`summary did not display recent notes and subjects: ${JSON.stringify(initial)}`);
  if (initial.state !== 'building' && initial.state !== 'ready') throw new Error(`unexpected connection state: ${JSON.stringify(initial)}`);
  const usefulMs = Date.now() - started;
  if (usefulMs > 5000) throw new Error(`useful first view took ${usefulMs}ms, expected < 5000ms`);
  await until(() => evaluate('document.querySelector("#connections-status")?.dataset.state === "ready"'), 'background connections ready', 1800000);
  const readyWithoutReload = await evaluate('({state:document.querySelector("#connections-status").dataset.state,pageId:window.__u12PageId})');
  if (readyWithoutReload.pageId !== 'first-launch') throw new Error(`connections required a page reload: ${JSON.stringify(readyWithoutReload)}`);

  // Stall a subsequent first-class summary fetch. The real page timeout must
  // become visible, then Retry succeeds after the injected request is ended.
  stallEnabled = true;
  const retrySummaryPaused = waitEvent('summary-paused');
  const stalledAt = Date.now();
  await evaluate('location.reload()');
  await retrySummaryPaused;
  await until(() => evaluate('!document.querySelector("#load-error")?.hidden && !!document.querySelector("#retry")'), 'visible timeout error and Retry', 15000);
  const timeoutMs = Date.now() - stalledAt;
  if (timeoutMs > 12000) throw new Error(`error state took ${timeoutMs}ms, expected <= 12000ms`);
  if (!pausedSummary) throw new Error('summary request was not intercepted');
  await cdp('Fetch.failRequest', { requestId: pausedSummary, errorReason: 'TimedOut' }).catch(() => {});
  await evaluate('document.querySelector("#retry").click()');
  await until(() => evaluate('document.querySelector("#loading")?.hidden && document.querySelector("#load-error")?.hidden'), 'Retry summary success', 10000);
  const indexDir = path.join(vault, '.brain-state', 'index');
  if (!fs.existsSync(indexDir)) throw new Error('background discovery did not persist .brain-state/index');
  const indexFiles = fs.readdirSync(indexDir).map(name => path.join(indexDir, name));
  if (!indexFiles.length) throw new Error('persisted index directory is empty');
  const before = indexFiles.map(file => [file, fs.statSync(file).mtimeMs, fs.statSync(file).size]);
  // Restart evidence is provided by the next server's API state; the page's
  // authenticated session persistence itself remains covered by browser-native.mjs.
  brain.kill('SIGTERM');
  await once(brain, 'exit');
  const restartStart = Date.now();
  brain = startBrain();
  await health();
  await cdp('Page.navigate', { url: 'about:blank' });
  await cdp('Page.navigate', { url: await launchUrl() });
  await until(() => evaluate('document.querySelector("#loading")?.hidden && graph.noteCount === 2000 && document.querySelector("#connections-status")?.dataset.state === "ready"'), 'persisted summary after restart', 10000);
  const restartMs = Date.now() - restartStart;
  if (restartMs >= 2000) throw new Error(`persisted summary after restart took ${restartMs}ms, expected < 2000ms`);
  const after = indexFiles.map(file => [file, fs.statSync(file).mtimeMs, fs.statSync(file).size]);
  if (JSON.stringify(before) !== JSON.stringify(after)) throw new Error('index files changed during restart; expected reuse without rebuild');
  await evaluate('window.__u12PageId="after-restart"');
  fs.writeFileSync(path.join(vault, '2000.md'), '# Research fixture note 2000\nRecorded: 2026-11-05T12:00:00+00:00\nConfidence: verified\nWorlds: Research\n\nSynthetic watcher fixture.\n', { mode: 0o600 });
  await until(() => evaluate('graph.noteCount === 2001 && document.querySelector("#connections-status")?.dataset.state === "ready"'), 'vault-change summary and connections refresh', 1800000);
  const changed = await evaluate('({count:graph.noteCount,pageId:window.__u12PageId,state:document.querySelector("#connections-status").dataset.state})');
  if (changed.pageId !== 'after-restart') throw new Error(`vault changes required a page reload: ${JSON.stringify(changed)}`);
  console.log(JSON.stringify({ status: 'passed', notes: initial.count, subjects: initial.subjects, firstLaunch403AuthRetry: 'passed', timeoutErrorAndRetry: 'passed', timeoutMs, usefulMs, connectionsWithoutReload: 'passed', restartMs, persistedIndexReused: true, vaultChangeWithoutReload: changed.count, indexFiles: before.length }));
} finally {
  if (socket?.readyState === WebSocket.OPEN && target) {
    socket.close();
    await fetch(`http://127.0.0.1:${await cdpDiscoveryPort()}/json/close/${target.id}`).catch(() => {});
  }
  if (brain && brain.exitCode === null && brain.signalCode === null) {
    brain.kill('SIGTERM');
    await once(brain, 'exit');
  }
  compositor.close();
  fs.rmSync(temp, { recursive: true, force: true });
}
