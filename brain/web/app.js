"use strict";
const tr = (...args) => window.NacreI18n.tr(...args);
const $ = (id) => document.getElementById(id),
  NS = "http://www.w3.org/2000/svg";
let graph = { nodes: [], links: [], noteCount: 0 },
  byId = new Map(),
  groupChildren = new Map(),
  parents = new Map(),
  sourceNeighbours = new Map();
let state = {
    view: "wiki",
    focus: null,
    selected: null,
    query: "",
    filter: "all",
    kindFilter: "all",
    showArchive: false,
    limit: 60,
    history: [],
  },
  ready = false,
  loading = false,
  loadFailed = false,
  noteRequest = 0,
  lastNavigation = "",
  mapZoom = 1,
  lastSignature = "",
  searchIds = null,
  searchResults = null,
  searchSuggestion = "",
  searchController = null;
let firstApiCall = true;
let askEnabled = false;
let retryAction = () => load(true);
function showLoadError(message, retry = () => load(true)) {
  retryAction = retry;
  $("load-error-message").textContent = message;
  $("load-error").hidden = false;
}
function askActivate(active) {
  $("ask-panel").hidden = !active;
  $("content-scroll").classList.toggle("ask-active", active);
  if (active) refreshAskSetting();
}
async function refreshAskSetting() {
  try {
    const response = await fetch("/api/ask");
    if (!response.ok) throw new Error("Ask setting unavailable");
    const data = await response.json();
    askEnabled = !!data.enabled;
    $("ask-disabled").hidden = askEnabled;
    $("ask-form").hidden = !askEnabled;
    if (!askEnabled) {
      $("ask-result").hidden = true;
      $("ask-result").replaceChildren();
      $("ask-status").hidden = true;
    }
  } catch {
    $("ask-status").textContent = tr(
      "brain.ui.error.ask-unavailable",
      "Ask is unavailable right now.",
    );
    $("ask-status").hidden = false;
  }
}
async function submitAsk(event) {
  event.preventDefault();
  const question = $("ask-question").value.trim();
  if (!question || !askEnabled) return;
  const status = $("ask-status"),
    result = $("ask-result");
  result.hidden = true;
  status.hidden = false;
  status.textContent = tr("brain.ui.text.ask-loading", "Searching your notes…");
  try {
    const submit = $("ask-form").querySelector("button[type=submit]");
    submit.disabled = true;
    let response;
    try {
      response = await fetch("/api/ask", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ question }),
      });
    } finally {
      submit.disabled = false;
    }
    const data = await response.json();
    if (!response.ok) {
      const messages = {
        disabled: tr("brain.ui.error.ask-disabled", "Ask is disabled."),
        busy: tr(
          "brain.ui.error.ask-busy",
          "Another question is being answered. Try again shortly.",
        ),
        auth: tr(
          "brain.ui.error.ask-auth",
          "Claude is not signed in. Sign in with the Claude CLI and try again.",
        ),
        usage: tr(
          "brain.ui.error.ask-usage",
          "Claude could not process this request. Check your subscription usage.",
        ),
        timeout: tr(
          "brain.ui.error.ask-timeout",
          "The answer took too long. Try again.",
        ),
        missing_cli: tr(
          "brain.ui.error.ask-cli",
          "Claude CLI is unavailable. Install Claude Code and sign in.",
        ),
        question: tr(
          "brain.ui.error.ask-question",
          "Enter a question of 1–1000 characters.",
        ),
        unsafe_input: tr(
          "brain.ui.error.ask-unsafe-input",
          "The question or matching notes may contain a secret. Nothing was sent.",
        ),
        invalid_citation: tr(
          "brain.ui.error.ask-invalid-citation",
          "Claude cited a source outside the selected notes. The answer was refused.",
        ),
        unsafe_reply: tr(
          "brain.ui.error.ask-unsafe-reply",
          "The answer failed the safety checks and was refused.",
        ),
        unavailable: tr(
          "brain.ui.error.ask-unavailable",
          "Ask is unavailable right now.",
        ),
      };
      status.textContent =
        messages[data.code] ||
        tr("brain.ui.error.ask-failed", "Could not answer that question.");
      return;
    }
    status.hidden = true;
    result.replaceChildren();
    result.hidden = false;
    result.append(
      el(
        "p",
        data.confidence === "not_found"
          ? tr("brain.ui.text.ask-not-found", "That is not in your notes.")
          : data.answer || "",
        "ask-answer",
      ),
    );
    if (Array.isArray(data.citations) && data.citations.length) {
      const citations = el("div", "", "ask-citations");
      citations.append(
        el("span", tr("brain.ui.label.sources", "Sources"), "ask-label"),
      );
      for (const citation of data.citations) {
        const path = String(citation).replace(/^\[\[|\]\]$/g, "");
        citations.append(
          button(path, () => openAskCitation(path), "ask-citation"),
        );
      }
      result.append(citations);
    }
    if (Array.isArray(data.followups) && data.followups.length) {
      const followups = el("div", "", "ask-followups");
      followups.append(
        el(
          "span",
          tr("brain.ui.label.follow-up-questions", "Follow-up questions"),
          "ask-label",
        ),
      );
      for (const followup of data.followups)
        followups.append(
          button(
            followup,
            () => {
              $("ask-question").value = followup;
              $("ask-form").requestSubmit();
            },
            "ask-followup",
          ),
        );
      result.append(followups);
    }
  } catch {
    status.textContent = tr(
      "brain.ui.error.ask-failed",
      "Could not answer that question.",
    );
  }
}
async function openAskCitation(path) {
  try {
    const known = graph.nodes.find((item) => item.path === path);
    let note = known;
    if (!note) {
      const filename = path.split("/").pop();
      const digest = new Uint8Array(
        await crypto.subtle.digest(
          "SHA-256",
          new TextEncoder().encode(filename),
        ),
      );
      const hash = Array.from(digest.slice(0, 8), (byte) =>
        byte.toString(16).padStart(2, "0"),
      ).join("");
      note = { id: `note:${hash}`, kind: "note", path, label: path };
    }
    byId.set(note.id, note);
    if (!graph.nodes.some((item) => item.id === note.id))
      graph.nodes.push(note);
    askActivate(false);
    select(note.id, { view: "read" });
  } catch {
    $("ask-status").textContent = tr(
      "brain.ui.error.ask-citation",
      "Could not open that source.",
    );
    $("ask-status").hidden = false;
  }
}
function clearLoadError() {
  $("load-error").hidden = true;
}
async function boundedRequest(url, init = {}, decode = "response") {
  const controller = new AbortController();
  const externalSignal = init.signal;
  const abort = () => controller.abort();
  if (externalSignal?.aborted) controller.abort();
  else externalSignal?.addEventListener("abort", abort, { once: true });
  let timer;
  try {
    const operation = (async () => {
      let response = await fetch(url, { ...init, signal: controller.signal });
      if (response.status === 403 && firstApiCall) {
        await (window.__nacreBrainAuthReady || Promise.resolve());
        response = await fetch(url, { ...init, signal: controller.signal });
      }
      firstApiCall = false;
      if (!response.ok)
        throw Error(response.statusText || "HTTP " + response.status);
      return decode === "json" ? response.json() : response;
    })();
    const timeout = new Promise((_, reject) => {
      timer = setTimeout(() => {
        controller.abort();
        reject(
          Error(
            tr(
              "brain.ui.error.request-timed-out",
              "The request timed out. Try again.",
            ),
          ),
        );
      }, 8000);
    });
    return await Promise.race([operation, timeout]);
  } catch (error) {
    if (externalSignal?.aborted) throw externalSignal.reason;
    if (error.name === "AbortError")
      throw Error(
        tr(
          "brain.ui.error.request-timed-out",
          "The request timed out. Try again.",
        ),
      );
    throw error;
  } finally {
    clearTimeout(timer);
    externalSignal?.removeEventListener("abort", abort);
  }
}
function el(tag, value = "", className = "") {
  const n = document.createElement(tag);
  if (value) n.textContent = value;
  if (className) n.className = className;
  return n;
}
function icon(name) {
  const s = document.createElementNS(NS, "svg"),
    use = document.createElementNS(NS, "use");
  use.setAttribute("href", "#" + name);
  s.append(use);
  s.setAttribute("aria-hidden", "true");
  return s;
}
function button(label, fn, className = "") {
  const b = el("button", label, className);
  b.onclick = fn;
  return b;
}
function isNote(n) {
  return ["note", "archive", "digest"].includes(n?.kind);
}
function ordered(items) {
  return [...items].sort(
    (a, b) =>
      (b.recorded || b.date || "").localeCompare(a.recorded || a.date || "") ||
      a.label.localeCompare(b.label),
  );
}
function subjects() {
  return (graph.worlds || []).map(
    (w) => byId.get(w.id) || { ...w, kind: "world", count: w.count },
  );
}
function registryWorlds() {
  return graph.worlds || subjects();
}
function areasFor(world) {
  const record = registryWorlds().find(
    (w) => w.id === world.id || w.label === world.label,
  );
  return record?.areas || children(world.id).filter((n) => n.kind === "area");
}
function notePaths(note) {
  const paths = [];
  for (const areaName of note.areas || []) {
    const world = registryWorlds().find((candidate) =>
      areasFor(candidate).some((area) => area.label === areaName),
    );
    if (world)
      paths.push({
        world: world.label,
        area: areaName,
        worldId: world.id,
        areaId: areasFor(world).find((area) => area.label === areaName)?.id,
      });
  }
  for (const worldName of note.worlds || []) {
    if (!paths.some((path) => path.world === worldName)) {
      const world = registryWorlds().find(
        (candidate) => candidate.label === worldName,
      );
      paths.push({
        world: worldName,
        area: "",
        worldId: world?.id,
        areaId: null,
      });
    }
  }
  return paths;
}
function children(id) {
  return (groupChildren.get(id) || [])
    .map((id) => byId.get(id))
    .filter(Boolean);
}
function ancestors(id, seen = new Set()) {
  if (seen.has(id)) return [];
  seen.add(id);
  return (parents.get(id) || [])
    .flatMap((p) => [byId.get(p), ...ancestors(p, seen)])
    .filter(Boolean);
}
function notesIn(id) {
  if (id === "review")
    return graph.nodes.filter(
      (n) => n.review && ["note", "archive", "digest"].includes(n.kind),
    );
  if (!id)
    return graph.nodes.filter((n) =>
      ["note", "archive", "digest"].includes(n.kind),
    );
  const result = new Map(),
    seen = new Set();
  function walk(key) {
    if (seen.has(key)) return;
    seen.add(key);
    const n = byId.get(key);
    if (n && ["note", "archive", "digest"].includes(n.kind)) result.set(key, n);
    else for (const c of children(key)) walk(c.id);
  }
  walk(id);
  return [...result.values()];
}
function context() {
  const focus = byId.get(state.focus);
  return isNote(focus)
    ? ancestors(focus.id).find((n) => n.kind === "area") ||
        ancestors(focus.id).find((n) => n.kind === "world")
    : focus;
}
function theme() {
  for (const [name, role] of Object.entries({
    bg: "surface",
    panel: "surfaceContainer",
    raised: "surfaceContainerHigh",
    text: "onSurface",
    muted: "onSurfaceVariant",
    line: "outlineVariant",
    accent: "primary",
    secondary: "secondary",
    tertiary: "tertiary",
    "on-accent": "onPrimary",
  })) {
    const v = graph.theme?.[role];
    if (v && /^#?[a-f0-9]{6}$/i.test(v))
      document.documentElement.style.setProperty(
        "--" + name,
        "#" + v.replace(/^#/, ""),
      );
  }
  const surface = graph.theme?.surface || "151310";
  const light =
    parseInt(surface.replace("#", "").slice(0, 2), 16) +
      parseInt(surface.replace("#", "").slice(2, 4), 16) +
      parseInt(surface.replace("#", "").slice(4, 6), 16) >
    420;
  document.documentElement.style.setProperty(
    "--logo-highlight",
    "#" +
      (graph.theme?.[light ? "primary" : "primaryFixed"] || "eee9f4").replace(
        "#",
        "",
      ),
  );
  const value = graph.theme?.surface;
  if (value) {
    const rgb = value.replace("#", "");
    document.documentElement.style.colorScheme =
      parseInt(rgb.slice(0, 2), 16) +
        parseInt(rgb.slice(2, 4), 16) +
        parseInt(rgb.slice(4), 16) >
      420
        ? "light"
        : "dark";
  }
}
async function load(force = false) {
  if (loading) return;
  if (loadFailed && !force) return;
  loading = true;
  $("retry").disabled = true;
  $("load-error").hidden = true;
  if (!ready) $("loading").hidden = false;
  try {
    let summaryOnly = !ready;
    let payload = await requestJson(
      (summaryOnly ? "/api/summary" : "/api/brain") + (force ? "?retry=1" : ""),
    );
    loadFailed = false;
    applyGraph(payload, force);
    while (payload.connections?.state === "building") {
      setConnectionState("building");
      // Recheck promptly so a reused index appears without a long startup pause.
      await new Promise((resolve) => setTimeout(resolve, 250));
      payload = await requestJson("/api/brain");
      summaryOnly = false;
      applyGraph(payload);
    }
    if (payload.connections?.state === "ready" && summaryOnly) {
      payload = await requestJson("/api/brain");
      applyGraph(payload);
    }
    if (payload.connections?.state === "error")
      throw Error(
        payload.connections.error ||
          tr("brain.ui.error.connections", "Connections could not be loaded."),
      );
    setConnectionState(payload.connections?.state || "ready");
  } catch (error) {
    loadFailed = true;
    const message =
      error.message ||
      tr(
        "brain.ui.error.unavailable",
        "Could not load your knowledge. Try again.",
      );
    $("index-status").textContent = message;
    $("loading").hidden = true;
    showLoadError(message);
    setConnectionState("error");
  } finally {
    loading = false;
    $("retry").disabled = false;
  }
}
async function requestJson(url) {
  try {
    return await boundedRequest(url, {}, "json");
  } catch (error) {
    throw Error(
      error.message === "Failed to fetch"
        ? tr(
            "brain.ui.error.unavailable",
            "Could not load your knowledge. Try again.",
          )
        : error.message ||
            tr(
              "brain.ui.error.unavailable",
              "Could not load your knowledge. Try again.",
            ),
    );
  }
}
function setConnectionState(value) {
  const element = $("connections-status");
  element.dataset.state = value;
  element.textContent =
    value === "building"
      ? tr("brain.ui.status.building-connections", "Building connections…")
      : value === "error"
        ? tr("brain.ui.error.connections", "Connections could not be loaded.")
        : tr("brain.ui.status.connections-ready", "Connections ready");
}
function applyGraph(payload, force = false) {
  const previous = byId.get(state.selected);
  graph = payload;
  const signature = JSON.stringify([graph.nodes, graph.links, graph.theme]),
    changed = force || signature !== lastSignature;
  lastSignature = signature;
  byId = new Map(graph.nodes.map((n) => [n.id, n]));
  groupChildren = new Map();
  parents = new Map();
  sourceNeighbours = new Map();
  for (const link of graph.links) {
    if (link.kind === "membership") {
      if (!groupChildren.has(link.source)) groupChildren.set(link.source, []);
      groupChildren.get(link.source).push(link.target);
      if (!parents.has(link.target)) parents.set(link.target, []);
      parents.get(link.target).push(link.source);
    } else {
      for (const [a, b] of [
        [link.source, link.target],
        [link.target, link.source],
      ]) {
        if (!sourceNeighbours.has(a)) sourceNeighbours.set(a, new Set());
        sourceNeighbours.get(a).add(b);
      }
    }
  }
  theme();
  ready = true;
  $("loading").hidden = true;
  if (
    state.focus &&
    state.focus !== "review" &&
    state.view !== "wiki" &&
    !byId.has(state.focus)
  )
    state.focus = null;
  if (state.selected && !byId.has(state.selected)) state.selected = null;
  $("index-status").textContent = tr(
    "brain.ui.status.index",
    "{count} notes · {time}",
    {
      count: graph.noteCount,
      time: new Date(graph.generated).toLocaleTimeString([], {
        hour: "2-digit",
        minute: "2-digit",
      }),
    },
  );
  if (changed)
    render(
      !force &&
        isNote(previous) &&
        previous.revision === byId.get(previous.id)?.revision &&
        previous.path === byId.get(previous.id)?.path &&
        previous.kind === byId.get(previous.id)?.kind,
    );
  if (state.query) search();
}
async function apiAction(name, value = "") {
  try {
    await boundedRequest("/api/action", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ name, value }),
    });
    clearLoadError();
  } catch (e) {
    $("index-status").textContent = e.message;
    showLoadError(e.message, () => apiAction(name, value));
  }
}
function select(
  id,
  { view = state.view, inspect = true, history = true } = {},
) {
  if (!byId.has(id)) return;
  if (history)
    state.history.push({
      focus: state.focus,
      selected: state.selected,
      view: state.view,
    });
  const n = byId.get(id);
  state.focus = id;
  state.selected = inspect ? id : null;
  state.view = isNote(n) ? "read" : view;
  state.limit = 60;
  mapZoom = 1;
  render();
  $("content-scroll").scrollTop = 0;
}
function home(view = "wiki") {
  state = {
    ...state,
    focus: null,
    selected: null,
    view,
    query: "",
    limit: 60,
    history: [],
  };
  $("search").value = "";
  searchIds = searchResults = null;
  render();
}
function back() {
  const prior = state.history.pop();
  if (prior) {
    Object.assign(state, prior);
    mapZoom = 1;
    render();
  }
}
function render(keepInspector = false) {
  renderRail();
  renderMain();
  if (!keepInspector) renderInspector();
}
function renderRail() {
  const list = $("subjects");
  list.replaceChildren();
  $("subject-count").textContent = registryWorlds().length;
  $("overview-link").classList.toggle(
    "active",
    !state.focus && state.view !== "wiki",
  );
  $("wiki-link").classList.toggle(
    "active",
    !state.focus && state.view === "wiki",
  );
  const related = state.focus
    ? new Set([state.focus, ...ancestors(state.focus).map((n) => n.id)])
    : new Set();
  for (const n of registryWorlds()) {
    const expanded = expandedWorlds.has(n.id);
    const row = el("div", "", "world-row");
    const b = button(
      "",
      () => {
        state.query = "";
        $("search").value = "";
        searchIds = searchResults = null;
        select(n.id, { view: state.view === "wiki" ? "wiki" : "overview" });
      },
      "subject" + (related.has(n.id) ? " active" : ""),
    );
    const disclosure = button(expanded ? "▾" : "▸", (e) => {}, "world-expand");
    disclosure.setAttribute("aria-expanded", String(expanded));
    disclosure.setAttribute(
      "aria-label",
      tr(
        expanded
          ? "brain.ui.aria.collapse-world"
          : "brain.ui.aria.expand-world",
        expanded ? "Collapse world" : "Expand world",
      ),
    );
    disclosure.onclick = (event) => {
      event.stopPropagation();
      expanded ? expandedWorlds.delete(n.id) : expandedWorlds.add(n.id);
      renderRail();
    };
    b.append(
      icon("folder"),
      el("span", n.label, "name"),
      el("span", String(n.count || 0), "count"),
    );
    row.append(disclosure, b);
    if (n.new)
      row.append(el("small", tr("brain.ui.label.new", "New"), "new-badge"));
    list.append(row);
    if (expanded)
      for (const area of areasFor(n)) {
        const ab = button(
          "",
          () =>
            select(area.id, {
              view: state.view === "wiki" ? "wiki" : "overview",
            }),
          "area-link" + (related.has(area.id) ? " active" : ""),
        );
        ab.append(
          el("span", area.label, "name"),
          el("span", String(area.count || 0), "count"),
        );
        if (area.proposed) {
          ab.querySelector(".name").textContent = tr(
            "brain.ui.label.new-area",
            "New: {area}",
            { area: area.label },
          );
          ab.classList.add("proposed");
        } else if (area.new)
          ab.append(el("small", tr("brain.ui.label.new", "New"), "new-badge"));
        list.append(ab);
      }
  }
  const review = button(
    "",
    () => {
      state.focus = "review";
      state.view = "notes";
      render();
    },
    "subject review-link" + (state.focus === "review" ? " active" : ""),
  );
  review.append(
    icon("note"),
    el("span", tr("brain.ui.label.review", "Review"), "name"),
    el("span", String(graph.reviewCount || 0), "count"),
  );
  list.append(review);
}
const expandedWorlds = new Set();
function toggleArchive() {
  state.showArchive = !state.showArchive;
  if (!state.showArchive && state.kindFilter === "archive")
    state.kindFilter = "all";
  renderMain();
  if (state.query) search();
}
function section(title, action) {
  const n = el("div", "", "section-heading");
  n.append(el("h2", title));
  if (action) n.append(button(action.label, action.fn));
  return n;
}
function renderMain() {
  if (!ready) return;
  if (window.BrainGraphUI?.isActive()) {
    $("graph-view").hidden = false;
    $("content").hidden = true;
    $("inspector").hidden = true;
    return;
  }
  const c = context(),
    focus = byId.get(state.focus),
    searching = !!state.query;
  const crumbs = $("breadcrumbs");
  crumbs.replaceChildren();
  if (focus) {
    crumbs.append(
      button(tr("brain.ui.literal.brain", "Brain"), () =>
        home(state.view === "wiki" ? "wiki" : "overview"),
      ),
      el("span", "/"),
    );
    const parent = ancestors(focus.id).find((n) => n.kind === "world");
    const notePath = isNote(focus) && notePaths(focus)[0];
    if (notePath?.area)
      crumbs.append(
        button(
          notePath.world,
          () =>
            notePath.worldId && select(notePath.worldId, { view: "overview" }),
        ),
        el("span", "›"),
        button(
          notePath.area,
          () =>
            notePath.areaId && select(notePath.areaId, { view: "overview" }),
        ),
        el("span", "›"),
      );
    else if (notePath)
      crumbs.append(
        button(
          notePath.world,
          () =>
            notePath.worldId && select(notePath.worldId, { view: "overview" }),
        ),
        el("span", "›"),
      );
    if (parent && parent.id !== focus.id && !notePath)
      crumbs.append(
        button(parent.label, () => select(parent.id)),
        el("span", "›"),
      );
    crumbs.append(
      el(
        "span",
        isNote(focus) ? tr("brain.ui.label.note", "Note") : focus.label,
      ),
    );
  }
  const reading = state.view === "read" && isNote(byId.get(state.selected));
  const graphContext = $("graph-context");
  graphContext.hidden = !focus || reading;
  graphContext.onclick = () => {
    if (!focus) return;
    const kind = isNote(focus) ? "note" : focus.kind;
    const value = kind === "note" ? focus.id : focus.label || focus.id;
    window.BrainGraphUI?.open(`${kind}:${value}`, false);
  };
  document
    .querySelector(".content-scroll")
    .classList.toggle("reading", reading);
  $("inspector").hidden = !reading;
  $("content").hidden = reading;
  document.querySelector(".viewbar").hidden = reading;
  $("title").textContent = reading
    ? byId.get(state.selected).label
    : searching
      ? tr("brain.ui.label.search-results", "Search results")
      : state.focus === "review"
        ? tr("brain.ui.label.review", "Review")
        : c?.label || tr("brain.ui.label.your-brain", "Your brain");
  $("subtitle").textContent = reading
    ? ""
    : searching
      ? tr(
          "brain.ui.label.matching-titles-topics-and-note-content",
          "Matching titles, topics and note content.",
        )
      : c
        ? tr("brain.ui.count.notes-areas", "{notes} notes · {areas} areas", {
            notes: notesIn(c.id).length,
            areas: c.kind === "world" ? areasFor(c).length : 0,
          })
        : tr(
            "brain.ui.label.the-subjects-you-return-to-connected-by-evidence",
            "The subjects you return to, connected by evidence.",
          );
  $("back").hidden = state.history.length === 0;
  for (const b of document.querySelectorAll("[data-view]")) {
    const active = b.dataset.view === state.view;
    b.classList.toggle("active", active);
    b.setAttribute("aria-selected", String(active));
  }
  const out = $("content");
  if (reading) return;
  out.replaceChildren();
  if (searching || state.view === "notes") {
    renderNotes(out);
    return;
  }
  if (state.view === "wiki") {
    renderWiki(out);
    return;
  }
  if (state.view === "map") {
    renderMap(out);
    return;
  }
  $("view-summary").textContent = c
    ? tr("brain.ui.label.focused-knowledge", "Focused knowledge")
    : tr(
        "brain.ui.count.worlds",
        { one: "{count} worlds", other: "{count} worlds" },
        { count: subjects().length },
      );
  if (!c) {
    const cards = el("div", "", "subjects-grid");
    for (const n of subjects()) {
      const b = button("", () => select(n.id), "subject-card"),
        top = el("div", "", "card-top");
      top.append(
        icon("folder"),
        el(
          "small",
          tr(
            "brain.ui.count.notes",
            { one: "{count} notes", other: "{count} notes" },
            { count: n.count },
          ),
        ),
      );
      if (n.new)
        top.append(el("small", tr("brain.ui.label.new", "New"), "new-badge"));
      const topics = areasFor(n);
      b.append(
        top,
        el("h2", n.label),
        el(
          "p",
          topics
            .slice(0, 3)
            .map((v) =>
              v.proposed
                ? tr("brain.ui.label.new-area", "New: {area}", {
                    area: v.label,
                  })
                : v.label,
            )
            .join(", ") ||
            tr("brain.ui.label.notes-and-ideas", "Notes and ideas"),
        ),
      );
      cards.append(b);
    }
    out.append(cards);
  } else {
    const topics = c.kind === "world" ? areasFor(c) : [];
    if (topics.length) {
      out.append(section(tr("brain.ui.label.areas", "Areas")));
      const list = el("div", "", "topics");
      for (const n of topics) {
        const b = button(
          n.proposed
            ? tr("brain.ui.label.new-area", "New: {area}", { area: n.label })
            : n.label,
          () => select(n.id),
          "topic" + (n.proposed ? " proposed" : ""),
        );
        b.append(el("span", String(n.count || 0)));
        if (n.new)
          b.append(el("small", tr("brain.ui.label.new", "New"), "new-badge"));
        list.append(b);
      }
      out.append(list);
    }
    if (c.kind === "area") {
      const wiki = graph.nodes.find(
        (node) =>
          node.path?.startsWith("wiki/") &&
          (node.area === c.label || node.areas?.includes(c.label)),
      );
      if (wiki)
        out.append(
          button(
            tr("brain.ui.label.open-wiki-page", "Open wiki page"),
            () => select(wiki.id),
            "wiki-link",
          ),
        );
    }
  }
  const split = el("div", "", "split-sections"),
    recent = el("div"),
    related = el("div");
  recent.append(
    section(tr("brain.ui.literal.recent-knowledge", "Recent knowledge"), {
      label: state.showArchive
        ? tr("brain.ui.label.hide-archive", "Hide archive")
        : tr("brain.ui.label.show-archive", "Show archive"),
      fn: toggleArchive,
    }),
  );
  recent.append(
    button(
      tr("brain.ui.label.view-notes", "View notes"),
      () => {
        state.view = "notes";
        renderMain();
      },
      "view-notes-action",
    ),
  );
  for (const n of ordered(
    notesIn(c?.id).filter(
      (note) => state.showArchive || note.kind !== "archive",
    ),
  ).slice(0, 8))
    recent.append(noteRow(n));
  if (!notesIn(c?.id).length)
    recent.append(
      el(
        "p",
        tr(
          "brain.ui.label.no-notes-here-yet-new-knowledge-will-appear-as-it-is-saved",
          "No notes here yet. New knowledge will appear as it is saved.",
        ),
        "empty",
      ),
    );
  related.append(
    section(tr("brain.ui.literal.across-subjects", "Across subjects")),
  );
  const pairs = sharedSubjects(c?.kind === "world" ? c.id : null).slice(0, 5);
  if (pairs.length) {
    for (const p of pairs) {
      const b = button(
        p.a.label + " / " + p.b.label,
        () => select(p.b.id, { view: "map" }),
        "connection-row",
      );
      b.append(
        el(
          "small",
          tr(
            "brain.ui.count.shared",
            { one: "{count} shared note", other: "{count} shared notes" },
            { count: p.count },
          ),
        ),
      );
      related.append(b);
    }
    related.append(
      el(
        "p",
        tr(
          "brain.ui.label.shared-notes-connect-subjects-they-do-not-establish-a-dependency",
          "Shared notes connect subjects; they do not establish a dependency.",
        ),
        "activity-caption",
      ),
    );
  } else
    related.append(
      el(
        "p",
        tr(
          "brain.ui.label.explicit-references-and-shared-notes-will-appear-here-as-your-knowledge-connects",
          "Explicit references and shared notes will appear here as your knowledge connects.",
        ),
        "empty",
      ),
    );
  const suggestions =
    graph.discovery?.suggestions || graph.discovery?.clusters || [];
  const details = el("details", "", "suggestions");
  details.append(
    el(
      "summary",
      tr("brain.ui.label.suggested-groupings", "Suggested groupings"),
    ),
  );
  if (Array.isArray(suggestions) && suggestions.length) {
    for (const item of suggestions.slice(0, 20))
      details.append(el("p", item.label || item.name || String(item)));
  } else
    details.append(
      el(
        "p",
        tr("brain.ui.label.no-suggestions", "No suggested groupings yet."),
      ),
    );
  related.append(details);
  split.append(recent, related);
  out.append(split);
}
function sharedSubjects(focus) {
  const memberships = new Map();
  for (const hub of subjects())
    for (const n of notesIn(hub.id)) {
      if (!memberships.has(n.id)) memberships.set(n.id, []);
      memberships.get(n.id).push(hub.id);
    }
  const scores = new Map();
  for (const ids of memberships.values())
    for (let i = 0; i < ids.length; i++)
      for (let j = i + 1; j < ids.length; j++) {
        const pair = [ids[i], ids[j]].sort(),
          key = pair.join("\n");
        scores.set(key, (scores.get(key) || 0) + 1);
      }
  return [...scores]
    .map(([key, count]) => {
      let [a, b] = key.split("\n").map((id) => byId.get(id));
      if (focus && b.id === focus) [a, b] = [b, a];
      return { a, b, count };
    })
    .filter((p) => !focus || p.a.id === focus)
    .sort((a, b) => b.count - a.count);
}
function confidenceLabel(value) {
  const labels = {
    verified: () => tr("brain.ui.confidence.verified", "verified"),
    reported: () => tr("brain.ui.confidence.reported", "reported"),
    unverified: () => tr("brain.ui.confidence.unverified", "unverified"),
  };
  return (
    labels[value]?.() ||
    value ||
    tr("brain.ui.literal.source-page", "Source page")
  );
}
function noteRow(n) {
  const b = button(
    "",
    () => select(n.id),
    "note-row" + (state.selected === n.id ? " selected" : ""),
  );
  const copy = el("div");
  copy.append(el("span", n.label, "note-title"));
  copy.append(el("small", confidenceLabel(n.confidence)));
  for (const path of notePaths(n))
    copy.append(
      el(
        "small",
        [path.world, path.area].filter(Boolean).join(" › "),
        "note-path-chips",
      ),
    );
  const date = el("time", n.date || tr("brain.ui.label.undated", "Undated"));
  b.append(icon("note"), copy, date);
  return b;
}
// Ranked server row with a highlighted snippet. Results the partial
// first-paint graph lacks are added so the reader can open them.
function resultRow(r) {
  if (!byId.has(r.id)) byId.set(r.id, { ...r, label: r.title });
  const n = byId.get(r.id);
  const b = noteRow(n);
  const snippet = el("p", "", "note-snippet");
  const chars = Array.from(r.snippet || "");
  let at = 0;
  for (const [start, end] of r.highlights || []) {
    snippet.append(chars.slice(at, start).join(""));
    snippet.append(el("mark", chars.slice(start, end).join("")));
    at = end;
  }
  snippet.append(chars.slice(at).join(""));
  b.querySelector(".note-title").after(snippet);
  return b;
}
function renderNotes(out) {
  if (state.query && searchResults) {
    renderResults(out);
    return;
  }
  const available = ordered(
    state.focus === "review" ? notesIn("review") : notesIn(context()?.id),
  ).filter((n) => state.showArchive || n.kind !== "archive");
  const filtered = available.filter(
    (n) =>
      (!state.query ||
        (searchIds
          ? searchIds.has(n.id)
          : (
              n.label +
              " " +
              n.path +
              " " +
              ancestors(n.id)
                .map((v) => v.label)
                .join(" ")
            )
              .toLowerCase()
              .includes(state.query.toLowerCase()))) &&
      (state.filter === "all" ||
        (n.confidence || "")
          .trim()
          .toLowerCase()
          .split(/[\s·:]/)[0] === state.filter) &&
      (state.kindFilter === "all" || n.kind === state.kindFilter),
  );
  $("view-summary").textContent = tr(
    "brain.ui.count.notes",
    { one: "{count} notes", other: "{count} notes" },
    { count: filtered.length },
  );
  renderNotesFilters(out);
  for (const n of filtered.slice(0, state.limit)) out.append(noteRow(n));
  if (!filtered.length)
    out.append(
      el(
        "p",
        searchIds === null && state.query
          ? tr("brain.ui.label.searching", "Searching…")
          : tr(
              "brain.ui.label.no-matching-notes-try-a-different-search-or-filter",
              "No matching notes. Try a different search or filter.",
            ),
        "empty",
      ),
    );
  if (filtered.length > state.limit)
    out.append(
      button(
        tr("brain.ui.literal.show-more-notes", "Show more notes"),
        () => {
          state.limit += 60;
          renderMain();
        },
        "load-more",
      ),
    );
}
function renderNotesFilters(out) {
  const filters = el("div", "", "filters");
  for (const [value, label] of [
    ["all", tr("brain.ui.literal.all-notes", "All notes")],
    ["verified", tr("brain.ui.literal.verified", "Verified")],
    ["reported", tr("brain.ui.literal.reported", "Reported")],
    ["unverified", tr("brain.ui.literal.unverified", "Unverified")],
  ])
    filters.append(
      button(
        label,
        () => {
          state.filter = value;
          state.limit = 60;
          renderMain();
        },
        state.filter === value ? "active" : "",
      ),
    );
  for (const [value, label] of [
    ["all", tr("brain.ui.text.all-kinds", "All kinds")],
    ["note", tr("brain.ui.label.notes-kind", "Notes")],
    ["archive", tr("brain.ui.label.archive", "Archive")],
    ["digest", tr("brain.ui.label.digest", "Digests")],
  ])
    filters.append(
      button(
        label,
        () => {
          state.kindFilter = value;
          if (value === "archive") state.showArchive = true;
          state.limit = 60;
          renderMain();
        },
        state.kindFilter === value ? "active" : "",
      ),
    );
  filters.append(
    button(
      state.showArchive
        ? tr("brain.ui.label.hide-archive", "Hide archive")
        : tr("brain.ui.label.show-archive", "Show archive"),
      toggleArchive,
      state.showArchive ? "active" : "",
    ),
  );
  out.append(filters);
}
function renderResults(out) {
  const results = searchResults.filter(
    (r) =>
      (state.filter === "all" ||
        (r.confidence || "")
          .trim()
          .toLowerCase()
          .split(/[\s·:]/)[0] === state.filter) &&
      (state.kindFilter === "all" || r.kind === state.kindFilter),
  );
  // `ids` lists every ranked match; rows carry the first page with snippets.
  const unfiltered = state.filter === "all" && state.kindFilter === "all";
  $("view-summary").textContent = tr(
    "brain.ui.count.notes",
    { one: "{count} notes", other: "{count} notes" },
    { count: unfiltered ? searchIds.size : results.length },
  );
  if (searchSuggestion) {
    const line = el("p", "", "did-you-mean");
    line.append(
      tr("brain.ui.label.did-you-mean", "Did you mean"),
      " ",
      button(searchSuggestion, () => {
        $("search").value = searchSuggestion;
        $("search").dispatchEvent(new Event("input"));
      }),
    );
    out.append(line);
  }
  renderNotesFilters(out);
  for (const r of results.slice(0, state.limit)) out.append(resultRow(r));
  if (!results.length)
    out.append(
      el(
        "p",
        tr(
          "brain.ui.label.no-matching-notes-try-a-different-search-or-filter",
          "No matching notes. Try a different search or filter.",
        ),
        "empty",
      ),
    );
  if (results.length > state.limit)
    out.append(
      button(
        tr("brain.ui.literal.show-more-notes", "Show more notes"),
        () => {
          state.limit += 60;
          renderMain();
        },
        "load-more",
      ),
    );
}
async function search() {
  searchController?.abort();
  if (!state.query) {
    searchIds = searchResults = null;
    renderMain();
    return;
  }
  const query = state.query;
  searchController = new AbortController();
  try {
    const result = await boundedRequest(
      "/api/search?q=" +
        encodeURIComponent(query) +
        (state.showArchive ? "&archive=1" : ""),
      {
        signal: searchController.signal,
      },
      "json",
    );
    if (state.query === query) {
      searchIds = new Set(result.ids);
      searchResults = result.results || null;
      searchSuggestion = result.didYouMean || "";
      renderMain();
      clearLoadError();
    }
  } catch (e) {
    if (e.name !== "AbortError") {
      $("index-status").textContent = tr(
        "brain.ui.label.search-unavailable-showing-title-matches",
        "Search unavailable; showing title matches.",
      );
      showLoadError(e.message, () => search());
    }
  }
}
function svgEl(tag, attributes = {}, value) {
  const n = document.createElementNS(NS, tag);
  for (const [k, v] of Object.entries(attributes)) n.setAttribute(k, v);
  if (value) n.textContent = value;
  return n;
}
function lines(label, max = 27) {
  const result = [],
    words = label.split(/\s+/);
  let line = "";
  for (const word of words) {
    if ((line + " " + word).trim().length > max && line) {
      result.push(line);
      line = word;
    } else line = (line + " " + word).trim();
  }
  if (line) result.push(line);
  return result
    .slice(0, 2)
    .map((v, i) =>
      v.length > max || (i === 1 && result.length > 2)
        ? v.slice(0, max - 1) + "…"
        : v,
    );
}
function mapNode(svg, n, x, y, width = 225, focus = false) {
  const g = svgEl("g", {
    class: "map-node" + (focus ? " focus" : ""),
    transform: `translate(${x} ${y})`,
    role: "button",
    tabindex: "0",
    "aria-label": n.label,
  });
  g.append(
    svgEl("title", {}, n.label),
    svgEl("rect", { width, height: 82, rx: 12 }),
    svgEl("circle", {
      class: "node-mark",
      cx: 15,
      cy: 19,
      r: isNote(n) ? 2 : 3,
    }),
  );
  const names = lines(n.label);
  names.forEach((line, i) =>
    g.append(svgEl("text", { x: 27, y: 23 + i * 18 }, line)),
  );
  const detail = isNote(n)
    ? n.date || tr("brain.ui.detail.evidence-note", "Evidence note")
    : tr(
        "brain.ui.count.notes",
        { one: "{count} notes", other: "{count} notes" },
        { count: n.count },
      );
  g.append(svgEl("text", { class: "node-detail", x: 27, y: 69 }, detail));
  const open = () => select(n.id, { view: "map" });
  g.addEventListener("click", open);
  g.addEventListener("keydown", (e) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      open();
    }
  });
  svg.append(g);
}
function renderMap(out) {
  $("view-summary").textContent = tr(
    "brain.ui.label.focused-connections",
    "Focused connections",
  );
  const controls = el("div", "", "map-controls");
  controls.append(
    button("−", () => {
      mapZoom = Math.max(0.6, mapZoom - 0.15);
      renderMain();
    }),
    button(tr("brain.ui.literal.reset-view", "Reset view"), () => {
      mapZoom = 1;
      renderMain();
    }),
    button("+", () => {
      mapZoom = Math.min(2, mapZoom + 0.15);
      renderMain();
    }),
  );
  out.append(controls);
  const scroll = el("div", "", "map-scroll"),
    svg = svgEl("svg", {
      class: "knowledge-map",
      role: "img",
      "aria-label": tr(
        "brain.ui.literal.knowledge-connections",
        "Knowledge connections",
      ),
    });
  let focus = byId.get(state.focus),
    nodes,
    height;
  if (!focus) {
    nodes = subjects();
    height = Math.max(330, Math.ceil(nodes.length / 3) * 135 + 70);
    svg.setAttribute("viewBox", `0 0 830 ${height}`);
    const positions = new Map(
      nodes.map((n, i) => [
        n.id,
        { x: 30 + (i % 3) * 270, y: 35 + Math.floor(i / 3) * 135 },
      ]),
    );
    for (const p of sharedSubjects().slice(0, 9)) {
      const a = positions.get(p.a.id),
        b = positions.get(p.b.id);
      svg.append(
        svgEl("path", {
          class: "map-edge",
          d: `M${a.x + 112} ${a.y + 41}L${b.x + 112} ${b.y + 41}`,
        }),
      );
    }
    for (const n of nodes) {
      const p = positions.get(n.id);
      mapNode(svg, n, p.x, p.y);
    }
  } else {
    let connected = isNote(focus)
      ? [...(sourceNeighbours.get(focus.id) || [])]
          .map((id) => byId.get(id))
          .filter(Boolean)
      : children(focus.id);
    connected = [...connected].sort((a, b) =>
      a.kind === b.kind
        ? (b.recorded || b.date || "").localeCompare(
            a.recorded || a.date || "",
          ) || a.label.localeCompare(b.label)
        : a.kind === "area"
          ? -1
          : 1,
    );
    if (isNote(focus))
      connected = [
        ...connected,
        ...ancestors(focus.id).filter(
          (n) => n.kind === "area" || n.kind === "world",
        ),
      ];
    connected = [...new Map(connected.map((n) => [n.id, n])).values()].filter(
      (n) => state.showArchive || n.kind !== "archive",
    );
    nodes = connected.slice(0, 16);
    height = Math.max(330, Math.ceil(nodes.length / 2) * 110 + 70);
    svg.setAttribute("viewBox", `0 0 830 ${height}`);
    const fy = height / 2 - 41;
    for (let i = 0; i < nodes.length; i++) {
      const x = 310 + (i % 2) * 260,
        y = 35 + Math.floor(i / 2) * 110,
        explicit = sourceNeighbours.get(focus.id)?.has(nodes[i].id);
      svg.append(
        svgEl("path", {
          class: "map-edge" + (explicit ? " explicit" : ""),
          d: `M255 ${fy + 41}C280 ${fy + 41} 280 ${y + 41} ${x} ${y + 41}`,
        }),
      );
    }
    mapNode(svg, focus, 30, fy, 225, true);
    nodes.forEach((n, i) =>
      mapNode(svg, n, 310 + (i % 2) * 260, 35 + Math.floor(i / 2) * 110),
    );
    if (connected.length > 16)
      out.append(
        el(
          "p",
          connected.length -
            16 +
            tr(
              "brain.ui.label.more-connections-overview-shows-all-topics-notes-shows-all-evidence",
              " more connections. Overview shows all topics; Notes shows all evidence.",
            ),
          "activity-caption",
        ),
      );
    if (!nodes.length)
      out.append(
        el(
          "p",
          tr(
            "brain.ui.label.no-explicit-references-yet-open-this-note-to-follow-its-subjects",
            "No explicit references yet. Open this note to follow its subjects.",
          ),
          "empty",
        ),
      );
  }
  svg.style.width = mapZoom * 100 + "%";
  scroll.append(svg);
  out.append(scroll);
  const legend = el("div", "", "map-caption");
  legend.append(
    el(
      "span",
      tr(
        "brain.ui.label.topic-grouping-or-shared-notes",
        "Topic grouping or shared notes",
      ),
    ),
    el(
      "span",
      tr("brain.ui.label.explicit-note-reference", "Explicit note reference"),
      "explicit",
    ),
  );
  out.append(legend);
}
function badge(value) {
  const v = value || tr("brain.ui.literal.source-page", "Source page");
  return el(
    "span",
    confidenceLabel(value),
    "badge" + (v.toLowerCase() === "verified" ? " verified" : ""),
  );
}
function renderInspector() {
  const panel = $("inspector"),
    n = byId.get(state.selected);
  if (window.BrainGraphUI?.isActive() || state.view !== "read" || !isNote(n)) {
    panel.hidden = true;
    return;
  }
  panel.hidden = false;
  panel.replaceChildren();
  noteRequest++;
  const tools = el("div", "", "reader-tools");
  tools.append(
    button(
      tr(
        "brain.ui.literal.open-original-in-obsidian",
        "Open original in Obsidian",
      ),
      () => apiAction("open-note", n.path),
      "inspector-action",
    ),
    button(
      tr("brain.ui.literal.show-connections", "Show connections"),
      () => {
        state.focus = n.id;
        state.view = "map";
        state.selected = null;
        render();
      },
      "inspector-action",
    ),
    button(
      tr("brain.ui.text.show-in-graph", "Show in graph"),
      () => window.BrainGraphUI?.open(`note:${n.id}`, false),
      "inspector-action",
    ),
  );
  if (n.path?.startsWith("wiki/"))
    tools.prepend(
      button(
        tr("brain.ui.label.open-in-wiki", "Open in Wiki"),
        () => openWiki(n.id),
        "inspector-action",
      ),
    );
  panel.append(tools);
  const meta = el("div", "", "metadata");
  meta.append(
    badge(n.confidence),
    el("span", n.date || tr("brain.ui.label.undated", "Undated")),
  );
  panel.append(meta);
  const grouping = el("div", "", "grouping");
  grouping.append(
    el(
      "div",
      tr("brain.ui.label.filed-under", "Filed under"),
      "inspector-label",
    ),
  );
  grouping.append(
    el(
      "p",
      notePaths(n)
        .map((path) => [path.world, path.area].filter(Boolean).join(" › "))
        .join(" · ") || tr("brain.ui.label.needs-review", "Needs review"),
    ),
  );
  panel.append(grouping);
  const reader = el(
    "div",
    tr("brain.ui.label.reading-note", "Reading note…"),
    "reader",
  );
  panel.append(reader);
  readNote(n, reader, noteRequest);
  const links = el("div", "", "reader-connections");
  panel.append(links, el("div", n.path, "reader-path"));
  readConnections(n, links, noteRequest);
}
// Wiki layer (N0144): readable world → area → entity pages led by their
// current facts, with an evidence index into the detailed note reader.
let pageLinks = {};
const wikiCache = new Map();
function safeDecode(value) {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}
function ensureNode(summary) {
  if (!summary?.id) return null;
  if (!byId.has(summary.id)) {
    const node = { ...summary, label: summary.label || summary.title };
    byId.set(node.id, node);
    graph.nodes.push(node);
  }
  return byId.get(summary.id);
}
function wikiKey() {
  return (graph.graphRevision || "summary") + (graph.connections?.state || "");
}
// Cached JSON for the wiki view; a changed key refreshes in the background
// while the previous value stays visible.
function wikiData(url) {
  const key = wikiKey(),
    hit = wikiCache.get(url);
  if (!hit || hit.key !== key) {
    wikiCache.set(url, { key, value: hit?.value || null });
    requestJson(url)
      .then((value) => wikiCache.set(url, { key, value }))
      .catch((error) =>
        wikiCache.set(url, { key, value: { error: error.message } }),
      )
      .finally(() => {
        if (state.view === "wiki" && !state.query) renderMain();
      });
  }
  return wikiCache.get(url).value;
}
function openWiki(id) {
  state.history.push({
    focus: state.focus,
    selected: state.selected,
    view: state.view,
  });
  state.focus = id;
  state.selected = null;
  state.view = "wiki";
  state.query = "";
  $("search").value = "";
  searchIds = searchResults = null;
  render();
  $("content-scroll").scrollTop = 0;
}
function wikiTarget() {
  const id = state.focus;
  if (!id || id === "review") return null;
  if (/^(world|area):/.test(id)) return id;
  const focus = byId.get(id);
  if (!focus || focus.path?.startsWith("wiki/")) return id;
  return context()?.id || null;
}
function wikiCrumbs(page) {
  const crumbs = $("breadcrumbs");
  crumbs.replaceChildren(
    button(tr("brain.ui.text.wiki", "Wiki"), () => home("wiki")),
  );
  for (const part of [page?.world, page?.area, page?.parent].filter(Boolean))
    crumbs.append(
      el("span", "›"),
      button(part.label, () => openWiki(part.id)),
    );
}
function staleBadge(stale) {
  const badge = el("span", tr("brain.ui.label.stale", "Stale"), "stale-badge");
  badge.title =
    stale.class === "address"
      ? tr(
          "brain.ui.label.stale-address",
          "Address or host state checked {days} days ago (limit {limit})",
          stale,
        )
      : tr(
          "brain.ui.label.stale-version",
          "Version or deploy state checked {days} days ago (limit {limit})",
          stale,
        );
  return badge;
}
function sourceLink(target, base) {
  const note = resolveNote(target, base);
  if (!note) return el("span", target.split("/").pop(), "fact-source");
  const link = el("a", note.label, "fact-source");
  link.href = "#" + encodeURIComponent(note.id);
  link.onclick = (event) => {
    event.preventDefault();
    select(note.id);
  };
  return link;
}
function renderFacts(out, page) {
  const base = page.page?.path || "wiki/";
  out.append(
    el("h2", tr("brain.ui.label.current-facts", "Current facts"), "wiki-h2"),
  );
  let list = el("ul", "", "facts");
  out.append(list);
  for (const fact of page.facts || []) {
    if (fact.heading) {
      out.append(el("h3", fact.heading, "facts-group"));
      list = el("ul", "", "facts");
      out.append(list);
      continue;
    }
    const item = el(
      "li",
      "",
      "fact" +
        (fact.superseded ? " superseded" : "") +
        (fact.stale ? " stale" : ""),
    );
    if (fact.indent) item.style.marginInlineStart = fact.indent * 1.25 + "rem";
    const body = el(fact.superseded ? "s" : "span", "", "fact-text");
    inline(body, fact.text, base);
    item.append(body);
    if (fact.stale) item.append(" ", staleBadge(fact.stale));
    const meta = el("span", "", "provenance");
    if (fact.provenance?.date) {
      const time = el("time", fact.provenance.date);
      time.dateTime = fact.provenance.date;
      time.title = fact.provenance.verb;
      meta.append(time);
    }
    for (const source of fact.provenance?.sources || [])
      meta.append(sourceLink(source, base));
    if (fact.superseded) {
      const note = el("span", "", "superseded-by");
      note.append(
        fact.superseded.date
          ? tr("brain.ui.label.superseded-on", "Superseded {date}", {
              date: fact.superseded.date,
            })
          : tr("brain.ui.label.superseded", "Superseded"),
      );
      for (const source of fact.superseded.by || [])
        note.append(" ", sourceLink(source, base));
      meta.append(note);
    }
    if (meta.childNodes.length) item.append(meta);
    list.append(item);
  }
  if (!page.facts?.some((fact) => fact.text)) {
    list.remove();
    if (page.factsNote) {
      const note = el("div", "", "wiki-prose");
      markdown(note, page.factsNote, base);
      out.append(note);
    } else
      out.append(
        el(
          "p",
          tr(
            "brain.ui.label.no-current-facts",
            "No current facts yet. The consolidation job fills them in from the evidence below.",
          ),
          "empty",
        ),
      );
  } else if (page.factsNote) {
    const note = el("div", "", "wiki-prose");
    markdown(note, page.factsNote, base);
    out.append(note);
  }
}
function factCounts(summary) {
  const facts = summary?.facts || {},
    parts = [];
  if (facts.current)
    parts.push(
      tr(
        "brain.ui.count.facts",
        { one: "{count} fact", other: "{count} facts" },
        { count: facts.current },
      ),
    );
  return parts.join(" · ");
}
function wikiCard(label, summary, extra, onClick, className = "") {
  const card = button("", onClick, "wiki-card" + className);
  const top = el("div", "", "wiki-card-top");
  top.append(el("small", extra || "", "wiki-kind"));
  if (summary?.facts?.stale)
    top.append(
      el(
        "span",
        tr(
          "brain.ui.count.stale-facts",
          { one: "{count} stale", other: "{count} stale" },
          { count: summary.facts.stale },
        ),
        "stale-badge",
      ),
    );
  card.append(top, el("h3", label));
  if (summary?.summary) card.append(el("p", summary.summary));
  const counts = factCounts(summary);
  if (counts) card.append(el("small", counts, "wiki-card-foot"));
  return card;
}
function relatedReason(why = {}) {
  const parts = [];
  if (why.e?.length)
    parts.push(
      tr("brain.ui.label.related-shares", "Shares {names}", {
        names: why.e.join(", "),
      }),
    );
  if (why.a)
    parts.push(
      tr("brain.ui.label.related-area", "Same area: {area}", { area: why.a }),
    );
  if (why.l) parts.push(tr("brain.ui.label.related-link", "Linked"));
  if (why.w)
    parts.push(tr("brain.ui.label.related-wording", "Similar wording"));
  return parts.join(" · ");
}
function connectionList(out, title, rows, reasons = false) {
  if (!rows?.length) return;
  out.append(el("h3", title, "evidence-heading"));
  for (const row of rows) {
    const node = ensureNode(row),
      item = noteRow(node);
    if (reasons) {
      const why = relatedReason(row.why);
      if (why)
        item
          .querySelector(".note-title")
          .after(el("small", why, "related-reason"));
    }
    out.append(item);
  }
}
function renderEvidence(out, page) {
  const box = el("section", "", "wiki-evidence");
  box.append(el("h2", tr("brain.ui.label.evidence", "Evidence"), "wiki-h2"));
  const evidence = page.evidence;
  if (!evidence) {
    box.append(
      el(
        "p",
        tr("brain.ui.status.building-connections", "Building connections…"),
        "empty",
      ),
    );
    out.append(box);
    return;
  }
  box.append(
    el(
      "p",
      tr(
        "brain.ui.count.evidence-notes",
        {
          one: "{count} evidence note. Open one for the full detail.",
          other: "{count} evidence notes. Open one for the full detail.",
        },
        { count: evidence.count },
      ),
      "wiki-muted",
    ),
  );
  connectionList(
    box,
    tr("brain.ui.label.latest-notes", "Latest notes"),
    evidence.latest,
  );
  connectionList(box, tr("brain.ui.label.digest", "Digests"), evidence.digests);
  connectionList(
    box,
    tr("brain.ui.label.linked-from", "Linked from"),
    evidence.linkedFrom,
  );
  connectionList(
    box,
    tr("brain.ui.label.related", "Related"),
    evidence.related,
    true,
  );
  out.append(box);
}
function renderWikiHome(out) {
  $("title").textContent = tr("brain.ui.text.wiki", "Wiki");
  $("subtitle").textContent = tr(
    "brain.ui.label.wiki-intro",
    "What your brain currently knows, summarised. Open any note for the full evidence.",
  );
  $("breadcrumbs").replaceChildren();
  const index = wikiData("/api/wiki"),
    pages = new Map((index?.worlds || []).map((world) => [world.id, world]));
  $("view-summary").textContent = tr(
    "brain.ui.count.worlds",
    { one: "{count} worlds", other: "{count} worlds" },
    { count: registryWorlds().length },
  );
  const cards = el("div", "", "wiki-cards");
  for (const world of registryWorlds()) {
    const entry = pages.get(world.id),
      card = wikiCard(
        world.label,
        entry?.page,
        tr(
          "brain.ui.count.notes",
          { one: "{count} notes", other: "{count} notes" },
          { count: world.count || 0 },
        ),
        () => openWiki(world.id),
      );
    const chips = el("div", "", "wiki-chips");
    for (const area of areasFor(world).slice(0, 6))
      chips.append(
        el(
          "span",
          area.proposed
            ? tr("brain.ui.label.new-area", "New: {area}", { area: area.label })
            : area.label,
          "wiki-chip",
        ),
      );
    if (chips.childNodes.length) card.append(chips);
    cards.append(card);
  }
  out.append(cards);
  if (index?.other?.length) {
    out.append(
      el("h2", tr("brain.ui.label.other-pages", "Other pages"), "wiki-h2"),
    );
    const other = el("div", "", "wiki-cards compact");
    for (const page of index.other)
      other.append(
        wikiCard(page.name, page, page.entity, () => openWiki(page.id)),
      );
    out.append(other);
  }
  if (index?.error) out.append(el("p", index.error, "empty"));
  const recent = ordered(notesIn(null).filter((n) => n.kind === "note")).slice(
    0,
    6,
  );
  if (recent.length) {
    out.append(
      el(
        "h2",
        tr("brain.ui.label.recently-saved", "Recently saved"),
        "wiki-h2",
      ),
    );
    for (const note of recent) out.append(noteRow(note));
  }
}
function renderWiki(out) {
  const target = wikiTarget();
  if (!target) {
    renderWikiHome(out);
    return;
  }
  const page = wikiData("/api/wiki/page?id=" + encodeURIComponent(target));
  if (!page || page.error) {
    $("title").textContent =
      byId.get(target)?.label || tr("brain.ui.text.wiki", "Wiki");
    $("subtitle").textContent = "";
    wikiCrumbs(null);
    out.append(
      el(
        "p",
        page?.error ||
          tr("brain.ui.status.loading-wiki", "Loading the wiki page…"),
        "empty",
      ),
    );
    return;
  }
  pageLinks = page.links || {};
  wikiCrumbs(page);
  $("title").textContent = page.title;
  $("subtitle").textContent =
    page.kind === "world"
      ? tr("brain.ui.label.wiki-world", "World")
      : page.kind === "area"
        ? tr("brain.ui.label.wiki-area", "Area in {world}", {
            world: page.world?.label || "",
          })
        : (page.page?.entity || "").replace(/^./, (c) => c.toUpperCase());
  $("view-summary").textContent = factCounts({ facts: page.counts });
  const article = el("article", "", "wiki-page");
  const base = page.page?.path || "wiki/";
  if (page.intro) {
    const intro = el("div", "", "wiki-prose wiki-intro");
    markdown(intro, page.intro, base);
    article.append(intro);
  }
  renderFacts(article, page);
  const subpages = page.subpages || [];
  if (subpages.length) {
    article.append(
      el(
        "h2",
        page.kind === "world"
          ? tr("brain.ui.label.areas", "Areas")
          : tr("brain.ui.label.pages", "Pages"),
        "wiki-h2",
      ),
    );
    const cards = el("div", "", "wiki-cards compact");
    for (const sub of subpages) {
      const id = sub.areaId || sub.id,
        label = sub.proposed
          ? tr("brain.ui.label.new-area", "New: {area}", { area: sub.label })
          : sub.label || sub.name;
      cards.append(
        wikiCard(
          label,
          sub.id ? sub : null,
          sub.areaId
            ? tr(
                "brain.ui.count.notes",
                { one: "{count} notes", other: "{count} notes" },
                {
                  count:
                    areasFor(byId.get(page.id) || { id: page.id }).find(
                      (area) => area.id === sub.areaId,
                    )?.count || 0,
                },
              )
            : sub.entity,
          () => openWiki(id),
        ),
      );
    }
    article.append(cards);
  }
  if (page.body) {
    const body = el("div", "", "wiki-prose");
    markdown(body, page.body, base);
    article.append(body);
  }
  const info = el("p", "", "wiki-info");
  for (const [label, value] of [
    [tr("brain.ui.label.status", "Status"), page.meta?.status],
    [tr("brain.ui.label.reviewed", "Reviewed"), page.meta?.reviewed],
    [
      tr("brain.ui.label.aliases", "Also known as"),
      page.meta?.aliases?.join(", "),
    ],
  ])
    if (value) info.append(el("span", label + ": " + value));
  if (page.page)
    info.append(
      button(
        tr("brain.ui.label.open-full-note", "Open the full note"),
        () => select(page.page.id),
        "wiki-open-note",
      ),
    );
  if (!page.page)
    info.append(
      el(
        "span",
        tr(
          "brain.ui.label.no-wiki-page",
          "No wiki page yet; this summary is built from the evidence.",
        ),
      ),
    );
  article.append(info);
  renderEvidence(article, page);
  out.append(article);
}
async function readConnections(n, container, token) {
  try {
    const data = await boundedRequest(
      "/api/connections?id=" + encodeURIComponent(n.id),
      {},
      "json",
    );
    if (token !== noteRequest) return;
    if (data.state !== "ready") {
      container.replaceChildren(
        el(
          "p",
          tr("brain.ui.status.building-connections", "Building connections…"),
          "empty",
        ),
      );
      setTimeout(() => {
        if (token === noteRequest) readConnections(n, container, token);
      }, 1000);
      return;
    }
    container.replaceChildren();
    if (data.mentions?.length) {
      container.append(
        el("div", tr("brain.ui.label.mentions", "Mentions"), "inspector-label"),
      );
      const chips = el("div", "", "wiki-chips");
      for (const target of data.mentions)
        chips.append(
          button(
            target.label,
            () => openWiki(target.kind === "area" ? target.id : target.page),
            "wiki-chip",
          ),
        );
      container.append(chips);
    }
    for (const [label, rows, reasons] of [
      [tr("brain.ui.literal.references", "References"), data.linksTo],
      [tr("brain.ui.label.linked-from", "Linked from"), data.linkedFrom],
      [tr("brain.ui.label.related", "Related"), data.related, true],
    ]) {
      if (!rows?.length) continue;
      container.append(el("div", label, "inspector-label"));
      for (const row of rows) {
        const item = noteRow(ensureNode(row));
        const why = reasons && relatedReason(row.why);
        if (why)
          item
            .querySelector(".note-title")
            .after(el("small", why, "related-reason"));
        container.append(item);
      }
    }
  } catch {
    if (token === noteRequest) container.replaceChildren();
  }
}
async function readNote(n, container, token) {
  try {
    const data = await boundedRequest(
      "/api/note?path=" + encodeURIComponent(n.path),
      {},
      "json",
    );
    if (token !== noteRequest) return;
    clearLoadError();
    container.replaceChildren();
    markdown(container, data.text, n.path);
  } catch (e) {
    if (token === noteRequest) {
      container.textContent = tr(
        "brain.ui.label.could-not-read-this-note-try-selecting-it-again",
        "Could not read this note. Try selecting it again.",
      );
      showLoadError(e.message, () => readNote(n, container, token));
    }
  }
}
function resolveNote(target, base) {
  let value = target.split("|")[0].split("#")[0].trim();
  if (!value) return null;
  const known = pageLinks[value] || pageLinks[safeDecode(value)];
  if (known) return ensureNode(known);
  const candidates = [value, value + ".md"];
  try {
    const p = new URL(value, "http://local/" + base).pathname.slice(1);
    candidates.push(decodeURIComponent(p));
  } catch {}
  let found = graph.nodes.find((n) => isNote(n) && candidates.includes(n.path));
  if (found) return found;
  const name = value.replace(/\.md$/, "").split("/").pop().toLowerCase(),
    matches = graph.nodes.filter(
      (n) =>
        isNote(n) &&
        (n.label.toLowerCase() === name ||
          n.path.split("/").pop().replace(/\.md$/, "").toLowerCase() === name),
    );
  return matches.length === 1 ? matches[0] : null;
}
function inline(parent, value, base) {
  const re =
    /\[\[([^\]]+)\]\]|\[([^\]]+)\]\(([^)]+)\)|\*\*([^*]+)\*\*|`([^`]+)`/g;
  let last = 0;
  for (const m of value.matchAll(re)) {
    parent.append(document.createTextNode(value.slice(last, m.index)));
    let node;
    if (m[4]) node = el("strong", m[4]);
    else if (m[5]) node = el("code", m[5]);
    else {
      const target = m[1] || m[3],
        label = m[1] ? m[1].split("|")[1] || m[1].split("#")[0] : m[2],
        note = resolveNote(target, base);
      if (note) {
        node = el("a", m[1] && !m[1].includes("|") ? note.label : label);
        node.href = "#" + encodeURIComponent(note.id);
        node.onclick = (e) => {
          e.preventDefault();
          select(note.id);
        };
      } else if (!m[1] && /^https?:\/\//i.test(target)) {
        node = el("a", label);
        node.href = target;
        node.target = "_blank";
        node.rel = "noopener noreferrer";
      } else node = el("span", label);
    }
    parent.append(node);
    last = m.index + m[0].length;
  }
  parent.append(document.createTextNode(value.slice(last)));
}
function markdown(container, body, base) {
  let code = null,
    list = null,
    paragraph = [],
    details = null;
  function flush() {
    if (paragraph.length) {
      const p = el("p");
      inline(p, paragraph.join(" "), base);
      container.append(p);
      paragraph = [];
    }
    list = null;
  }
  const rows = body.split("\n");
  for (let index = 0; index < rows.length; index++) {
    const line = rows[index];
    if (
      !code &&
      line.includes("|") &&
      /^\s*\|?\s*:?-{3,}:?\s*(\|\s*:?-{3,}:?\s*)+\|?\s*$/.test(
        rows[index + 1] || "",
      )
    ) {
      flush();
      const table = el("table"),
        head = el("thead"),
        header = el("tr"),
        split = (value) =>
          value.trim().replace(/^\|/, "").replace(/\|$/, "").split("|");
      for (const value of split(line)) {
        const cell = el("th");
        inline(cell, value.trim(), base);
        header.append(cell);
      }
      head.append(header);
      table.append(head);
      const body = el("tbody");
      index += 2;
      while (
        index < rows.length &&
        rows[index].includes("|") &&
        rows[index].trim()
      ) {
        const row = el("tr");
        for (const value of split(rows[index])) {
          const cell = el("td");
          inline(cell, value.trim(), base);
          row.append(cell);
        }
        body.append(row);
        index++;
      }
      index--;
      table.append(body);
      const wrap = el("div", "", "table-scroll");
      wrap.append(table);
      container.append(wrap);
      continue;
    }
    if (line.startsWith("```")) {
      flush();
      if (code) {
        container.append(el("pre", code.join("\n")));
        code = null;
      } else code = [];
      continue;
    }
    if (code) {
      code.push(line);
      continue;
    }
    if (!line.trim()) {
      flush();
      continue;
    }
    const metadata = line.match(
      /^(Entity|Name|Project|Aliases|Reviewed|Status|Source|Recorded|Date|Confidence|Worlds|Areas|Parent|Review|Topics|Kind):\s*(.*)$/,
    );
    if (metadata) {
      flush();
      if (!details) {
        details = el("details");
        details.append(
          el("summary", tr("brain.ui.label.note-details", "Note details")),
        );
        container.append(details);
      }
      const row = el("div", "", "source-field");
      row.append(el("strong", metadata[1]));
      const value = el("span");
      inline(value, metadata[2], base);
      row.append(value);
      details.append(row);
      continue;
    }
    const heading = line.match(/^(#{1,4})\s+(.+)$/),
      bullet = line.match(/^\s*(?:[-*]|\d+\.)\s+(.+)$/);
    if (heading) {
      flush();
      const h = el("h" + heading[1].length);
      inline(h, heading[2], base);
      container.append(h);
    } else if (bullet) {
      if (paragraph.length) flush();
      if (!list) {
        list = el("ul");
        container.append(list);
      }
      const li = el("li");
      inline(li, bullet[1], base);
      list.append(li);
    } else if (line.startsWith("> ")) {
      flush();
      const q = el("blockquote");
      inline(q, line.slice(2), base);
      container.append(q);
    } else {
      if (list) list = null;
      paragraph.push(line);
    }
  }
  flush();
  if (code) container.append(el("pre", code.join("\n")));
}
async function navigation() {
  try {
    const n = await boundedRequest("/api/navigation", {}, "json");
    if (n.graphRevision && n.graphRevision !== graph.graphRevision)
      await load();
    if (!n.token || lastNavigation === n.token) return;
    const target = graph.nodes.find((v) => v.path === n.path);
    if (target) {
      lastNavigation = n.token;
      select(target.id, { view: "notes" });
    }
  } catch {}
}
$("brand").onclick = () => home("wiki");
$("ask-open").onclick = () => askActivate(true);
$("ask-close").onclick = () => askActivate(false);
$("ask-enable").onclick = async () => {
  try {
    const response = await fetch("/api/ask/settings", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ enabled: true }),
    });
    const data = await response.json();
    if (!response.ok)
      throw new Error(tr("brain.ui.error.ask-enable", "Could not enable Ask."));
    askEnabled = !!data.enabled;
    $("ask-disabled").hidden = askEnabled;
    $("ask-form").hidden = !askEnabled;
    $("ask-question").focus();
  } catch (error) {
    $("ask-status").textContent = tr(
      "brain.ui.error.ask-enable",
      "Could not enable Ask.",
    );
    $("ask-status").hidden = false;
  }
};
$("ask-form").addEventListener("submit", submitAsk);
$("wiki-link").onclick = () => home("wiki");
$("overview-link").onclick = () => home("overview");
$("refresh").onclick = () => load(true);
$("retry").onclick = () => retryAction();
$("back").onclick = back;
window.BrainGraphUI.request = requestJson;
window.BrainGraphUI.onClose = () => render();
window.BrainGraphUI.onOpenNote = (source) => {
  const n = {
    ...source,
    kind: ["archive", "digest"].includes(source.kind) ? source.kind : "note",
  };
  graph.nodes = graph.nodes.filter((item) => item.id !== n.id).concat(n);
  byId.set(n.id, n);
  window.BrainGraphUI.close();
  select(n.id, { view: "read" });
};
for (const b of document.querySelectorAll("[data-view]"))
  b.onclick = () => {
    state.view = b.dataset.view;
    mapZoom = 1;
    render();
  };
for (const b of document.querySelectorAll("[data-action]"))
  b.onclick = () => apiAction(b.dataset.action);
let searchDelay;
$("search").oninput = () => {
  state.query = $("search").value.trim();
  state.view = "notes";
  state.selected = null;
  state.limit = 60;
  state.focus = null;
  searchIds = searchResults = null;
  clearTimeout(searchDelay);
  renderMain();
  searchDelay = setTimeout(search, 250);
};
document.addEventListener("keydown", (e) => {
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
    e.preventDefault();
    $("search").focus();
    $("search").select();
  } else if (e.key === "Escape") {
    if (state.query) {
      $("search").value = "";
      state.query = "";
      searchIds = searchResults = null;
      renderMain();
    } else if (state.history.length) back();
    else if (state.selected) {
      state.selected = null;
      render();
    }
  }
});
document.addEventListener("visibilitychange", () => {
  if (!document.hidden) {
    load();
    navigation();
  }
});
load();
setInterval(() => {
  if (!document.hidden) load();
}, 30000);
setInterval(() => {
  if (!document.hidden && ready) navigation();
}, 1000);
