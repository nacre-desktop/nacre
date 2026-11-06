// N0144 Wiki tab and connections acceptance. Run only on dev-1 inside the
// task container with a private Chromium CDP profile and the native helper.
// Synthetic vault: 7 worlds / 30 areas, 15 entity pages with current facts
// (provenance, superseded and stale lines), world/area pages, 2,000 linked
// notes and digests. No owner data.
import fs from "node:fs";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { once } from "node:events";
import { spawn } from "node:child_process";
import { hierarchyWorlds, writeRegistry } from "./hierarchy-fixture.mjs";

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
const temp = fs.mkdtempSync(path.join(os.tmpdir(), "nacre-brain-wiki-"));
const home = path.join(temp, "home");
const vault = path.join(temp, "vault");
const installedWeb = path.join(home, ".local/share/nacre/brain/web");
const evidence = path.join(root, ".lead/evidence");
for (const dir of [home, vault, path.dirname(installedWeb), evidence])
  fs.mkdirSync(dir, { recursive: true, mode: 0o700 });
fs.cpSync(webSource, installedWeb, { recursive: true });

const day = (offset) =>
  new Date(Date.now() - offset * 86400000).toISOString().slice(0, 10);
function write(rel, text) {
  const file = path.join(vault, rel);
  fs.mkdirSync(path.dirname(file), { recursive: true, mode: 0o700 });
  fs.writeFileSync(file, text, { mode: 0o600 });
}
const { areas } = writeRegistry(vault);
const areaNames = areas.map((area) => area.name);
const entities = [
  "Red Lantern",
  "Copper Kettle",
  "Glass Harbor",
  "Silver Fern",
  "Iron Bridge",
  "Amber Lamp",
  "Quiet Mill",
  "Stone Owl",
  "Blue Heron",
  "Paper Kite",
  "Tin Robot",
  "Velvet Moth",
  "Cobalt Gate",
  "Maple Drum",
  "North Beacon",
];
const slug = (value) => value.toLowerCase().replace(/[^a-z0-9]+/g, "-");
// Entity i lives in area areaOf(i); entity 0 (Red Lantern) in Alder Area 1.
const areaOf = (i) => areaNames[(i * 2) % areaNames.length];
const worldOf = (area) => areas.find((a) => a.name === area).parent;
const pad = (i) => String(i).padStart(4, "0");
entities.forEach((name, i) => {
  const area = areaOf(i);
  const sibling = entities[(i + 1) % entities.length];
  write(
    `wiki/entity-${slug(name)}.md`,
    `# ${name}\nEntity: device\nName: ${name}\nAliases: ${name.split(" ")[1]} unit ${i}\nParent: ${area}\nReviewed: ${day(1)}\nStatus: Curated by the synthetic fixture\n\nA synthetic device page. It pairs with the ${sibling}.\n\n## Current facts\n\n` +
      `- ${name} firmware is r${3100 + i}. (confirmed ${day(2)}; [[notes/${pad(i * 7)}]])\n` +
      `- ${name} answers on address 100.64.0.${10 + i}. (confirmed ${day(40)}; [[notes/${pad(i * 7 + 1)}]])\n` +
      `- ~~${name} sits on the ${hierarchyWorlds[(i + 1) % 7].split(" ")[1]} bench.~~ (confirmed ${day(70)}; [[notes/${pad(i * 7 + 2)}]]) (superseded ${day(5)} by [[notes/${pad(i * 7 + 3)}]])\n` +
      `- The owner prefers the **quiet** profile and \`slow-fan\` mode for ${name}. (decided ${day(90)})\n\n` +
      `## Notes\n\n| Port | Use |\n| --- | --- |\n| 1 | Power |\n| 2 | Sensor |\n\n\`\`\`\n${slug(name)} --status\n\`\`\`\n`,
  );
});
for (const [index, world] of hierarchyWorlds.entries())
  write(
    `wiki/world-${slug(world)}.md`,
    `# ${world}\nEntity: world\nName: ${world}\nWorlds: ${world}\nReviewed: ${day(3)}\nStatus: Curated\n\nThe ${world} world of the synthetic fixture.\n\n## Current facts\n\n- ${world} holds ${index < 2 ? 5 : 4} areas. (confirmed ${day(3)})\n- ${world} moved to the shared lock rule. (decided ${day(20)})\n`,
  );
