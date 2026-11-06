// N0140 synthetic taxonomy acceptance. Run only on dev-1 inside the task
// container with a private Chromium CDP profile and the native helper built.
import fs from "node:fs";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { createHash } from "node:crypto";
import { once } from "node:events";
import { spawn } from "node:child_process";
import { writeHierarchyNotes, hierarchyWorlds } from "./hierarchy-fixture.mjs";

const root = path.resolve(".");
const binary = path.resolve(
  process.env.BRAIN_TEST_NATIVE || "target/debug/nacre-helper",
);
const webSource = path.resolve(process.env.BRAIN_TEST_WEB || "brain/web");
const profile = path.resolve(
  process.env.BRAIN_TEST_PROFILE ||
    path.join(os.homedir(), ".cache/nacre-brain-cdp"),
);
const requestedCdp = Number(process.env.BRAIN_TEST_CDP || 17946);
const temp = fs.mkdtempSync(path.join(os.tmpdir(), "nacre-brain-hierarchy-"));
const home = path.join(temp, "home");
const vault = path.join(temp, "vault");
const installedWeb = path.join(home, ".local/share/nacre/brain/web");
const evidence = path.join(root, ".lead/evidence");
fs.mkdirSync(home, { recursive: true, mode: 0o700 });
fs.mkdirSync(vault, { recursive: true, mode: 0o700 });
fs.mkdirSync(path.dirname(installedWeb), { recursive: true, mode: 0o700 });
fs.mkdirSync(evidence, { recursive: true, mode: 0o700 });
fs.cpSync(webSource, installedWeb, { recursive: true });
writeHierarchyNotes(vault);

