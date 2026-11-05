// U7 graph-view acceptance on a synthetic 2,000-note vault.
// Run in a private HOME with headless Chromium exposing CDP:
// BRAIN_TEST_CDP=0 BRAIN_TEST_PROFILE=<private-profile> node brain/tests/browser-graph.mjs
import fs from "node:fs";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { once } from "node:events";
import { spawn } from "node:child_process";

const root = path.resolve(".");
const binary = path.resolve(
  process.env.BRAIN_TEST_NATIVE || "target/debug/nacre-helper",
);
const webSource = path.resolve(process.env.BRAIN_TEST_WEB || "brain/web");
const requestedCdp = Number(process.env.BRAIN_TEST_CDP || 17946);
const profile = path.resolve(
  process.env.BRAIN_TEST_PROFILE ||
    path.join(os.homedir(), ".cache/nacre-brain-cdp"),
);
const temp = fs.mkdtempSync(path.join(os.tmpdir(), "nacre-brain-graph-"));
const home = path.join(temp, "home");
const vault = path.join(temp, "vault");
const installedWeb = path.join(home, ".local/share/nacre/brain/web");
const evidence = path.join(root, ".lead/evidence");
fs.mkdirSync(home, { recursive: true, mode: 0o700 });
fs.mkdirSync(vault, { recursive: true, mode: 0o700 });
fs.mkdirSync(path.dirname(installedWeb), { recursive: true, mode: 0o700 });
fs.mkdirSync(evidence, { recursive: true, mode: 0o700 });
fs.cpSync(webSource, installedWeb, { recursive: true });

const worlds = ["Research", "Software", "Music", "Climbing", "Electronics"];
for (let i = 0; i < 2000; i += 1) {
  const world = worlds[i % worlds.length];
  const prev = i > 0 ? `\nRelated: [[${String(i - 1).padStart(4, "0")}]]` : "";
  fs.writeFileSync(
    path.join(vault, `${String(i).padStart(4, "0")}.md`),
    `# ${world} fixture note ${i}\nRecorded: 2026-11-05T12:00:00+00:00\nConfidence: verified\nWorlds: ${world}\n\nSynthetic graph fixture ${i}.${prev}\n`,
    { mode: 0o600 },
  );
}
for (const [index, name] of worlds.entries()) {
  const destination = path.join(vault, "wiki", `${name.toLowerCase()}.md`);
  fs.mkdirSync(path.dirname(destination), { recursive: true, mode: 0o700 });
  const entity = index === 0 ? "world" : "topic";
  const parent = index === 0 ? "" : "\nParent: Research";
  fs.writeFileSync(
    destination,
    `# ${name}\nEntity: ${entity}\nName: ${name}${parent}\n\nSynthetic wiki fixture for ${name}.\n`,
    { mode: 0o600 },
  );
}

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
  const file = path.join(profile, "DevToolsActivePort");
  for (let i = 0; i < 100; i += 1) {
    try {
      const value = Number(fs.readFileSync(file, "utf8").split(/\r?\n/, 1)[0]);
      if (value > 0) return value;
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
  const { token } = JSON.parse(fs.readFileSync(file, "utf8"));
  return `${base}/#nacre-bootstrap=${token}`;
}
async function until(test, label, timeout = 30000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (errors.length) throw new Error(`browser errors: ${errors.join("\n")}`);
    if (await test()) return;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`timed out waiting for ${label}`);
}

let brain;
let socket;
let target;
let sequence = 0;
let buildingInjected = false;
const graphRequests = [];
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
  const result = await call("Page.captureScreenshot", { format: "png" });
  fs.writeFileSync(
    path.join(evidence, `n0136-graph-${name}.png`),
    Buffer.from(result.data, "base64"),
  );
}