for (const area of areaNames.slice(0, 10))
  write(
    `wiki/area-${slug(area)}.md`,
    `# ${area}\nEntity: area\nName: ${area}\nParent: ${worldOf(area)}\nWorlds: ${worldOf(area)}\nAreas: ${area}\nReviewed: ${day(2)}\n\nArea in ${worldOf(area)}.\n\n## Current facts\n\n- ${area} deploys build v1.${areaNames.indexOf(area)}.0 nightly. (confirmed ${day(45)})\n- ${area} keeps its notes in the synthetic vault. (confirmed ${day(4)})\n`,
  );
for (let i = 0; i < 2000; i += 1) {
  const entity = i % 3 === 0 ? entities[i % entities.length] : null;
  const area = entity
    ? areaOf(entities.indexOf(entity))
    : areaNames[i % areaNames.length];
  const links = [`[[notes/${pad((i + 1999) % 2000)}]]`];
  if (i % 7 === 0) links.push(`[[notes/${pad((i + 7) % 2000)}]]`);
  if (entity && i % 10 === 0)
    links.push(`[[wiki/entity-${slug(entity)}|${entity}]]`);
  write(
    `notes/${pad(i)}.md`,
    `# ${worldOf(area)} note ${i}${entity ? ` about ${entity}` : ""}\nRecorded: ${day(i % 60)}T${String(i % 24).padStart(2, "0")}:00:00+00:00\nConfidence: verified\nWorlds: ${worldOf(area)}\nAreas: ${area}\n\nSynthetic wiki fixture note ${i}.${entity ? ` Checked the ${entity} wiring and calibration${i % 2 ? " harness" : ""}.` : ` Routine ${["kiln", "loom", "orchard", "ledger"][i % 4]} upkeep log.`} See ${links.join(" and ")}.\n`,
  );
}
for (const area of areaNames.slice(0, 6))
  write(
    `digests/2026-10/${slug(worldOf(area))}-${slug(area)}.md`,
    `# Digest 2026-10: ${worldOf(area)} › ${area}\nRecorded: 2026-10-31\nConfidence: verified\nWorlds: ${worldOf(area)}\nAreas: ${area}\nKind: digest\n\n- 2026-10-01 [[notes/0001|note]]: synthetic digest line.\n`,
  );

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

let brain, socket, target;
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
async function until(expression, label, timeout = 30000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (errors.length) throw new Error(`browser errors: ${errors.join("\n")}`);
    if (await evaluate(expression)) return;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error(`timed out waiting for ${label}`);
}
const tokens = {
  light: {
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
  },
  dark: {
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
  },
};
async function screenshots(name) {
  for (const mode of ["light", "dark"]) {
    await evaluate(
      `(()=>{const r=document.documentElement;r.style.colorScheme=${JSON.stringify(mode)};for(const [k,v] of Object.entries(${JSON.stringify(tokens[mode])}))r.style.setProperty(k,v)})()`,
    );
    await new Promise((resolve) => setTimeout(resolve, 150));
    const result = await call("Page.captureScreenshot", { format: "png" });
    fs.writeFileSync(
      path.join(evidence, `n0144-${name}-${mode}.png`),
      Buffer.from(result.data, "base64"),
    );
  }
}
const click = (selector, text) =>
  evaluate(
    `(()=>{const n=Array.from(document.querySelectorAll(${JSON.stringify(selector)})).find(e=>${text ? `e.textContent.includes(${JSON.stringify(text)})` : "true"});if(!n)throw new Error("missing "+${JSON.stringify(selector + " " + (text || ""))});n.click();return true})()`,
  );
const content = () =>
  evaluate('document.querySelector("#content").textContent');

