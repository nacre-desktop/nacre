// N0149 Ask the Brain browser acceptance. Run on the desktop test host with
// BRAIN_TEST_NATIVE and a private Chromium CDP profile configured.
import fs from "node:fs";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { once } from "node:events";
import { spawn } from "node:child_process";

const root = path.resolve("."),
  binary = path.resolve(
    process.env.BRAIN_TEST_NATIVE || "target/debug/nacre-helper",
  );
const webSource = path.resolve(process.env.BRAIN_TEST_WEB || "brain/web");
const profile = path.resolve(
  process.env.BRAIN_TEST_PROFILE ||
    path.join(os.homedir(), ".cache/nacre-brain-cdp"),
);
const requestedCdp = Number(process.env.BRAIN_TEST_CDP || 17946);
const temp = fs.mkdtempSync(path.join(os.tmpdir(), "nacre-brain-ask-"));
const home = path.join(temp, "home"),
  vault = path.join(temp, "vault"),
  installedWeb = path.join(home, ".local/share/nacre/brain/web");
const evidence = path.resolve(
  process.env.BRAIN_TEST_EVIDENCE || ".lead/evidence",
);
const portServer = net.createServer();
await new Promise((resolve, reject) =>
  portServer.listen(0, "127.0.0.1", (error) =>
    error ? reject(error) : resolve(),
  ),
);
const port = portServer.address().port;
await new Promise((resolve, reject) =>
  portServer.close((error) => (error ? reject(error) : resolve())),
);
for (const dir of [home, vault, path.dirname(installedWeb), evidence])
  fs.mkdirSync(dir, { recursive: true, mode: 0o700 });
