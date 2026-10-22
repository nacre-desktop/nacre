// Fixture-only CDP security and functional check for native Brain HTTP.
// Run with a private HOME and a headless Chromium already exposing CDP:
// "$BRAIN_TEST_CHROME" --headless --remote-debugging-port=0 \
//   --user-data-dir="$HOME/.cache/nacre-brain-cdp" about:blank
// BRAIN_TEST_CDP=0 BRAIN_TEST_PROFILE="$HOME/.cache/nacre-brain-cdp" node brain/tests/browser-native.mjs
import fs from 'node:fs';
import http from 'node:http';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { once } from 'node:events';
import { spawn } from 'node:child_process';
import vm from 'node:vm';

const binary = path.resolve(process.env.BRAIN_TEST_NATIVE || 'target/debug/nacre-helper');
const webSource = path.resolve(process.env.BRAIN_TEST_WEB || 'brain/web');
if (!fs.statSync(webSource).isDirectory()) throw new Error(`Brain web assets missing: ${webSource}`);
const requestedCdpPort = Number(process.env.BRAIN_TEST_CDP || 17946);
const browserProfile = path.resolve(process.env.BRAIN_TEST_PROFILE || path.join(os.homedir(), '.cache/nacre-brain-cdp'));

async function resolveCdpPort() {
  if (requestedCdpPort > 0) return requestedCdpPort;
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

const cdpPort = await resolveCdpPort();
const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'nacre-brain-browser-'));
const home = path.join(temp, 'home');
const vault = path.join(temp, 'vault');
const installedWeb = path.join(home, '.local/share/nacre/brain/web');
fs.mkdirSync(home, { recursive: true, mode: 0o700 });
fs.mkdirSync(vault, { recursive: true, mode: 0o700 });
fs.mkdirSync(path.dirname(installedWeb), { recursive: true, mode: 0o700 });
fs.cpSync(webSource, installedWeb, { recursive: true });
const today = new Date().toISOString().slice(0, 10);
const fixtureSecret = 'PRIVATE_FIXTURE_NOTE_ONLY_7b93';
const files = {
  'wiki/qa-world.md': '# QA World\nEntity: world\nName: QA World\nProject: qa-world\nAliases: Quality fixture\n\nPublic test entity.\n',
  'inbox/qa.md': `# QA public note\nRecorded: ${today}T12:00:00+00:00\nConfidence: verified\nWorlds: QA World\nSource: browser fixture\n\nwaveform search fixture ${fixtureSecret}\n`,
  'projects/release.md': `# QA release note\nRecorded: ${today}T12:00:00+00:00\nConfidence: reported\nWorlds: QA World\nSource: browser fixture\n\nA public release fixture links to [[qa]].\n`,
};
for (const [relative, body] of Object.entries(files)) {
  const destination = path.join(vault, relative);
  fs.mkdirSync(path.dirname(destination), { recursive: true, mode: 0o700 });
  fs.writeFileSync(destination, body, { mode: 0o600 });
}

function freePort() {
  return new Promise((resolve, reject) => {
    const server = netServer();
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const { port } = server.address();
      server.close(error => error ? reject(error) : resolve(port));
    });
  });
}

function netServer() {
  return http.createServer();
}

const brainPort = await freePort();
const nativeUrl = `http://127.0.0.1:${brainPort}`;
const env = {
  ...process.env,
  HOME: home,
  NACRE_BRAIN_DIR: vault,
  NACRE_BRAIN_SEMANTICS: '0',
  NACRE_BRAIN_PORT: String(brainPort),
  NACRE_BRAIN_IDLE_SECONDS: '900',
};
for (const key of [
  'NIRI_SOCKET',
  'HYPRLAND_INSTANCE_SIGNATURE',
  'WAYLAND_DISPLAY',
  'DISPLAY',
  'DBUS_SESSION_BUS_ADDRESS',
]) delete env[key];

function startBrain() {
  return spawn(binary, ['brain-control', 'serve'], {
    env,
    stdio: 'ignore',
  });
}

