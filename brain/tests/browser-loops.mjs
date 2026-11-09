// N0151 Open loops and timeline viewer acceptance. Run on the desktop test
// host under its capture lock, with private Chromium/CDP and a synthetic vault.
import fs from "node:fs";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { once } from "node:events";
import { spawn } from "node:child_process";
import { writeRegistry } from "./hierarchy-fixture.mjs";

const root = path.resolve("."),
  binary = path.resolve(
    process.env.BRAIN_TEST_NATIVE || "target/debug/nacre-helper",
  );
const temp = fs.mkdtempSync(path.join(os.tmpdir(), "nacre-brain-loops-"));
const home = path.join(temp, "home"),
  vault = path.join(temp, "vault");
const web = path.join(home, ".local/share/nacre/brain/web"),
  evidence = path.join(root, ".lead/evidence");
for (const dir of [home, vault, path.dirname(web), evidence])
  fs.mkdirSync(dir, { recursive: true, mode: 0o700 });
fs.cpSync(path.join(root, "brain/web"), web, { recursive: true });
const { areas } = writeRegistry(vault);
const area = areas.find((item) => item.name === "Alder Area 1");
const world = area.parent;
const write = (rel, data) => {
  const file = path.join(vault, rel);
  fs.mkdirSync(path.dirname(file), { recursive: true, mode: 0o700 });
  fs.writeFileSync(file, data, { mode: 0o600 });
};
write(
  "wiki/area-alder-area-1.md",
  `# Alder Area 1\nEntity: area\nName: Alder Area 1\nParent: ${world}\nWorlds: ${world}\nAreas: Alder Area 1\n\nArea fixture.\n\n## Current facts\n\n- Stable synthetic area. (confirmed 2026-11-08)\n`,
);
write(
  "notes/open.md",
  `# Open loop fixture\nRecorded: 2026-11-08\nWorlds: ${world}\nAreas: Alder Area 1\n\n- [ ] Finish loop source link fixture N0151\nNext: Verify source opens the reader\n`,
);
write(
  ".brain-state/index/review.json",
  JSON.stringify({
    items: [
      {
        id: "review-fixture",
        path: "notes/open.md",
        text: "Check review loop fixture",
        date: "2026-11-08",
        worlds: [world],
        areas: ["Alder Area 1"],
      },
    ],
  }),
);
const server = net.createServer();
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const port = server.address().port;
server.close();
const profile = path.resolve(
  process.env.BRAIN_TEST_PROFILE ||
    path.join(os.homedir(), ".cache/nacre-brain-cdp"),
);
async function browserCdpPort() {
  const requested = Number(process.env.BRAIN_TEST_CDP || 17946);
  if (requested > 0) return requested;
  const active = path.join(profile, "DevToolsActivePort");
  for (let i = 0; i < 100; i++) {
    try {
      const value = Number(
        fs.readFileSync(active, "utf8").split(/\r?\n/, 1)[0],
      );
      if (value > 0) return value;
    } catch {}
    await new Promise((r) => setTimeout(r, 100));
  }
  throw Error(`Chromium did not publish DevToolsActivePort under ${profile}`);
}
const cdpPort = await browserCdpPort(),
  base = `http://127.0.0.1:${port}`;