fs.cpSync(webSource, installedWeb, { recursive: true });
const notePath = "projects/smash-board.md";
fs.mkdirSync(path.join(vault, "projects"), { recursive: true, mode: 0o700 });
fs.writeFileSync(
  path.join(vault, notePath),
  "# Smash-Board status\nRecorded: 2026-11-09T10:00:00+00:00\nConfidence: verified\nWorlds: Newbringer\nAreas: Smash-Board\n\nThe Smash-Board is running firmware 4.2. The magazine sensor is clean.\n",
  { mode: 0o600 },
);
const stubDir = path.join(temp, "bin");
fs.mkdirSync(stubDir);
const stub = path.join(stubDir, "claude");
const answer = {
  answer: "The Smash-Board is running firmware 4.2.",
  citations: [`[[${notePath}]]`],
  confidence: "high",
  followups: ["When was firmware 4.2 installed?"],
};
const claudeOutput = JSON.stringify({
  result: JSON.stringify(answer),
  is_error: false,
});
fs.writeFileSync(
  stub,
  `#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '${claudeOutput}'\n`,
);
fs.chmodSync(stub, 0o700);
const base = `http://127.0.0.1:${port}`;
const env = {
  ...process.env,
  HOME: home,
  PATH: `${stubDir}:${process.env.PATH}`,
  XDG_CONFIG_HOME: path.join(home, ".config"),
  XDG_STATE_HOME: path.join(home, ".local/state"),
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
let brain,
  socket,
  target,
  sequence = 0;
const pending = new Map(),
  errors = [];
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
  const end = Date.now() + timeout;
  while (Date.now() < end) {
    if (errors.length) throw new Error(errors.join("\n"));
    if (await evaluate(expression)) return;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error(`timed out waiting for ${label}`);
}
async function screenshot(name) {
  const image = await call("Page.captureScreenshot", { format: "png" });
  fs.writeFileSync(
    path.join(evidence, `n0149-ask-${name}.png`),
    Buffer.from(image.data, "base64"),
  );
}
async function cdpPort() {
  if (requestedCdp > 0) return requestedCdp;
  const active = path.join(profile, "DevToolsActivePort");
  for (let i = 0; i < 100; i++) {
    try {
      const value = Number(
        fs.readFileSync(active, "utf8").split(/\r?\n/, 1)[0],
      );
      if (value > 0) return value;
    } catch {}
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error("Chromium CDP endpoint unavailable");
}
try {
  brain = spawn(binary, ["brain-control", "serve"], { env, stdio: "ignore" });
  for (let i = 0; i < 200; i++) {
    try {
      if ((await (await fetch(`${base}/api/health`)).json()).ready) break;
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
  await until('document.querySelector("#loading")?.hidden', "initial view");
  for (const color of ["light", "dark"]) {
    await call("Emulation.setEmulatedMedia", {
      features: [{ name: "prefers-color-scheme", value: color }],
    });
    const colors =
      color === "light"
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
            "--on-accent": "#ffffff",
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
            "--on-accent": "#3b2e10",
          };
    await evaluate(
      `(()=>{const r=document.documentElement;r.style.colorScheme=${JSON.stringify(color)};for(const [k,v] of Object.entries(${JSON.stringify(colors)}))r.style.setProperty(k,v)})()`,
    );
    await evaluate(
      `fetch("/api/ask/settings",{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({enabled:false})}).then(r=>{if(!r.ok)throw new Error("settings reset failed")})`,
    );
    await evaluate('$("ask-open").click()');
    await until(
      '!$("ask-panel").hidden && !$("ask-disabled").hidden && $("ask-result").hidden',
      "disabled Ask state and cleared result",
    );
    if (
      !(await evaluate(
        '$("ask-disabled").textContent.includes("Ask the Brain sends excerpts of matching notes to Anthropic (Claude) using your subscription.")',
      ))
    )
      throw new Error("Ask disclosure missing");
    const disabled = await evaluate(
      `(async()=>{const r=await fetch("/api/ask",{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({question:"What is the Smash-Board status?"})});return {status:r.status,body:await r.json()}})()`,
    );
    if (
      disabled.status !== 403 ||
      disabled.body.code !== "disabled" ||
      !disabled.body.error.includes("disabled")
    )
      throw new Error("Disabled Ask did not return a clear 403");
    await screenshot(`${color}-disabled`);
    await evaluate('$("ask-enable").click()');
    await until('!$("ask-form").hidden', "enabled Ask form");
    await evaluate(
      '$("ask-question").value="What is the Smash-Board status?";$("ask-form").requestSubmit()',
    );
    await until(
      '!$("ask-result").hidden && $("ask-result").textContent.includes("firmware 4.2")',
      "answer and citation",
    );
    await screenshot(`${color}-answer`);
    const pendingFollowup = await evaluate(
      'document.querySelector(".ask-followup").click();$("ask-form").querySelector("button[type=submit]").disabled',
    );
    if (!pendingFollowup) throw new Error("follow-up did not start");
    await until(
      '!$("ask-form").querySelector("button[type=submit]").disabled',
      "follow-up response",
    );
    await evaluate(
      `(async()=>{const p=${JSON.stringify("projects/smash-board.md")};const name=p.split("/").pop();const digest=new Uint8Array(await crypto.subtle.digest("SHA-256",new TextEncoder().encode(name)));const expected="note:"+Array.from(digest.slice(0,8),b=>b.toString(16).padStart(2,"0")).join("");const original=graph.nodes.find(n=>n.path===p);if(original&&original.id!==expected)throw new Error("citation fixture ID mismatch");window.askExpectedId=original?.id||expected;graph.nodes=graph.nodes.filter(n=>n.path!==p);for(const [id,n] of byId)if(n.path===p)byId.delete(id);document.querySelector(".ask-citation").click()})()`,
    );
    await until(
      '!$("inspector").hidden && $("inspector").textContent.includes("magazine sensor is clean") && state.selected === window.askExpectedId',
      "fallback citation id opens its reader",
    );
  }
  if (errors.length)
    throw new Error(`browser console errors: ${errors.join("\n")}`);
  console.log(
    JSON.stringify({
      status: "passed",
      viewport: "1600x1000",
      screenshots: [
        ".lead/evidence/n0149-ask-light-disabled.png",
        ".lead/evidence/n0149-ask-light-answer.png",
        ".lead/evidence/n0149-ask-dark-disabled.png",
        ".lead/evidence/n0149-ask-dark-answer.png",
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