async function waitHealth() {
  for (let i = 0; i < 100; i += 1) {
    try {
      const response = await fetch(`${nativeUrl}/api/health`);
      if (response.ok) {
        const health = await response.json();
        if (health.ready && health.authVersion === 1) return;
      }
    } catch {}
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  throw new Error('native Brain server did not become ready');
}

async function rawProtectedGet(cookieName, cookieValue) {
  return new Promise((resolve, reject) => {
    const client = net.createConnection({ host: '127.0.0.1', port: brainPort });
    let response = '';
    client.setTimeout(3000, () => client.destroy(new Error('raw protected request timed out')));
    client.once('error', reject);
    client.once('connect', () => {
      client.write([
        'GET /api/note?path=inbox%2Fqa.md HTTP/1.1',
        `Host: 127.0.0.1:${brainPort}`,
        `Cookie: ${cookieName}=${cookieValue}`,
        `Origin: ${nativeUrl}`,
        `Referer: ${nativeUrl}/`,
        'Sec-Fetch-Site: same-origin',
        'Connection: close',
        '',
        '',
      ].join('\r\n'));
    });
    client.on('data', chunk => { response += chunk.toString('utf8'); });
    client.once('end', () => resolve(response));
  });
}

const attacker = http.createServer((request, response) => {
  const target = new URL(request.url, 'http://127.0.0.1').searchParams.get('target');
  if (!target) {
    response.writeHead(400);
    response.end('missing target');
    return;
  }
  response.writeHead(200, {
    'Content-Type': 'text/html; charset=utf-8',
    'Cache-Control': 'no-store',
    'Content-Security-Policy': "default-src 'none'; script-src 'unsafe-inline'",
  });
  response.end(`<script>location.replace(${JSON.stringify(target)})</script>`);
});
attacker.listen(0, '127.0.0.1');
await once(attacker, 'listening');
const attackerPort = attacker.address().port;

let brain;
let socket;
let target;
let seq = 0;
const pending = new Map();
const exceptions = [];
const responses = [];
const extraInfo = [];

function cdp(method, params = {}) {
  return new Promise((resolve, reject) => {
    const id = ++seq;
    pending.set(id, { resolve, reject });
    socket.send(JSON.stringify({ id, method, params }));
  });
}

function until(predicate, label, attempts = 100) {
  return (async () => {
    for (let i = 0; i < attempts; i += 1) {
      if (await predicate()) return;
      await new Promise(resolve => setTimeout(resolve, 100));
    }
    throw new Error(`timed out waiting for ${label}`);
  })();
}

async function evaluate(expression) {
  const result = await cdp('Runtime.evaluate', {
    expression,
    returnByValue: true,
    awaitPromise: true,
    userGesture: true,
  });
  if (result.exceptionDetails) {
    throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text);
  }
  return result.result.value;
}

function openPage() {
  return pathToFileURL(path.join(home, '.local/state/nacre/brain/auth/open.html')).href;
}