try {
  if (!fs.statSync(binary).isFile())
    throw new Error(`native helper missing: ${binary}`);
  brain = startBrain();
  await health();
  const targetResponse = await fetch(
    `http://127.0.0.1:${await cdpPort()}/json/new?about:blank`,
    { method: "PUT" },
  );
  if (!targetResponse.ok)
    throw new Error(`CDP target creation failed: ${targetResponse.status}`);
  target = await targetResponse.json();
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
    } else if (message.method === "Runtime.exceptionThrown")
      errors.push(
        message.params.exceptionDetails.exception?.description ||
          message.params.exceptionDetails.text,
      );
    else if (
      message.method === "Runtime.consoleAPICalled" &&
      message.params.type === "error"
    )
      errors.push(
        message.params.args
          .map((arg) => arg.description || String(arg.value))
          .join(" "),
      );
    else if (
      message.method === "Log.entryAdded" &&
      message.params.entry.level === "error"
    )
      errors.push(message.params.entry.text);
    else if (message.method === "Fetch.requestPaused") {
      const request = message.params;
      if (new URL(request.request.url).pathname === "/api/graph")
        graphRequests.push(
          new URL(request.request.url).searchParams.get("scope"),
        );
      if (
        !buildingInjected &&
        new URL(request.request.url).pathname === "/api/graph"
      ) {
        buildingInjected = true;
        call("Fetch.fulfillRequest", {
          requestId: request.requestId,
          responseCode: 200,
          responseHeaders: [
            { name: "Content-Type", value: "application/json" },
          ],
          body: Buffer.from('{"state":"building"}').toString("base64"),
        }).catch(() => {});
      } else
        call("Fetch.continueRequest", { requestId: request.requestId }).catch(
          () => {},
        );
    }
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
  await call("Fetch.enable", {
    patterns: [
      { urlPattern: "*://127.0.0.1:*/api/graph*", requestStage: "Request" },
    ],
  });
  await call("Page.navigate", { url: bootstrapUrl() });
  await until(
    () => evaluate('document.querySelector("#loading")?.hidden'),
    "Brain summary",
  );
  const summary = await evaluate(
    '({notes:graph.noteCount, sample:graph.nodes.find(n=>n.kind==="note")?.id, error:document.querySelector("#load-error-message").textContent})',
  );
  if (summary.notes !== 2000 + worlds.length || !summary.sample)
    throw new Error(`fixture summary mismatch: ${JSON.stringify(summary)}`);

  const start = Date.now();
  await evaluate('document.querySelector("#graph-toggle").click()');
  await until(
    () => evaluate('!document.querySelector("#graph-view").hidden'),
    "graph view open",
  );
  await until(
    () =>
      evaluate(
        '/building/i.test(document.querySelector("#graph-status").textContent)',
      ),
    "building status",
  );
  if (!buildingInjected) throw new Error("building response was not injected");
  await until(
    () =>
      evaluate(
        'document.querySelector("#graph-status").textContent.trim()===""',
      ),
    "ready graph",
    1800000,
  );
  const all = await evaluate('fetch("/api/graph?scope=all").then(r=>r.json())');
  if (all.state !== "ready")
    throw new Error(`all graph did not become ready: ${all.state}`);
  const graphReadyMs = Date.now() - start;
  const readyAt = Date.now();
  await until(
    () =>
      evaluate(
        'document.querySelector("#graph-view").dataset.layout === "settled"',
      ),
    "settled layout",
    5000,
  );
  const settleMs = Date.now() - readyAt;
  const fittedBounds = await evaluate(`(()=>{
    const canvas=document.querySelector("#graph-canvas"),r=canvas.getBoundingClientRect();
    const points=window.BrainGraphUI.nodes.map(n=>window.BrainGraphUI.screenNode(n.id));
    const xs=points.map(p=>p.x),ys=points.map(p=>p.y);
    return {left:Math.min(...xs)-r.left,right:r.right-Math.max(...xs),top:Math.min(...ys)-r.top,bottom:r.bottom-Math.max(...ys),width:Math.max(...xs)-Math.min(...xs),height:Math.max(...ys)-Math.min(...ys),canvasWidth:r.width,canvasHeight:r.height};
  })()`);
  if (
    Math.min(
      fittedBounds.left,
      fittedBounds.right,
      fittedBounds.top,
      fittedBounds.bottom,
    ) < 40 ||
    Math.max(
      fittedBounds.width / fittedBounds.canvasWidth,
      fittedBounds.height / fittedBounds.canvasHeight,
    ) < 0.65
  )
    throw new Error(
      `graph did not fit its viewport: ${JSON.stringify(fittedBounds)}`,
    );
  if (
    !(await evaluate(
      'document.querySelector("#graph-legend").textContent.includes("Colour = world") && document.querySelector("#graph-legend").textContent.includes("Size = note count")',
    ))
  )
    throw new Error("graph legend is missing");

  // Narrow by clicking a structural category on the actual canvas.
  const category = all.nodes.find((n) =>
    ["world", "area", "subject", "hub", "topic"].includes(n.kind),
  );
  if (!category)
    throw new Error(
      `all graph has no category nodes: ${JSON.stringify(all.nodes.slice(0, 8))}`,
    );
  {
    const categoryPoint = await evaluate(
      `window.BrainGraphUI.screenNode(${JSON.stringify(category.id)})`,
    );
    if (!categoryPoint)
      throw new Error(
        `category node ${category.id} is not visible to the canvas`,
      );
    await call("Input.dispatchMouseEvent", {
      type: "mousePressed",
      x: categoryPoint.x,
      y: categoryPoint.y,
      button: "left",
      clickCount: 1,
    });
    await call("Input.dispatchMouseEvent", {
      type: "mouseReleased",
      x: categoryPoint.x,
      y: categoryPoint.y,
      button: "left",
      clickCount: 1,
    });
    await until(
      () =>
        evaluate(
          'document.querySelector("#graph-status").textContent.trim()===""',
        ),
      "narrowed graph",
      1800000,
    );
    if (!graphRequests.some((scope) => /^(world|area):/.test(scope || "")))
      throw new Error(
        `category click did not request a narrowed scope: ${JSON.stringify(graphRequests)}`,
      );
    await until(
      () =>
        evaluate(
          'document.querySelector("#graph-view").dataset.layout === "settled"',
        ),
      "settled narrowed graph",
      5000,
    );
    await screenshot("narrowed");
  }

  await evaluate('window.BrainGraphUI.open("all",false)');
  await until(
    () =>
      evaluate(
        'document.querySelector("#graph-view").dataset.layout === "settled"',
      ),
    "all graph before drag",
    5000,
  );
  const rafStart = Date.now();
  await evaluate(
    "window.__graphRafFrames=0;window.__graphRafRun=true;(function tick(){if(!window.__graphRafRun)return;window.__graphRafFrames++;requestAnimationFrame(tick)})()",
  );
  const rect = await evaluate(
    '(()=>{const r=document.querySelector("#graph-canvas").getBoundingClientRect();return{x:r.x,y:r.y,w:r.width,h:r.height}})()',
  );
  await call("Input.dispatchMouseEvent", {
    type: "mouseMoved",
    x: rect.x + 8,
    y: rect.y + 8,
  });
  await call("Input.dispatchMouseEvent", {
    type: "mousePressed",
    x: rect.x + 8,
    y: rect.y + 8,
    button: "left",
  });
  for (let i = 0; i < 50; i += 1) {
    await call("Input.dispatchMouseEvent", {
      type: "mouseMoved",
      x: rect.x + 8 + i * 2,
      y: rect.y + 8 + i,
    });
    await new Promise((resolve) => setTimeout(resolve, 16));
  }
  await call("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    x: rect.x + 68,
    y: rect.y + 38,
    button: "left",
  });
  const dragFrames = await evaluate(
    "window.__graphRafRun=false;window.__graphRafFrames",
  );
  const dragFps = dragFrames / ((Date.now() - rafStart) / 1000);
  await evaluate('document.querySelector("#graph-fit").click()');
  const fitButtonCenter = await evaluate(`(()=>{
    const r=document.querySelector("#graph-canvas").getBoundingClientRect();
    const p=window.BrainGraphUI.nodes.map(n=>window.BrainGraphUI.screenNode(n.id));
    return {x:(Math.min(...p.map(n=>n.x))+Math.max(...p.map(n=>n.x)))/2-r.left-r.width/2,y:(Math.min(...p.map(n=>n.y))+Math.max(...p.map(n=>n.y)))/2-r.top-r.height/2};
  })()`);
  if (Math.max(Math.abs(fitButtonCenter.x), Math.abs(fitButtonCenter.y)) > 24)
    throw new Error(
      `Fit button did not recenter the graph: ${JSON.stringify(fitButtonCenter)}`,
    );
  await call("Emulation.setDeviceMetricsOverride", {
    width: 1400,
    height: 900,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await new Promise((resolve) => setTimeout(resolve, 150));
  const resizePadding = await evaluate(`(()=>{
    const r=document.querySelector("#graph-canvas").getBoundingClientRect();
    const p=window.BrainGraphUI.nodes.map(n=>window.BrainGraphUI.screenNode(n.id));
    return Math.min(Math.min(...p.map(n=>n.x))-r.left,r.right-Math.max(...p.map(n=>n.x)),Math.min(...p.map(n=>n.y))-r.top,r.bottom-Math.max(...p.map(n=>n.y)));
  })()`);
  if (resizePadding < 40)
    throw new Error(`resize did not fit graph: ${resizePadding}px padding`);
  await call("Emulation.setDeviceMetricsOverride", {
    width: 1600,
    height: 1000,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await new Promise((resolve) => setTimeout(resolve, 150));

  const noteScope = `note:${summary.sample}`;
  const directScopeStart = Date.now();
  const scoped = await evaluate(
    `fetch("/api/graph?scope="+encodeURIComponent(${JSON.stringify(noteScope)})).then(r=>r.json())`,
  );
  const scopeMs = Date.now() - directScopeStart;
  if (scoped.state !== "ready" || scopeMs >= 500)
    throw new Error(
      `note scope request state=${scoped.state}, took ${scopeMs}ms`,
    );
  const uiScopeStart = Date.now();
  await evaluate(`window.BrainGraphUI.open(${JSON.stringify(noteScope)})`);
  const uiScopeMs = Date.now() - uiScopeStart;
  if (uiScopeMs >= 500)
    throw new Error(`note scope UI opened in ${uiScopeMs}ms`);
  await until(
    () =>
      evaluate(
        'document.querySelector("#graph-status").textContent.trim()===""',
      ),
    "note scope rendered",
    1800000,
  );
  if (!graphRequests.includes(noteScope))
    throw new Error(
      `note scope was not requested by UI: ${JSON.stringify(graphRequests)}`,
    );
  await until(
    () =>
      evaluate(
        'document.querySelector("#graph-view").dataset.layout === "settled"',
      ),
    "note layout before click",
    5000,
  );
  await until(
    () =>
      evaluate(
        `window.BrainGraphUI?.screenNode?.(${JSON.stringify(scoped.nodes.find((n) => n.kind === "note")?.id)})`,
      ),
    "note coordinates",
  );
  const note = scoped.nodes.find((n) => n.kind === "note")?.id;
  const point = await evaluate(
    `window.BrainGraphUI.screenNode(${JSON.stringify(note)})`,
  );
  await call("Input.dispatchMouseEvent", {
    type: "mousePressed",
    x: point.x,
    y: point.y,
    button: "left",
    clickCount: 1,
  });
  await call("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    x: point.x,
    y: point.y,
    button: "left",
    clickCount: 1,
  });
  await until(
    () => evaluate('!document.querySelector("#inspector").hidden'),
    "note reader",
  );

  await until(
    () => evaluate('document.querySelector("#graph-view").hidden'),
    "graph close",
  );
  await evaluate(
    'document.documentElement.style.colorScheme="light";document.documentElement.style.setProperty("--bg","#faf8f3");document.documentElement.style.setProperty("--panel","#f1eee8");document.documentElement.style.setProperty("--raised","#e9e5dc");document.documentElement.style.setProperty("--text","#25211b");document.documentElement.style.setProperty("--muted","#625b50");document.documentElement.style.setProperty("--line","#d3cabc");document.documentElement.style.setProperty("--accent","#73592a");document.documentElement.style.setProperty("--secondary","#655b4a");document.documentElement.style.setProperty("--tertiary","#725573");document.documentElement.style.setProperty("--logo-highlight","#4d506e");window.BrainGraphUI.open("all",false)',
  );
  await until(
    () =>
      evaluate(
        '!document.querySelector("#graph-view").hidden && document.querySelector("#graph-status").textContent.trim()===""',
      ),
    "light graph",
  );
  await until(
    () =>
      evaluate(
        'document.querySelector("#graph-view").dataset.layout === "settled"',
      ),
    "settled light graph",
    5000,
  );
  await evaluate("render()");
  if (
    !(await evaluate(
      'document.querySelector("#inspector").hidden && !document.querySelector("#graph-view").hidden',
    ))
  )
    throw new Error("background render covered the graph with the reader");
  await screenshot("light");
  await evaluate(
    'document.documentElement.style.colorScheme="dark";document.documentElement.style.setProperty("--bg","#151310");document.documentElement.style.setProperty("--panel","#211f1c");document.documentElement.style.setProperty("--raised","#2b2925");document.documentElement.style.setProperty("--text","#e7e1db");document.documentElement.style.setProperty("--muted","#cec5b7");document.documentElement.style.setProperty("--line","#4c463c");document.documentElement.style.setProperty("--accent","#dbc492");document.documentElement.style.setProperty("--secondary","#d2c5ad");document.documentElement.style.setProperty("--tertiary","#becca2");document.documentElement.style.setProperty("--logo-highlight","#eee9f4")',
  );
  await new Promise((resolve) => setTimeout(resolve, 200));
  await screenshot("dark");
  if (errors.length)
    throw new Error(
      `browser console errors: ${[...new Set(errors)].join("\n")}`,
    );
  if (settleMs >= 3000 && dragFps < 50)
    throw new Error(
      `graph layout took ${settleMs}ms and drag averaged ${dragFps.toFixed(1)}fps`,
    );
  const simulationStopped = await evaluate(
    "window.BrainGraphUI.isRunning() === false",
  );
  if (!simulationStopped)
    throw new Error("graph simulation remained active after layout settled");
  console.log(
    JSON.stringify({
      status: "passed",
      notes: 2000,
      wikiPages: worlds.length,
      button: "passed",
      building: "passed",
      categoryNarrow: "passed",
      noteScopeMs: scopeMs,
      noteScopeOpenMs: uiScopeMs,
      graphReadyMs,
      layoutSettledMs: settleMs,
      dragFps: Number(dragFps.toFixed(1)),
      simulationStopped,
      fittedBounds,
      fitButton: "passed",
      resizePadding,
      legend: "passed",
      noteClickReader: "passed",
      screenshots: [
        ".lead/evidence/n0136-graph-light.png",
        ".lead/evidence/n0136-graph-dark.png",
        ".lead/evidence/n0136-graph-narrowed.png",
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