const env = {
  ...process.env,
  HOME: home,
  NACRE_BRAIN_DIR: vault,
  NACRE_BRAIN_PORT: String(port),
  NACRE_BRAIN_SEMANTICS: "0",
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
let brain,
  socket,
  target,
  seq = 0;
const pending = new Map(),
  errors = [];
function call(method, params = {}) {
  return new Promise((resolve, reject) => {
    const id = ++seq;
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
    throw Error(
      result.exceptionDetails.exception?.description ||
        result.exceptionDetails.text,
    );
  return result.result.value;
}
async function until(expression, label, timeout = 60000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) {
    if (errors.length) throw Error(errors.join("\n"));
    if (await evaluate(expression)) return;
    await new Promise((r) => setTimeout(r, 80));
  }
  throw Error(`Timed out: ${label}`);
}
async function screenshot(name, mode) {
  await evaluate(
    `(()=>{const r=document.documentElement;r.style.colorScheme=${JSON.stringify(mode)};for(const [k,v] of Object.entries(${JSON.stringify(mode === "light" ? { "--bg": "#faf8f3", "--panel": "#f1eee8", "--raised": "#e9e5dc", "--text": "#25211b", "--muted": "#625b50", "--line": "#d3cabc", "--accent": "#73592a", "--on-accent": "#ffffff" } : { "--bg": "#151310", "--panel": "#211f1c", "--raised": "#2b2925", "--text": "#e7e1db", "--muted": "#cec5b7", "--line": "#4c463c", "--accent": "#dbc492", "--on-accent": "#3b2e10" })}))r.style.setProperty(k,v)})()`,
  );
  await new Promise((r) => setTimeout(r, 120));
  const shot = await call("Page.captureScreenshot", { format: "png" });
  fs.writeFileSync(
    path.join(evidence, `n0151-${name}-${mode}.png`),
    Buffer.from(shot.data, "base64"),
  );
}
try {
  brain = spawn(binary, ["brain-control", "serve"], { env, stdio: "ignore" });
  for (let i = 0; i < 300; i++) {
    try {
      if ((await (await fetch(`${base}/api/health`)).json()).ready) break;
    } catch {}
    await new Promise((r) => setTimeout(r, 100));
  }
  const token = JSON.parse(
    fs.readFileSync(
      path.join(
        home,
        ".local/state/nacre/brain/auth/ports",
        `${port}.bootstrap.json`,
      ),
      "utf8",
    ),
  ).token;
  const created = await fetch(
    `http://127.0.0.1:${cdpPort}/json/new?about:blank`,
    { method: "PUT" },
  );
  if (!created.ok) throw Error(`CDP target creation failed: ${created.status}`);
  target = await created.json();
  socket = new WebSocket(target.webSocketDebuggerUrl);
  await once(socket, "open");
  socket.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    if (message.id) {
      const item = pending.get(message.id);
      pending.delete(message.id);
      message.error
        ? item?.reject(message.error)
        : item?.resolve(message.result);
    } else if (message.method === "Runtime.exceptionThrown")
      errors.push(message.params.exceptionDetails.text);
    else if (
      message.method === "Runtime.consoleAPICalled" &&
      message.params.type === "error"
    )
      errors.push(
        message.params.args
          .map((arg) => arg.description || String(arg.value))
          .join(" "),
      );
  });
  await call("Runtime.enable");
  await call("Page.enable");
  await call("Emulation.setDeviceMetricsOverride", {
    width: 1600,
    height: 1000,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await call("Page.navigate", { url: `${base}/#nacre-bootstrap=${token}` });
  await until(
    'typeof ready !== "undefined" && ready && document.querySelector("#loading").hidden',
    "viewer ready",
  );
  await until(
    'Array.from(document.querySelectorAll("#subjects .review-link")).some((link)=>link.textContent.includes("Open loops")&&link.textContent.includes("3"))',
    "sidebar count of three open loops",
  );
  await evaluate(
    'Array.from(document.querySelectorAll("#subjects .review-link")).find((link)=>link.textContent.includes("Open loops")).click()',
  );
  await until(
    'document.querySelectorAll("#content .open-loop").length===3',
    "global open loops page",
  );
  await evaluate('openWiki("world:world-alder")');
  await until(
    'document.querySelector(".wiki-loops") && document.querySelector(".wiki-info").textContent.includes("No wiki page yet")',
    "world fallback Open and Timeline sections",
  );
  await evaluate('openWiki("area:alder-area-1")');
  await until(
    'document.querySelector(".wiki-loops .open-loop") && document.querySelector(".wiki-loops").textContent.includes("Timeline")',
    "area loops and timeline controls",
  );
  const openState = await evaluate(
    '({count:document.querySelectorAll(".wiki-loops .open-loop").length, text:document.querySelector(".wiki-loops").textContent, source:document.querySelector(".wiki-loop-source")?.textContent, width:innerWidth, height:innerHeight})',
  );
  if (
    openState.count < 2 ||
    !openState.text.includes("Finish loop source link fixture")
  )
    throw Error(`Area open loops missing: ${JSON.stringify(openState)}`);
  await evaluate(
    'const f=document.querySelector(".wiki-loops .loop-kind-filter");f.value="todo";f.dispatchEvent(new Event("change"))',
  );
  const filtered = await evaluate(
    'document.querySelectorAll(".wiki-loops .open-loop").length',
  );
  if (filtered !== 2)
    throw Error(`Kind filter returned ${filtered} todo loops`);
  await evaluate(
    'document.querySelector(".wiki-loops .loop-kind-filter").value="all";document.querySelector(".wiki-loops .loop-kind-filter").dispatchEvent(new Event("change"))',
  );
  for (const mode of ["light", "dark"]) await screenshot("area-open", mode);
  await evaluate(
    'Array.from(document.querySelectorAll(".wiki-loop-tabs [role=tab]")).find((tab)=>tab.textContent==="Timeline").click()',
  );
  await until('document.querySelector(".timeline-event")', "timeline events");
  const timeline = await evaluate(
    '({events:document.querySelectorAll(".timeline-event").length,ids:Array.from(document.querySelectorAll(".timeline-event .unit-id")).map(n=>n.textContent)})',
  );
  if (!timeline.events || !timeline.ids.includes("N0151"))
    throw Error(`Timeline details missing: ${JSON.stringify(timeline)}`);
  for (const mode of ["light", "dark"]) await screenshot("area-timeline", mode);
  await evaluate(
    'Array.from(document.querySelectorAll(".wiki-loop-tabs [role=tab]")).find((tab)=>tab.textContent==="Open").click()',
  );
  await until(
    'document.querySelector(".wiki-loop-source")',
    "open loop source link",
  );
  await evaluate('document.querySelector(".wiki-loop-source").click()');
  await until(
    '!document.querySelector("#inspector").hidden && document.querySelector("#inspector .reader").textContent.includes("Finish loop source link fixture")',
    "source opens note reader",
  );
  await evaluate(
    'Array.from(document.querySelectorAll("#subjects .review-link")).find((link)=>link.textContent.includes("Open loops")).click()',
  );
  await until(
    'document.querySelectorAll("#content .open-loop").length===3',
    "global list before review index update",
  );
  const sourcePath = path.join(vault, "notes/open.md");
  const sourceBefore = fs.readFileSync(sourcePath);
  const reviewPath = path.join(vault, ".brain-state/index/review.json");
  const review = JSON.parse(fs.readFileSync(reviewPath, "utf8"));
  review.items.push({
    id: "review-auto-refresh",
    path: "notes/open.md",
    text: "New Review waiting item",
    date: "2026-11-08",
    worlds: [world],
    areas: ["Alder Area 1"],
  });
  fs.writeFileSync(reviewPath, JSON.stringify(review), { mode: 0o600 });
  await until(
    'document.querySelectorAll("#content .open-loop").length===4 && document.querySelector("#content").textContent.includes("New Review waiting item") && Array.from(document.querySelectorAll("#subjects .review-link")).some((link)=>link.textContent.includes("Open loops")&&link.textContent.includes("4"))',
    "Review-only index update refreshes sidebar and open list without interaction",
    60000,
  );
  const sourceUnchanged =
    Buffer.compare(sourceBefore, fs.readFileSync(sourcePath)) === 0;
  if (!sourceUnchanged)
    throw Error("Review-only index refresh changed the source note bytes");
  const refreshedCount = await evaluate(
    'document.querySelectorAll("#content .open-loop").length',
  );
  if (refreshedCount !== 4)
    throw Error(`Review-only refresh showed ${refreshedCount} loops`);
  if (errors.length) throw Error(errors.join("\n"));
  console.log(
    JSON.stringify({
      status: "passed",
      openState,
      timeline,
      sourceReader: "passed",
      reviewOnlyRefresh: {
        loops: refreshedCount,
        sourceNoteUnchanged: sourceUnchanged,
      },
      consoleErrors: 0,
      screenshots: ["area-open", "area-timeline"].flatMap((name) =>
        ["light", "dark"].map(
          (mode) => `.lead/evidence/n0151-${name}-${mode}.png`,
        ),
      ),
    }),
  );
} finally {
  socket?.close();
  if (target)
    await fetch(`http://127.0.0.1:${cdpPort}/json/close/${target.id}`).catch(
      () => {},
    );
  if (brain && !brain.killed) brain.kill("SIGTERM");
  fs.rmSync(temp, { recursive: true, force: true });
}