try {
  if (!fs.statSync(binary).isFile()) throw new Error(`native helper missing: ${binary}`);
  brain = startBrain();
  await waitHealth();
  const targetResponse = await fetch(`http://127.0.0.1:${cdpPort}/json/new?about:blank`, { method: 'PUT' });
  if (!targetResponse.ok) throw new Error(`CDP target creation failed: ${targetResponse.status}`);
  target = await targetResponse.json();
  socket = new WebSocket(target.webSocketDebuggerUrl);
  await once(socket, 'open');
  socket.addEventListener('message', event => {
    const message = JSON.parse(event.data);
    if (message.id) {
      const item = pending.get(message.id);
      if (!item) return;
      pending.delete(message.id);
      message.error ? item.reject(message.error) : item.resolve(message.result);
      return;
    }
    if (message.method === 'Runtime.exceptionThrown') {
      exceptions.push(message.params.exceptionDetails.exception?.description || message.params.exceptionDetails.text);
    }
    if (message.method === 'Network.responseReceived') responses.push(message.params);
    if (message.method === 'Network.requestWillBeSentExtraInfo') extraInfo.push(message.params);
  });
  await cdp('Runtime.enable');
  await cdp('Network.enable');
  await cdp('Page.enable');
  await cdp('Page.navigate', { url: openPage() });
  await until(() => evaluate('typeof ready !== "undefined" && ready && graph.noteCount >= 3'), 'Brain graph');
  const graph = await evaluate('({count:graph.noteCount,nodes:graph.nodes.length,subjects:subjects().length})');
  if (graph.count !== 3 || graph.subjects < 1) throw new Error(`unexpected fixture graph ${JSON.stringify(graph)}`);

  await evaluate('$("search").value="waveform";$("search").dispatchEvent(new Event("input"))');
  await until(() => evaluate('searchIds !== null && searchIds.size > 0'), 'full-text search');
  const searchCount = await evaluate('searchIds.size');

  const note = await evaluate('graph.nodes.find(node=>node.kind==="note"&&node.path==="inbox/qa.md")');
  if (!note) throw new Error('fixture note missing from graph');
  await evaluate(`select(${JSON.stringify(note.id)})`);
  await until(() => evaluate('document.querySelector(".reader")?.textContent.includes("waveform search fixture")'), 'note reader');
  const sameOrigin = await evaluate(`fetch("/api/note?path=inbox/qa.md").then(async r=>({status:r.status,text:(await r.json()).text}))`);
  if (sameOrigin.status !== 200 || !sameOrigin.text.includes(fixtureSecret)) throw new Error('same-origin note API did not return fixture content');

  const assignment = await evaluate(`fetch("/api/action",{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({name:"assign-note",value:JSON.stringify({path:"inbox/qa.md",subjects:["Browser Fixture"]})})}).then(async r=>({status:r.status,body:await r.text()}))`);
  if (assignment.status !== 200 || !JSON.parse(assignment.body).ok) throw new Error(`allowed same-origin assignment failed: ${JSON.stringify(assignment)}`);

  // Exercise the served hook itself without relaxing the real page's CSP.
  const prefix = (await (await fetch(`${nativeUrl}/app.js`)).text()).split('\n')[0];
  const forwarded = [];
  const realm = {
    URL, URLSearchParams, Request, Headers,
    location: {href:nativeUrl+'/',origin:nativeUrl,hash:'',pathname:'/',search:''},
    localStorage: {getItem:()=> 'a'.repeat(64)},
    history: {replaceState:()=>{}},
    window: {fetch:async (input,init={})=>{forwarded.push({input,init});return {status:200};}},
  };
  vm.runInNewContext(prefix,realm);
  for (const input of [nativeUrl+'/api/brain',new URL(nativeUrl+'/api/brain'),new Request(nativeUrl+'/api/brain')]) {
    await realm.window.fetch(input);
    if (new Headers(forwarded.at(-1).init.headers).get('X-Nacre-Brain-Authorization') !== 'a'.repeat(64)) throw new Error('same-origin Fetch input lost the API credential');
  }
  for (const input of [`http://127.0.0.1:${attackerPort}/`,new URL(`http://127.0.0.1:${attackerPort}/`),new Request(`http://127.0.0.1:${attackerPort}/`)]) {
    await realm.window.fetch(input);
    if (new Headers(forwarded.at(-1).init.headers).has('X-Nacre-Brain-Authorization')) throw new Error('cross-origin Fetch input received the API credential');
  }

  const storedCredential = await evaluate('localStorage.getItem("nacre-brain-authorization")');
  await cdp('Page.navigate',{url:nativeUrl+'/#nacre-auth='+'0'.repeat(64)});
  await until(()=>evaluate('typeof ready !== "undefined" && ready && graph.noteCount >= 3'),'invalid credential fragment rejected');
  if (await evaluate('localStorage.getItem("nacre-brain-authorization")') !== storedCredential) throw new Error('unverified fragment poisoned the origin credential');

  const cookiesBefore = await cdp('Network.getAllCookies');
  const cookie = cookiesBefore.cookies.find(item => item.name === `nacre_brain_${brainPort}`);
  if (!cookie || !cookie.httpOnly) throw new Error('port-specific HttpOnly cookie was not stored');
  const bootstrapHtml = fs.readFileSync(path.join(home, '.local/state/nacre/brain/auth/open.html'), 'utf8');
  const bootstrapUrl = bootstrapHtml.match(/http:\/\/127\.0\.0\.1:[0-9]+\/\?token=[^" ]+/)?.[0];
  if (!bootstrapUrl) throw new Error('bootstrap URL missing from generated handoff file');
  const replay = await fetch(bootstrapUrl, { redirect: 'manual' });
  if (replay.status !== 403) throw new Error(`consumed bootstrap replay returned ${replay.status}`);

  responses.length = 0;
  extraInfo.length = 0;
  const attackTarget = `${nativeUrl}/api/note?path=${encodeURIComponent('inbox/qa.md')}`;
  await cdp('Page.navigate', { url: `http://127.0.0.1:${attackerPort}/?target=${encodeURIComponent(attackTarget)}` });
  await until(() => Promise.resolve(responses.some(item => item.response.url === attackTarget)), 'cross-port navigation response');
  const crossPortResponse = responses.find(item => item.response.url === attackTarget);
  await until(() => Promise.resolve(extraInfo.some(item => item.requestId === crossPortResponse.requestId)), 'cross-port request headers');
  const requestExtra = extraInfo.find(item => item.requestId === crossPortResponse.requestId);
  const crossPortBody = await cdp('Network.getResponseBody', { requestId: crossPortResponse.requestId });
  const bodyText = crossPortBody.base64Encoded ? Buffer.from(crossPortBody.body, 'base64').toString() : crossPortBody.body;
  if (crossPortResponse.response.status !== 403) throw new Error(`cross-port navigation returned ${crossPortResponse.response.status}, expected 403`);
  if (bodyText.includes(fixtureSecret)) throw new Error('cross-port navigation exposed fixture note content');
  const cookieHeader = Object.entries(requestExtra?.headers || {}).find(([name]) => name.toLowerCase() === 'cookie')?.[1] || '';
  const cookieWasSent = cookieHeader.includes(`nacre_brain_${brainPort}=`);
  if (!cookieWasSent) throw new Error('cross-port navigation did not exercise host-wide cookie leakage');

  const firstCookie = cookie.value;
  const stolenCookieResponse = await rawProtectedGet(`nacre_brain_${brainPort}`, firstCookie);
  if (!stolenCookieResponse.startsWith('HTTP/1.1 403')) throw new Error(`cookie-only request with spoofed same-origin metadata returned ${stolenCookieResponse.split('\r\n')[0]}`);
  if (stolenCookieResponse.includes(fixtureSecret)) throw new Error('cookie-only request exposed fixture note content');
  brain.kill('SIGTERM');
  await once(brain, 'exit');
  brain = startBrain();
  await waitHealth();
  const refreshedHtml = fs.readFileSync(path.join(home, '.local/state/nacre/brain/auth/open.html'), 'utf8');
  if (refreshedHtml === bootstrapHtml) throw new Error('server restart did not rotate bootstrap page');
  // Restore an existing tab through the separate auto-closing handoff origin.
  await evaluate('localStorage.removeItem("nacre-brain-authorization")');
  const withoutBearer = await evaluate('fetch("/api/note?path=inbox/qa.md").then(r=>r.status)');
  if (withoutBearer !== 403) throw new Error('cookie alone authorized the browser after restart');
  const handoffUrl = pathToFileURL(path.join(home, '.local/state/nacre/brain/auth/handoff.html')).href;
  const handoffTarget = await (await fetch(`http://127.0.0.1:${cdpPort}/json/new?${handoffUrl}`, { method: 'PUT' })).json();
  await until(() => evaluate('!!localStorage.getItem("nacre-brain-authorization")'), 'handoff credential restoration');
  await fetch(`http://127.0.0.1:${cdpPort}/json/close/${handoffTarget.id}`).catch(() => {});
  await cdp('Page.navigate', { url: nativeUrl + '/' });
  await until(() => evaluate('typeof ready !== "undefined" && ready && graph.noteCount >= 3'), 'Brain graph after restart');
  const cookiesAfter = await cdp('Network.getAllCookies');
  const persistentCookie = cookiesAfter.cookies.find(item => item.name === `nacre_brain_${brainPort}`);
  if (!persistentCookie || persistentCookie.value !== firstCookie) throw new Error('browser session cookie did not survive server restart');
  const readyAfterRestart = await evaluate('fetch("/api/health").then(async r=>({status:r.status,body:await r.json()}))');
  if (readyAfterRestart.status !== 200 || readyAfterRestart.body.authVersion !== 1) throw new Error('health failed after auth restart');
  if (exceptions.length) throw new Error(`browser exceptions: ${[...new Set(exceptions)].join('\n')}`);

  console.log(JSON.stringify({
    status: 'passed',
    graph,
    searchCount,
    noteReader: 'passed',
    sameOriginNoteStatus: sameOrigin.status,
    allowedAssignmentStatus: assignment.status,
    bootstrapReplayStatus: replay.status,
    crossPortNavigationStatus: crossPortResponse.response.status,
    crossPortCookieWasSent: cookieWasSent,
    crossPortLeakedFixtureContent: bodyText.includes(fixtureSecret),
    stolenCookieReplayStatus: 403,
    persistentCookieAfterRestart: true,
    handoffCredentialRestored: true,
    fetchInputOriginIsolation: 'passed',
    unverifiedFragmentRejected: true,
    browserExceptions: exceptions.length,
  }));
} finally {
  if (socket?.readyState === WebSocket.OPEN && target) {
    socket.close();
    await fetch(`http://127.0.0.1:${cdpPort}/json/close/${target.id}`).catch(() => {});
  }
  attacker.close();
  if (brain && brain.exitCode === null && brain.signalCode === null) {
    brain.kill('SIGTERM');
    await once(brain, 'exit');
  }
  fs.rmSync(temp, { recursive: true, force: true });
}