try {
  brain = spawn(binary, ["brain-control", "serve"], { env, stdio: "ignore" });
  for (let i = 0; i < 200; i += 1) {
    try {
      const response = await fetch(`${base}/api/health`);
      if (response.ok && (await response.json()).ready) break;
    } catch {}
    await new Promise((resolve) => setTimeout(resolve, 100));
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
    `http://127.0.0.1:${await cdpPort()}/json/new?about:blank`,
    { method: "PUT" },
  );
  if (!created.ok)
    throw new Error(`CDP target creation failed: ${created.status}`);
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
          .map((a) => a.description || String(a.value))
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
  const started = Date.now();
  await call("Page.navigate", { url: `${base}/#nacre-bootstrap=${token}` });
  await until('document.querySelector("#loading")?.hidden', "first paint");
  const landing = await evaluate(
    `({view:state.view,tab:document.querySelector('[data-view=wiki]').getAttribute('aria-selected'),title:$("title").textContent,cards:document.querySelectorAll("#content .wiki-card").length,rail:$("wiki-link").classList.contains("active")})`,
  );
  if (
    landing.view !== "wiki" ||
    landing.tab !== "true" ||
    landing.title !== "Wiki" ||
    landing.cards !== 7 ||
    !landing.rail
  )
    throw new Error(`Wiki is not the landing view: ${JSON.stringify(landing)}`);
  const firstPaintMs = Date.now() - started;
  await until(
    'document.querySelector("#connections-status")?.dataset.state === "ready"',
    "connections ready",
    120000,
  );
  const connectionsMs = Date.now() - started;
  await until(
    'document.querySelectorAll("#content .wiki-card p").length >= 7',
    "world summaries from wiki pages",
  );
  await screenshots("wiki-home");

  await click("#content .wiki-card", "World Alder");
  await until(
    '$("title").textContent === "World Alder" && document.querySelectorAll("#content .fact").length === 2',
    "world page facts",
  );
  if (!(await content()).includes("Alder Area 1"))
    throw new Error("world page lacks its area cards");
  await click("#content .wiki-card", "Alder Area 1");
  await until(
    '$("title").textContent === "Alder Area 1" && document.querySelectorAll("#content .fact").length === 2 && $("breadcrumbs").textContent.includes("World Alder")',
    "area page",
  );
  const areaStale = await evaluate(
    'document.querySelectorAll("#content .fact .stale-badge").length',
  );
  if (areaStale !== 1)
    throw new Error(`area page version fact should be stale: ${areaStale}`);
  await click("#content .wiki-card", "Red Lantern");
  await until(
    '$("title").textContent === "Red Lantern" && document.querySelectorAll("#content .fact").length === 4 && !!document.querySelector(".wiki-evidence .note-row")',
    "entity page with evidence",
  );
  const entity =
    await evaluate(`(()=>{const facts=Array.from(document.querySelectorAll("#content .fact"));const ev=document.querySelector(".wiki-evidence");const heads=Array.from(ev.querySelectorAll(".evidence-heading")).map(h=>h.textContent);return {
    crumbs:$("breadcrumbs").textContent,
    superseded:facts.filter(f=>f.classList.contains("superseded")&&f.querySelector("s")&&f.querySelector(".superseded-by a")).length,
    stale:facts.filter(f=>f.querySelector(".stale-badge")).map(f=>f.querySelector(".fact-text").textContent),
    provenance:facts.filter(f=>f.querySelector(".provenance time")&&f.querySelector(".provenance a")).length,
    bold:!!document.querySelector("#content .fact strong"),code:!!document.querySelector("#content .fact code"),
    table:!!document.querySelector("#content .wiki-prose table"),pre:!!document.querySelector("#content .wiki-prose pre"),
    heads,latest:ev.querySelectorAll(".note-row").length,reasons:ev.querySelectorAll(".related-reason").length,
    width:document.querySelector(".wiki-page").getBoundingClientRect().width,
    font:parseFloat(getComputedStyle(document.querySelector(".wiki-page")).fontSize)}})()`);
  if (
    !entity.crumbs.includes("World Alder") ||
    !entity.crumbs.includes("Alder Area 1") ||
    entity.superseded !== 1 ||
    entity.stale.length !== 1 ||
    !entity.stale[0].includes("address") ||
    entity.provenance < 2 ||
    !entity.bold ||
    !entity.code ||
    !entity.table ||
    !entity.pre ||
    !["Latest notes", "Linked from", "Related"].every((h) =>
      entity.heads.includes(h),
    ) ||
    entity.reasons < 1 ||
    entity.width > 75 * 0.7 * entity.font
  )
    throw new Error(`entity page: ${JSON.stringify(entity)}`);
  await screenshots("wiki-entity");
  await evaluate('document.querySelector(".wiki-evidence").scrollIntoView()');
  await screenshots("wiki-evidence");

  // A provenance link opens the detailed note reader.
  await click("#content .fact .provenance a");
  await until(
    '!document.querySelector("#inspector").hidden && document.querySelector("#inspector .reader").textContent.includes("Synthetic wiki fixture note 0.") && !!document.querySelector(".reader-connections .inspector-label")',
    "reader from provenance link",
  );
  const reader = await evaluate(
    `({labels:Array.from(document.querySelectorAll(".reader-connections .inspector-label")).map(n=>n.textContent),reasons:document.querySelectorAll(".reader-connections .related-reason").length,mentions:Array.from(document.querySelectorAll(".reader-connections .wiki-chip")).map(n=>n.textContent)})`,
  );
  if (
    !reader.labels.includes("Linked from") ||
    !reader.labels.includes("Related") ||
    !reader.mentions.includes("Red Lantern") ||
    reader.reasons < 1
  )
    throw new Error(`reader connections: ${JSON.stringify(reader)}`);
  await evaluate(
    'document.querySelector(".reader-connections").scrollIntoView()',
  );
  await screenshots("reader-connections");
  await click(".reader-connections .wiki-chip", "Red Lantern");
  await until(
    '$("title").textContent === "Red Lantern" && state.view === "wiki"',
    "mention chip opens the wiki page",
  );

  // Overview, notes and graph stay one click away.
  await click("[data-view=overview]");
  await until(
    'document.querySelector("#content").textContent.includes("Recent knowledge")',
    "overview tab",
  );
  await click("[data-view=notes]");
  await until('!!document.querySelector("#content .note-row")', "notes tab");
  const graph = await evaluate(
    `fetch("/api/graph?scope="+encodeURIComponent("area:Alder Area 1")).then(r=>r.json()).then(g=>({state:g.state,kinds:[...new Set(g.links.map(l=>l.kind))],related:g.links.filter(l=>l.kind==="related").length,nodes:g.nodes.length}))`,
  );
  const all = await evaluate(
    `fetch("/api/graph?scope=all").then(r=>r.json()).then(g=>({kinds:[...new Set(g.links.map(l=>l.kind))],nodes:g.nodes.length,links:g.links.length}))`,
  );
  if (
    !graph.kinds.includes("related") ||
    !graph.kinds.includes("membership") ||
    graph.related > graph.nodes * 5 ||
    !all.kinds.includes("mention")
  )
    throw new Error(`graph edges: ${JSON.stringify({ graph, all })}`);
  await click("#graph-toggle");
  await until(
    '!document.querySelector("#graph-view").hidden',
    "graph one click away",
  );
  await click("#graph-toggle");
  await click("#wiki-link");
  await until(
    'state.view === "wiki" && $("title").textContent === "Wiki"',
    "rail returns to the wiki",
  );
  if (errors.length) throw new Error(errors.join("\n"));
  console.log(
    JSON.stringify({
      status: "passed",
      notes: 2000,
      wikiPages: entities.length + hierarchyWorlds.length + 10,
      firstPaintMs,
      connectionsMs,
      entity,
      reader,
      graph,
      all,
      screenshots: [
        "wiki-home",
        "wiki-entity",
        "wiki-evidence",
        "reader-connections",
      ].flatMap((name) =>
        ["light", "dark"].map(
          (mode) => `.lead/evidence/n0144-${name}-${mode}.png`,
        ),
      ),
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
