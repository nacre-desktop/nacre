// N0142 ranked search acceptance. Run only on dev-1 inside the task container
// with a private Chromium CDP profile and the native helper built.
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
const profile = path.resolve(
  process.env.BRAIN_TEST_PROFILE ||
    path.join(os.homedir(), ".cache/nacre-brain-cdp"),
);
const requestedCdp = Number(process.env.BRAIN_TEST_CDP || 17946);
const temp = fs.mkdtempSync(path.join(os.tmpdir(), "nacre-brain-search-"));
const home = path.join(temp, "home");
const vault = path.join(temp, "vault");
const installedWeb = path.join(home, ".local/share/nacre/brain/web");
const evidence = path.join(root, ".lead/evidence");
for (const dir of [home, vault, path.dirname(installedWeb), evidence])
  fs.mkdirSync(dir, { recursive: true, mode: 0o700 });
fs.cpSync(webSource, installedWeb, { recursive: true });

function note(rel, title, header, body) {
  const file = path.join(vault, rel);
  fs.mkdirSync(path.dirname(file), { recursive: true, mode: 0o700 });
  fs.writeFileSync(
    file,
    `# ${title}\nRecorded: 2026-10-${String((rel.length % 27) + 1).padStart(2, "0")}T12:00:00+00:00\nConfidence: verified\n${header}\n\n${body}\n`,
    { mode: 0o600 },
  );
}
const smash = "Worlds: Newbringer\nAreas: Smash-Board";
for (let i = 0; i < 12; i += 1)
  note(
    `projects/smash-${i}.md`,
    `Smash-Board firmware ${100 + i}`,
    smash,
    `Flashed the Smash-Board firmware build ${100 + i}; the magazine sensor is clean.`,
  );
note(
  "archive/checkpoints/2026-09/smash-old.md",
  "Smash-Board archived checkpoint",
  smash,
  "Old smash board bring-up notes.",
);
for (let i = 0; i < 6; i += 1)
  note(
    `infrastructure/tailnet-${i}.md`,
    `Tailscale node ${i}`,
    "Worlds: Infrastructure\nAreas: Network",
    "The tailscale node joined the tailnet.",
  );
for (let i = 0; i < 280; i += 1)
  note(
    `checkpoints/2026${String(i).padStart(4, "0")}-note.md`,
    `Checkpoint ${i} card reader`,
    "Worlds: Nacre\nAreas: Desktop",
    "The SD card was full again; the card reader on the desk failed.",
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
async function screenshot(name) {
  const result = await call("Page.captureScreenshot", { format: "png" });
  fs.writeFileSync(
    path.join(evidence, `n0142-${name}.png`),
    Buffer.from(result.data, "base64"),
  );
}
async function type(query) {
  const started = Date.now();
  await evaluate(
    `$("search").value=${JSON.stringify(query)};$("search").dispatchEvent(new Event("input"))`,
  );
  await until(
    `searchResults !== null && state.query === ${JSON.stringify(query)}`,
    `results for ${query}`,
  );
  return Date.now() - started;
}
const rows = () =>
  evaluate(
    `Array.from(document.querySelectorAll("#content .note-row")).map(r=>({title:r.querySelector(".note-title").textContent,marks:Array.from(r.querySelectorAll(".note-snippet mark")).map(m=>m.textContent),chips:Array.from(r.querySelectorAll(".note-path-chips")).map(c=>c.textContent),archived:byId.get(searchResults.find(x=>x.title===r.querySelector(".note-title").textContent)?.id)?.kind==="archive"}))`,
  );

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
    width: 1400,
    height: 900,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await call("Page.navigate", { url: `${base}/#nacre-bootstrap=${token}` });
  await until('document.querySelector("#loading")?.hidden', "first paint");

  const smashMs = await type("smash card");
  const smashRows = await rows();
  if (
    !smashRows[0]?.title.startsWith("Smash-Board firmware") ||
    !smashRows[0].marks.length ||
    !smashRows[0].chips.includes("Newbringer › Smash-Board") ||
    smashRows.some((r) => r.archived) ||
    smashRows.slice(0, 12).some((r) => !r.title.startsWith("Smash-Board"))
  )
    throw new Error(
      `smash card ranking: ${JSON.stringify(smashRows.slice(0, 3))}`,
    );
  await screenshot("smash-card");

  await evaluate(
    `Array.from(document.querySelectorAll("#content .filters button")).at(-1).click()`,
  );
  await until(
    "state.showArchive && searchResults?.some(r=>r.archived)",
    "archive toggle results",
  );
  const archived = await rows();
  const archivedIndex = archived.findIndex((r) => r.archived);
  if (archivedIndex < 12)
    throw new Error(`archived note not ranked lower: ${archivedIndex}`);
  await evaluate(
    `Array.from(document.querySelectorAll("#content .filters button")).at(-1).click()`,
  );
  await until(
    "!state.showArchive && !searchResults?.some(r=>r.archived)",
    "archive hidden",
  );

  const typoMs = await type("tailscail");
  await until(
    'document.querySelector(".did-you-mean button")?.textContent === "tailscale"',
    "did you mean",
  );
  const typoRows = await rows();
  if (!typoRows[0]?.title.startsWith("Tailscale node"))
    throw new Error(`typo ranking: ${JSON.stringify(typoRows.slice(0, 3))}`);
  await screenshot("did-you-mean");
  await evaluate('document.querySelector(".did-you-mean button").click()');
  await until(
    '$("search").value === "tailscale" && state.query === "tailscale" && searchResults?.length === 6 && !document.querySelector(".did-you-mean")',
    "suggestion applied",
  );
  await evaluate('document.querySelector("#content .note-row").click()');
  await until(
    '!document.querySelector("#inspector").hidden && document.querySelector("#inspector").textContent.includes("tailnet")',
    "reader opens a ranked result",
  );
  const noneMs = await type("qzxv plorktunk");
  if (
    (await rows()).length ||
    !(await evaluate('!!document.querySelector("#content .empty")'))
  )
    throw new Error("nonsense query returned results");
  if (errors.length) throw new Error(errors.join("\n"));
  console.log(
    JSON.stringify({
      status: "passed",
      notes: 299,
      smashCardTop: smashRows[0].title,
      smashCardMarks: smashRows[0].marks,
      chips: smashRows[0].chips,
      archivedRank: archivedIndex + 1,
      didYouMean: "tailscale",
      inputToResultsMs: { smashCard: smashMs, typo: typoMs, none: noneMs },
      screenshots: [
        ".lead/evidence/n0142-smash-card.png",
        ".lead/evidence/n0142-did-you-mean.png",
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