function freePort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const port = server.address().port;
      server.close((error) => (error ? reject(error) : resolve(port)));
    });
  });
}
async function cdpPort() {
  if (requestedCdp > 0) return requestedCdp;
  const active = path.join(profile, "DevToolsActivePort");
  for (let i = 0; i < 100; i += 1) {
    try {
      const port = Number(fs.readFileSync(active, "utf8").split(/\r?\n/, 1)[0]);
      if (port > 0) return port;
    } catch {}
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(
    `Chromium did not publish DevToolsActivePort under ${profile}`,
  );
}
const port = await freePort();
const base = `http://127.0.0.1:${port}`;
const env = {
  ...process.env,
  HOME: home,
  NACRE_BRAIN_DIR: vault,
  NACRE_BRAIN_SEMANTICS: "0",
  NACRE_BRAIN_PORT: String(port),
  NACRE_BRAIN_IDLE_SECONDS: "900",
};
for (const key of [
  "NIRI_SOCKET",
  "HYPRLAND_INSTANCE_SIGNATURE",
  "WAYLAND_DISPLAY",
  "DISPLAY",
  "DBUS_SESSION_BUS_ADDRESS",
])
  delete env[key];
function startBrain() {
  return spawn(binary, ["brain-control", "serve"], { env, stdio: "ignore" });
}
async function health() {
  for (let i = 0; i < 200; i += 1) {
    try {
      const response = await fetch(`${base}/api/health`);
      if (response.ok && (await response.json()).ready) return;
    } catch {}
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error("Brain server did not become ready");
}
function bootstrapUrl() {
  const file = path.join(
    home,
    ".local/state/nacre/brain/auth/ports",
    `${port}.bootstrap.json`,
  );
  return `${base}/#nacre-bootstrap=${JSON.parse(fs.readFileSync(file, "utf8")).token}`;
}
async function until(test, label, timeout = 30000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (errors.length) throw new Error(`browser errors: ${errors.join("\n")}`);
    if (await test()) return;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  const state = await evaluate(
    '({count:graph.noteCount,nodes:graph.nodes.length,readerPath:document.querySelector("#inspector .reader-path")?.textContent,connections:graph.connections,error:document.querySelector("#load-error-message")?.textContent,status:document.querySelector("#index-status")?.textContent})',
  );
  throw new Error(`timed out waiting for ${label}: ${JSON.stringify(state)}`);
}

let brain;
let socket;
let target;
let sequence = 0;
const pending = new Map();
const errors = [];
function call(method, params = {}) {
  return new Promise((resolve, reject) => {
    const id = ++sequence;
    pending.set(id, { resolve, reject });
    socket.send(JSON.stringify({ id, method, params }));
  });
}
async function evaluate(expression) {
  const result = await call("Runtime.evaluate", {
    expression,
    returnByValue: true,
    awaitPromise: true,
    userGesture: true,
  });
  if (result.exceptionDetails)
    throw new Error(
      result.exceptionDetails.exception?.description ||
        result.exceptionDetails.text,
    );
  return result.result.value;
}
async function screenshot(name) {
  const result = await call("Page.captureScreenshot", {
    format: "png",
    captureBeyondViewport: false,
  });
  fs.writeFileSync(
    path.join(evidence, `n0140-${name}.png`),
    Buffer.from(result.data, "base64"),
  );
}
async function setPalette(mode) {
  const light = mode === "light";
  const tokens = light
    ? {
        "--bg": "#faf8f3",
        "--panel": "#f1eee8",
        "--raised": "#e9e5dc",
        "--text": "#25211b",
        "--muted": "#625b50",
        "--line": "#d3cabc",
        "--accent": "#73592a",
        "--secondary": "#655b4a",
        "--tertiary": "#725573",
        "--logo-highlight": "#4d506e",
      }
    : {
        "--bg": "#151310",
        "--panel": "#211f1c",
        "--raised": "#2b2925",
        "--text": "#e7e1db",
        "--muted": "#cec5b7",
        "--line": "#4c463c",
        "--accent": "#dbc492",
        "--secondary": "#d2c5ad",
        "--tertiary": "#becca2",
        "--logo-highlight": "#eee9f4",
      };
  await evaluate(
    `(()=>{const r=document.documentElement;r.style.colorScheme=${JSON.stringify(mode)};for(const [k,v] of Object.entries(${JSON.stringify(tokens)})){r.style.setProperty(k,v)}})()`,
  );
  await new Promise((resolve) => setTimeout(resolve, 100));
}
async function contrastReport() {
  return evaluate(
    `(()=>{const rgb=s=>s.match(/[0-9.]+/g).slice(0,3).map(Number);const lum=s=>{const c=rgb(s).map(v=>v/255).map(v=>v<=.04045?v/12.92:((v+.055)/1.055)**2.4);return .2126*c[0]+.7152*c[1]+.0722*c[2]};const bg=getComputedStyle(document.querySelector("#graph-view")).backgroundColor,L=lum(bg);const rows=Array.from(document.querySelectorAll('#graph-legend > span')).map(row=>({name:row.lastElementChild.textContent,color:getComputedStyle(row.querySelector('i')).backgroundColor}));const ratios=rows.map(r=>{const x=lum(r.color);return (Math.max(x,L)+.05)/(Math.min(x,L)+.05)});return {rows,ratios,background:bg}})()`,
  );
}

try {
  if (!fs.statSync(binary).isFile())
    throw new Error(`native helper missing: ${binary}`);
  brain = startBrain();
  await health();
  const response = await fetch(
    `http://127.0.0.1:${await cdpPort()}/json/new?about:blank`,
    { method: "PUT" },
  );
  if (!response.ok)
    throw new Error(`CDP target creation failed: ${response.status}`);
  target = await response.json();
  socket = new WebSocket(target.webSocketDebuggerUrl);
  await once(socket, "open");
  socket.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    if (message.id) {
      const item = pending.get(message.id);
      if (item) {
        pending.delete(message.id);
        message.error
          ? item.reject(message.error)
          : item.resolve(message.result);
      }
    } else if (message.method === "Runtime.exceptionThrown") {
      errors.push(
        message.params.exceptionDetails.exception?.description ||
          message.params.exceptionDetails.text,
      );
    } else if (
      message.method === "Runtime.consoleAPICalled" &&
      message.params.type === "error"
    ) {
      errors.push(
        message.params.args
          .map((arg) => arg.description || String(arg.value))
          .join(" "),
      );
    } else if (
      message.method === "Log.entryAdded" &&
      message.params.entry.level === "error"
    )
      errors.push(message.params.entry.text);
  });
  await call("Runtime.enable");
  await call("Log.enable");
  await call("Page.enable");
  await call("Emulation.setDeviceMetricsOverride", {
    width: 1600,
    height: 1000,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await call("Page.navigate", { url: bootstrapUrl() });
  await until(
    () =>
      evaluate(
        'document.querySelector("#loading")?.hidden && document.querySelector("#connections-status")?.dataset.state === "ready" && graph.nodes.filter(n=>["note","archive","digest"].includes(n.kind)).length === 2001',
      ),
    "complete 2,000-note graph",
    30000,
  );
  // N0144: the Wiki is the landing view; the overview is one click away.
  await evaluate("document.querySelector('[data-view=overview]').click()");

  const model = await evaluate(
    `({worlds:graph.worlds,review:graph.reviewCount,notes:graph.nodes.filter(n=>['note','archive','digest'].includes(n.kind)),archive:graph.nodes.filter(n=>n.kind==='archive').length,digests:graph.nodes.filter(n=>n.kind==='digest').length})`,
  );
  if (
    model.worlds.length !== 7 ||
    model.review !== 15 ||
    model.notes.length !== 2001 ||
    model.notes.filter((note) => !note.path.startsWith("wiki/")).length !==
      2000 ||
    model.archive < 1 ||
    model.digests < 1
  )
    throw new Error(
      `synthetic model mismatch: ${JSON.stringify({ worlds: model.worlds.length, review: model.review, notes: model.notes.length, archive: model.archive, digests: model.digests })}`,
    );
  const initialNames = await evaluate(
    'Array.from(document.querySelectorAll("#subjects .world-row .name"),n=>n.textContent)',
  );
  if (
    initialNames.length !== 7 ||
    initialNames.some((name) => !hierarchyWorlds.includes(name))
  )
    throw new Error(`sidebar worlds mismatch: ${JSON.stringify(initialNames)}`);
  if (
    (await evaluate(
      'document.querySelector("#subjects .review-link .count")?.textContent',
    )) !== "15"
  )
    throw new Error("Review count badge is missing or incorrect");
  if (
    await evaluate(
      '!!document.querySelector("#subjects .review-link")?.closest(".world-row")',
    )
  )
    throw new Error("Review is nested under a world");
  const suggestions = await evaluate(
    'document.querySelector("details.suggestions")?.textContent',
  );
  if (
    !suggestions?.includes("Synthetic suggested grouping") ||
    (await evaluate(
      'document.querySelector("details.suggestions")?.open !== false',
    ))
  )
    throw new Error("synthetic suggestion is missing or expanded by default");
  const autoArea = model.worlds[0].areas.find((area) => area.new);
  const proposal = model.worlds[0].areas.find((area) => area.proposed);
  if (!autoArea || !proposal)
    throw new Error("new auto and proposed areas are not represented");
  await evaluate(`document.querySelector('#subjects .world-expand').click()`);
  await until(
    () =>
      evaluate(
        'document.querySelectorAll("#subjects .area-link").length === 5',
      ),
    "expanded world areas",
  );
  if (
    !(await evaluate(
      'document.querySelector("#subjects .world-row .world-expand").getAttribute("aria-expanded") === "true"',
    ))
  )
    throw new Error("expanded state is not exposed to assistive technology");
  if (
    !(await evaluate(
      `Array.from(document.querySelectorAll('#subjects .area-link')).some(n=>n.textContent.includes('New: Proposed Grove'))`,
    ))
  )
    throw new Error("proposed area is not labeled New");
  if (
    !(await evaluate(
      `Array.from(document.querySelectorAll('#subjects .area-link .new-badge')).some(n=>n.textContent==='New')`,
    ))
  )
    throw new Error("today-created auto area badge is missing");
  if (
    await evaluate(
      `document.querySelectorAll('#subjects .area-link')[1].querySelector('.new-badge') !== null`,
    )
  )
    throw new Error("old auto area incorrectly retains the New badge");
  if (
    !(await evaluate(
      `document.querySelectorAll('#subjects .area-link')[2].classList.contains('proposed')`,
    ))
  )
    throw new Error("proposed area is not visually marked as proposed");
  await evaluate(`document.querySelector('#subjects .world-expand').click()`);
  await until(
    () =>
      evaluate(
        'document.querySelectorAll("#subjects .area-link").length === 0',
      ),
    "collapsed world areas",
  );
  await evaluate(`document.querySelector('#subjects .world-expand').click()`);
  await until(
    () =>
      evaluate(
        'document.querySelectorAll("#subjects .area-link").length === 5',
      ),
    "re-expanded world areas",
  );
  await evaluate(
    `document.querySelector('#subjects .world-row .subject').click()`,
  );
  await until(
    () =>
      evaluate(
        `document.querySelector('#title').textContent === ${JSON.stringify(model.worlds[0].label)}`,
      ),
    "world page",
  );
  if (
    !(await evaluate(
      `document.querySelector('#content').textContent.includes('Areas') && document.querySelector('#content').textContent.includes('Recent knowledge')`,
    ))
  )
    throw new Error("world page is missing areas or recent notes");
  await evaluate(`document.querySelector('#subjects .area-link').click()`);
  await until(
    () =>
      evaluate(
        `document.querySelector('#breadcrumbs').textContent.includes(${JSON.stringify(model.worlds[0].label)})`,
      ),
    "area breadcrumb",
  );
  const areaPage = await evaluate(
    '({crumbs:document.querySelector("#breadcrumbs").textContent,title:document.querySelector("#title").textContent,content:document.querySelector("#content").textContent})',
  );
  if (
    !areaPage.crumbs.includes(areaPage.title) ||
    !(await evaluate('!!document.querySelector("#content .wiki-link")'))
  )
    throw new Error(
      `area page lacks breadcrumb/wiki content: ${JSON.stringify(areaPage)}`,
    );
  await evaluate('document.querySelector("#content .wiki-link").click()');
  await until(
    () =>
      evaluate(
        '!document.querySelector("#inspector").hidden && document.querySelector("#inspector").textContent.includes("Synthetic wiki page")',
      ),
    "area wiki reader",
  );
  await evaluate('document.querySelector("#back").click()');
  await evaluate("document.querySelector('[data-view=notes]').click()");
  await until(
    () => evaluate('!!document.querySelector(".note-row")'),
    "area notes",
  );
  const chips = await evaluate(
    'document.querySelector(".note-row .note-path-chips")?.textContent',
  );
  if (
    !chips?.includes(model.worlds[0].label) ||
    !chips.includes(areaPage.title)
  )
    throw new Error(`note is missing world/area chips: ${chips}`);
  await evaluate('document.querySelector(".note-row").click()');
  await until(
    () => evaluate('!document.querySelector("#inspector").hidden'),
    "note reader",
  );
  await evaluate('document.querySelector("#back").click()');
  await until(
    () => evaluate('!!document.querySelector(".note-row")'),
    "return to area notes",
  );
  const noteId = `note:${createHash("sha256").update("stable-move.md").digest("hex").slice(0, 16)}`;
  const archiveBefore = await evaluate(
    'document.querySelector("#content .filters button:last-child")?.textContent',
  );
  if (archiveBefore !== "Show archive")
    throw new Error(`archives are not hidden by default: ${archiveBefore}`);
  const archivedLabel = await evaluate(
    'graph.nodes.find(n=>n.kind==="archive"&&n.areas?.includes("Alder Area 1"))?.label',
  );
  if (
    !archivedLabel ||
    (await evaluate(
      `Array.from(document.querySelectorAll('#content .note-row .note-title')).some(n=>n.textContent===${JSON.stringify(archivedLabel)})`,
    ))
  )
    throw new Error("archive fixture is missing or visible by default");
  await evaluate(
    `document.querySelector('#content .filters button:last-child').click()`,
  );
  await until(
    () =>
      evaluate(
        `document.querySelector('#content').textContent.includes('Hide archive') && Array.from(document.querySelectorAll('#content .note-row .note-title')).some(n=>n.textContent===${JSON.stringify(archivedLabel)})`,
      ),
    "archive visibility toggle",
  );
  await evaluate(`document.querySelector('#subjects .review-link').click()`);
  await until(
    () => evaluate('document.querySelector("#title").textContent === "Review"'),
    "Review queue",
  );
  if (
    (await evaluate(
      'document.querySelectorAll("#content .note-row").length',
    )) !== 15
  )
    throw new Error(
      "Review queue does not contain exactly the 15 flagged notes",
    );
  await evaluate("document.querySelector('#overview-link').click()");
  await until(
    () =>
      evaluate(
        '!document.querySelector("#title").textContent.includes("Review")',
      ),
    "overview",
  );
  await evaluate('document.documentElement.style.colorScheme="light"');
  await screenshot("overview-light");
  await evaluate('document.querySelector("#graph-toggle").click()');
  await until(
    () => evaluate('!document.querySelector("#graph-view").hidden'),
    "graph view",
  );
  await until(
    () =>
      evaluate(
        'document.querySelector("#graph-view").dataset.layout === "settled"',
      ),
    "graph layout",
    30000,
  );
  const graphKinds = await evaluate(
    '({worlds:window.BrainGraphUI.nodes.filter(n=>n.kind==="world").length,areas:window.BrainGraphUI.nodes.filter(n=>n.kind==="area").length,legend:document.querySelector("#graph-legend").textContent})',
  );
  if (graphKinds.worlds !== 7 || graphKinds.areas < 29)
    throw new Error(
      `graph hierarchy nodes mismatch: ${JSON.stringify(graphKinds)}`,
    );
  const kindOptions = await evaluate(
    'Array.from(document.querySelectorAll("#graph-kind option"),option=>option.value)',
  );
  if (!kindOptions.includes("archive") || !kindOptions.includes("digest"))
    throw new Error(
      `archive/digest graph filters are missing: ${JSON.stringify(kindOptions)}`,
    );
  if (!(await evaluate('!!document.querySelector("#graph-archive-toggle")')))
    throw new Error("graph archive visibility toggle is missing");
  await setPalette("light");
  const lightContrast = await contrastReport();
  if (
    lightContrast.rows.length !== 7 ||
    hierarchyWorlds.some(
      (world) => !lightContrast.rows.some((row) => row.name === world),
    ) ||
    new Set(lightContrast.rows.map((row) => row.color)).size !== 7 ||
    lightContrast.ratios.some((ratio) => !Number.isFinite(ratio) || ratio < 3)
  )
    throw new Error(
      `light world colors fail legend/uniqueness/3:1 contrast: ${JSON.stringify(lightContrast)}`,
    );
  await screenshot("graph-light");
  const beforeMove = await evaluate(
    `fetch('/api/brain').then(r=>r.json()).then(g=>g.nodes.find(n=>n.path==='notes/stable-move.md')?.id)`,
  );
  if (beforeMove !== noteId)
    throw new Error(`stable note id mismatch before move: ${beforeMove}`);
  await evaluate(
    `window.BrainGraphUI.close(); select(${JSON.stringify(noteId)}, {view:"read"})`,
  );
  await until(
    () =>
      evaluate(
        'document.querySelector("#inspector .reader-path")?.textContent === "notes/stable-move.md" && document.querySelector("#inspector").textContent.includes("Synthetic hierarchy fixture 0.")',
      ),
    "reader before archive move",
  );
  const baselineCount = model.notes.length;
  fs.mkdirSync(path.join(vault, "archive"), { recursive: true });
  fs.renameSync(
    path.join(vault, "notes/stable-move.md"),
    path.join(vault, "archive/stable-move.md"),
  );
  const moveStarted = Date.now();
  let moved;
  while (Date.now() - moveStarted < 5000) {
    moved = await evaluate(
      `fetch('/api/brain').then(r=>r.json()).then(g=>g.nodes.find(n=>n.path==='archive/stable-move.md'))`,
    );
    if (moved?.id) break;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  if (!moved || moved.id !== beforeMove || Date.now() - moveStarted >= 5000)
    throw new Error(
      `archive move did not preserve the note id within 5s: ${JSON.stringify(moved)}`,
    );
  await until(
    () =>
      evaluate(
        `state.selected === ${JSON.stringify(noteId)} && document.querySelector("#inspector .reader-path")?.textContent === "archive/stable-move.md"`,
      ),
    "open reader path after archive move",
    Math.max(1, 5000 - (Date.now() - moveStarted)),
  );
  await evaluate('home(); window.BrainGraphUI.open("all")');
  await until(
    () =>
      evaluate(
        'document.querySelector("#graph-view").dataset.layout === "settled"',
      ),
    "graph after archive move",
  );
  await setPalette("light");
  await screenshot("graph-light");
  fs.writeFileSync(
    path.join(vault, "notes/watcher-active.md"),
    `# Active watcher fixture\nRecorded: ${new Date().toISOString()}\nWorlds: World Alder\nAreas: Alder Area 1\n\nSynthetic new note.\n`,
    { mode: 0o600 },
  );
  const noteStarted = Date.now();
  let refreshedCount = baselineCount;
  while (Date.now() - noteStarted < 5000) {
    refreshedCount = await evaluate(
      `fetch('/api/brain').then(r=>r.json()).then(g=>g.noteCount)`,
    );
    if (
      refreshedCount > baselineCount &&
      (await evaluate(
        `graph.nodes.some(n=>n.path==='notes/watcher-active.md')`,
      ))
    )
      break;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  if (refreshedCount <= baselineCount || Date.now() - noteStarted >= 5000)
    throw new Error(
      "new note did not appear from the active watcher within 5s",
    );
  const activeWatcherMs = Date.now() - noteStarted;
  await setPalette("dark");
  const darkContrast = await contrastReport();
  if (
    darkContrast.rows.length !== 7 ||
    hierarchyWorlds.some(
      (world) => !darkContrast.rows.some((row) => row.name === world),
    ) ||
    new Set(darkContrast.rows.map((row) => row.color)).size !== 7 ||
    darkContrast.ratios.some((ratio) => !Number.isFinite(ratio) || ratio < 3)
  )
    throw new Error(
      `dark world colors fail legend/uniqueness/3:1 contrast: ${JSON.stringify(darkContrast)}`,
    );
  await screenshot("graph-dark");
  await evaluate('document.querySelector("#graph-toggle").click()');
  await until(
    () => evaluate('document.querySelector("#graph-view").hidden'),
    "return from graph",
  );
  await evaluate('document.querySelector("#overview-link").click()');
  await setPalette("light");
  await screenshot("overview-light");
  await setPalette("dark");
  await screenshot("overview-dark");
  if (errors.length)
    throw new Error(
      `browser console errors: ${[...new Set(errors)].join("\n")}`,
    );
  console.log(
    JSON.stringify({
      status: "passed",
      worlds: 7,
      areas: model.worlds.reduce((n, w) => n + w.areas.length, 0),
      syntheticNotes: 2000,
      wikiPages: 1,
      review: model.review,
      archiveKinds: model.archive,
      digestKinds: model.digests,
      filenameId: noteId,
      graph: graphKinds,
      contrast: {
        light: Math.min(...lightContrast.ratios),
        dark: Math.min(...darkContrast.ratios),
      },
      activeWatcherMs,
      screenshots: [
        ".lead/evidence/n0140-overview-light.png",
        ".lead/evidence/n0140-overview-dark.png",
        ".lead/evidence/n0140-graph-light.png",
        ".lead/evidence/n0140-graph-dark.png",
      ],
      consoleErrors: 0,
    }),
  );
} finally {
  if (socket?.readyState === WebSocket.OPEN && target) {
    socket.close();
    await fetch(
      `http://127.0.0.1:${await cdpPort()}/json/close/${target.id}`,
    ).catch(() => {});
  }
  if (brain && brain.exitCode === null && brain.signalCode === null) {
    brain.kill("SIGTERM");
    await once(brain, "exit");
  }
  fs.rmSync(temp, { recursive: true, force: true });
}
