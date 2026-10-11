"use strict";
const $ = (id) => document.getElementById(id),
  NS = "http://www.w3.org/2000/svg";
let graph = { nodes: [], links: [], noteCount: 0 },
  byId = new Map(),
  groupChildren = new Map(),
  parents = new Map(),
  sourceNeighbours = new Map();
let state = {
    view: "overview",
    focus: null,
    selected: null,
    query: "",
    filter: "all",
    limit: 60,
    history: [],
  },
  ready = false,
  loading = false,
  noteRequest = 0,
  lastNavigation = "",
  mapZoom = 1,
  lastSignature = "",
  searchIds = null,
  searchController = null;
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
function subjectIcon(n) {
  return (
    {
      ai: "network",
      electronics: "chip",
      software: "code",
      infrastructure: "chip",
      personal: "person",
      music: "music",
      research: "research",
    }[n.category] || "folder"
  );
}
function button(label, fn, className = "") {
  const b = el("button", label, className);
  b.onclick = fn;
  return b;
}
function ordered(items) {
  return [...items].sort(
    (a, b) =>
      (b.recorded || b.date || "").localeCompare(a.recorded || a.date || "") ||
      a.label.localeCompare(b.label),
  );
}
function subjects() {
  return graph.nodes
    .filter((n) => n.kind === "hub")
    .sort(
      (a, b) =>
        (b.activity || 0) - (a.activity || 0) || a.label.localeCompare(b.label),
    );
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
  if (!id) return graph.nodes.filter((n) => n.kind === "note");
  const result = new Map(),
    seen = new Set();
  function walk(key) {
    if (seen.has(key)) return;
    seen.add(key);
    const n = byId.get(key);
    if (n?.kind === "note") result.set(key, n);
    else for (const c of children(key)) walk(c.id);
  }
  walk(id);
  return [...result.values()];
}
function context() {
  const focus = byId.get(state.focus);
  return focus?.kind === "note"
    ? ancestors(focus.id).find((n) => n.kind === "topic") ||
        ancestors(focus.id).find((n) => n.kind === "hub")
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
  loading = true;
  try {
    const r = await fetch("/api/brain");
    if (!r.ok) throw Error("Local brain unavailable");
    const previous = byId.get(state.selected);
    graph = await r.json();
    const signature = JSON.stringify([graph.nodes, graph.links, graph.theme]),
      changed = force || signature !== lastSignature;
    lastSignature = signature;
    byId = new Map(graph.nodes.map((n) => [n.id, n]));
    groupChildren = new Map();
    parents = new Map();
    sourceNeighbours = new Map();
    for (const link of graph.links) {
      if (link.kind === "group") {
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
    if (state.focus && !byId.has(state.focus)) state.focus = null;
    if (state.selected && !byId.has(state.selected)) state.selected = null;
    $("index-status").textContent =
      graph.noteCount +
      " notes · " +
      new Date(graph.generated).toLocaleTimeString([], {
        hour: "2-digit",
        minute: "2-digit",
      });
    if (changed)
      render(
        !force &&
          previous?.kind === "note" &&
          previous.revision === byId.get(previous.id)?.revision,
      );
    if (state.query) search();
  } catch (e) {
    $("index-status").textContent = e.message;
    if (!ready)
      $("loading").textContent =
        "Could not read your brain. Use Refresh knowledge to retry.";
  } finally {
    loading = false;
  }
}
async function apiAction(name, value = "") {
  try {
    const r = await fetch("/api/action", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ name, value }),
    });
    if (!r.ok) throw Error("Could not open this action");
  } catch (e) {
    $("index-status").textContent = e.message;
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
  if (n.kind !== "note" || view === "map") state.focus = id;
  state.selected = inspect ? id : null;
  state.view = n.kind === "note" ? "read" : view;
  state.limit = 60;
  mapZoom = 1;
  render();
  $("content-scroll").scrollTop = 0;
}
function home() {
  state = {
    ...state,
    focus: null,
    selected: null,
    view: "overview",
    query: "",
    limit: 60,
    history: [],
  };
  $("search").value = "";
  searchIds = null;
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
  $("subject-count").textContent = subjects().length;
  $("overview-link").classList.toggle("active", !state.focus);
  const related = state.focus
    ? new Set([state.focus, ...ancestors(state.focus).map((n) => n.id)])
    : new Set();
  for (const n of subjects()) {
    const b = button(
      "",
      () => {
        state.query = "";
        $("search").value = "";
        searchIds = null;
        select(n.id, { view: "overview" });
      },
      "subject" + (related.has(n.id) ? " active" : ""),
    );
    b.append(
      icon(subjectIcon(n)),
      el("span", n.label, "name"),
      el("span", String(n.count), "count"),
    );
    list.append(b);
  }
}
function section(title, action) {
  const n = el("div", "", "section-heading");
  n.append(el("h2", title));
  if (action) n.append(button(action.label, action.fn));
  return n;
}
function renderMain() {
  if (!ready) return;
  const c = context(),
    focus = byId.get(state.focus),
    searching = !!state.query;
  const crumbs = $("breadcrumbs");
  crumbs.replaceChildren();
  if (focus) {
    crumbs.append(button("Brain", home), el("span", "/"));
    const parent = ancestors(focus.id).find((n) => n.kind === "hub");
    if (parent && parent.id !== focus.id)
      crumbs.append(
        button(parent.label, () => select(parent.id)),
        el("span", "/"),
      );
    crumbs.append(el("span", focus.kind === "note" ? "Note" : focus.label));
  }
  const reading =
    state.view === "read" && byId.get(state.selected)?.kind === "note";
  document
    .querySelector(".content-scroll")
    .classList.toggle("reading", reading);
  $("inspector").hidden = !reading;
  $("content").hidden = reading;
  document.querySelector(".viewbar").hidden = reading;
  $("title").textContent = reading
    ? byId.get(state.selected).label
    : searching
      ? "Search results"
      : c?.label || "Your brain";
  $("subtitle").textContent = reading
    ? ""
    : searching
      ? "Matching titles, topics and note content."
      : c
        ? `${notesIn(c.id).length} notes · ${children(c.id).filter((n) => n.kind === "topic").length} topics`
        : "The subjects you return to, connected by evidence.";
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
  if (state.view === "map") {
    renderMap(out);
    return;
  }
  $("view-summary").textContent = c
    ? "Focused knowledge"
    : subjects().length + " subjects";
  if (!c) {
    const cards = el("div", "", "subjects-grid");
    const peak = Math.max(1, ...subjects().map((n) => n.activity || 0));
    for (const n of subjects()) {
      const b = button("", () => select(n.id), "subject-card"),
        top = el("div", "", "card-top");
      top.append(
        icon(subjectIcon(n)),
        el(
          "small",
          n.count +
            " notes" +
            (n.origin === "discovered" || n.origin === "promoted"
              ? " · Suggested"
              : ""),
        ),
      );
      const topics = children(n.id).filter((v) => v.kind === "topic");
      b.append(
        top,
        el("h2", n.label),
        el(
          "p",
          topics
            .slice(0, 3)
            .map((v) => v.label)
            .join(", ") || "Notes and ideas",
        ),
      );
      const track = el("div", "", "activity-track"),
        fill = el("span");
      fill.style.width = Math.max(2, ((n.activity || 0) / peak) * 100) + "%";
      track.append(fill);
      b.append(track);
      b.title =
        "Recent focus from dated evidence; older knowledge remains available.";
      cards.append(b);
    }
    out.append(cards);
    const caption = el("div", "", "activity-caption");
    caption.append(
      el("span", "Recent focus"),
      el("span", "From saved conversations and work"),
    );
    out.append(caption);
  } else {
    const topics = children(c.id).filter((n) => n.kind === "topic");
    if (topics.length) {
      out.append(section("Topics"));
      const list = el("div", "", "topics");
      for (const n of topics) {
        const b = button(n.label, () => select(n.id), "topic");
        b.append(el("span", String(n.count)));
        list.append(b);
      }
      out.append(list);
    }
  }
  const split = el("div", "", "split-sections"),
    recent = el("div"),
    related = el("div");
  recent.append(
    section("Recent knowledge", {
      label: "View notes",
      fn: () => {
        state.view = "notes";
        renderMain();
      },
    }),
  );
  for (const n of ordered(notesIn(c?.id)).slice(0, 8))
    recent.append(noteRow(n));
  if (!notesIn(c?.id).length)
    recent.append(
      el(
        "p",
        "No notes here yet. New knowledge will appear as it is saved.",
        "empty",
      ),
    );
  related.append(section("Across subjects"));
  const pairs = sharedSubjects(c?.kind === "hub" ? c.id : null).slice(0, 5);
  if (pairs.length) {
    for (const p of pairs) {
      const b = button(
        p.a.label + " / " + p.b.label,
        () => select(p.b.id, { view: "map" }),
        "connection-row",
      );
      b.append(
        el("small", p.count + " shared " + (p.count === 1 ? "note" : "notes")),
      );
      related.append(b);
    }
    related.append(
      el(
        "p",
        "Shared notes connect subjects; they do not establish a dependency.",
        "activity-caption",
      ),
    );
  } else
    related.append(
      el(
        "p",
        "Explicit references and shared notes will appear here as your knowledge connects.",
        "empty",
      ),
    );
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
function noteRow(n) {
  const b = button(
    "",
    () => select(n.id),
    "note-row" + (state.selected === n.id ? " selected" : ""),
  );
  const copy = el("div");
  copy.append(el("span", n.label, "note-title"));
  copy.append(el("small", n.confidence || "Source page"));
  const date = el("time", n.date || "Undated");
  b.append(icon("note"), copy, date);
  return b;
}
function renderNotes(out) {
  const available = ordered(notesIn(context()?.id));
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
          .split(/[\s·:]/)[0] === state.filter),
  );
  $("view-summary").textContent = filtered.length + " notes";
  const filters = el("div", "", "filters");
  for (const [value, label] of [
    ["all", "All notes"],
    ["verified", "Verified"],
    ["reported", "Reported"],
    ["unverified", "Unverified"],
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
  out.append(filters);
  for (const n of filtered.slice(0, state.limit)) out.append(noteRow(n));
  if (!filtered.length)
    out.append(
      el(
        "p",
        searchIds === null && state.query
          ? "Searching…"
          : "No matching notes. Try a different search or filter.",
        "empty",
      ),
    );
  if (filtered.length > state.limit)
    out.append(
      button(
        "Show more notes",
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
    searchIds = null;
    renderMain();
    return;
  }
  const query = state.query;
  searchController = new AbortController();
  try {
    const r = await fetch("/api/search?q=" + encodeURIComponent(query), {
      signal: searchController.signal,
    });
    if (!r.ok) throw Error("Search unavailable");
    const result = await r.json();
    if (state.query === query) {
      searchIds = new Set(result.ids);
      renderMain();
    }
  } catch (e) {
    if (e.name !== "AbortError")
      $("index-status").textContent =
        "Search unavailable; showing title matches.";
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
      r: n.kind === "note" ? 2 : 3,
    }),
  );
  const names = lines(n.label);
  names.forEach((line, i) =>
    g.append(svgEl("text", { x: 27, y: 23 + i * 18 }, line)),
  );
  const detail =
    n.kind === "note" ? n.date || "Evidence note" : n.count + " notes";
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
  $("view-summary").textContent = "Focused connections";
  const controls = el("div", "", "map-controls");
  controls.append(
    button("−", () => {
      mapZoom = Math.max(0.6, mapZoom - 0.15);
      renderMain();
    }),
    button("Reset view", () => {
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
      "aria-label": "Knowledge connections",
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
    let connected =
      focus.kind === "note"
        ? [...(sourceNeighbours.get(focus.id) || [])]
            .map((id) => byId.get(id))
            .filter(Boolean)
        : children(focus.id);
    connected = [...connected].sort((a, b) =>
      a.kind === b.kind
        ? (b.recorded || b.date || "").localeCompare(
            a.recorded || a.date || "",
          ) || a.label.localeCompare(b.label)
        : a.kind === "topic"
          ? -1
          : 1,
    );
    if (focus.kind === "note")
      connected = [
        ...connected,
        ...ancestors(focus.id).filter(
          (n) => n.kind === "topic" || n.kind === "hub",
        ),
      ];
    connected = [...new Map(connected.map((n) => [n.id, n])).values()];
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
            " more connections. Overview shows all topics; Notes shows all evidence.",
          "activity-caption",
        ),
      );
    if (!nodes.length)
      out.append(
        el(
          "p",
          "No explicit references yet. Open this note to follow its subjects.",
          "empty",
        ),
      );
  }
  svg.style.width = mapZoom * 100 + "%";
  scroll.append(svg);
  out.append(scroll);
  const legend = el("div", "", "map-caption");
  legend.append(
    el("span", "Topic grouping or shared notes"),
    el("span", "Explicit note reference", "explicit"),
  );
  out.append(legend);
}
function badge(value) {
  const v = value || "Source page";
  return el(
    "span",
    v,
    "badge" + (v.toLowerCase() === "verified" ? " verified" : ""),
  );
}
function renderInspector() {
  const panel = $("inspector"),
    n = byId.get(state.selected);
  if (state.view !== "read" || n?.kind !== "note") {
    panel.hidden = true;
    return;
  }
  panel.hidden = false;
  panel.replaceChildren();
  noteRequest++;
  const tools = el("div", "", "reader-tools");
  tools.append(
    button(
      "Open original in Obsidian",
      () => apiAction("open-note", n.path),
      "inspector-action",
    ),
    button(
      "Show connections",
      () => {
        state.focus = n.id;
        state.view = "map";
        state.selected = null;
        render();
      },
      "inspector-action",
    ),
  );
  panel.append(tools);
  const meta = el("div", "", "metadata");
  meta.append(badge(n.confidence), el("span", n.date || "Undated"));
  panel.append(meta);
  const grouping = el("div", "", "grouping");
  grouping.append(el("div", "Filed under", "inspector-label"));
  const labels = (n.memberships || [])
    .filter((m) => m.subject !== "review")
    .map((m) => byId.get(m.subject)?.label)
    .filter(Boolean);
  const summary = el("p", labels.join(", ") || "Needs grouping");
  grouping.append(summary);
  const reasons = [...new Set((n.memberships || []).map((m) => m.reason))];
  grouping.append(
    el(
      "small",
      !labels.length
        ? "Not enough context for a confident grouping"
        : reasons.includes("manual grouping")
          ? "Your grouping"
          : reasons.some(
                (r) =>
                  r.includes("similarity") ||
                  r.includes("community") ||
                  r.includes("suggested"),
              )
            ? "Suggested from related evidence"
            : "From annotations and named evidence",
    ),
  );
  const edit = button("Change grouping", () => {
    edit.hidden = true;
    const input = el("input");
    input.value = labels.join(", ");
    input.placeholder = "Subjects, separated by commas";
    input.setAttribute("aria-label", "Subjects for this note");
    const suggestions = el("datalist");
    suggestions.id = "subject-options";
    for (const subject of subjects().filter((s) => s.id !== "review")) {
      const option = el("option");
      option.value = subject.label;
      suggestions.append(option);
    }
    input.setAttribute("list", suggestions.id);
    const actions = el("div", "", "grouping-actions");
    actions.append(
      button(
        "Save",
        async () => {
          await apiAction(
            "assign-note",
            JSON.stringify({
              path: n.path,
              subjects: input.value
                .split(",")
                .map((v) => v.trim())
                .filter(Boolean),
            }),
          );
          await new Promise((r) => setTimeout(r, 500));
          load(true);
        },
        "primary",
      ),
      button("Use automatic grouping", async () => {
        await apiAction(
          "assign-note",
          JSON.stringify({ path: n.path, reset: true }),
        );
        await new Promise((r) => setTimeout(r, 500));
        load(true);
      }),
    );
    grouping.append(input, suggestions, actions);
    input.focus();
  });
  grouping.append(edit);
  panel.append(grouping);
  const reader = el("div", "Reading note…", "reader");
  panel.append(reader);
  readNote(n, reader, noteRequest);
  for (const [field, label, opposite] of [
    ["source", "References", "target"],
    ["target", "Referenced by", "source"],
  ]) {
    const related = graph.links
      .filter((e) => e.kind === "source" && e[field] === n.id)
      .map((e) => byId.get(e[opposite]))
      .filter(Boolean);
    if (related.length) {
      panel.append(el("div", label, "inspector-label"));
      for (const r of related) panel.append(noteRow(r));
    }
  }
  const hubs = [
    ...new Map(
      ancestors(n.id)
        .filter((p) => p.kind === "hub")
        .map((p) => [p.id, p]),
    ).values(),
  ];
  if (hubs.length) {
    panel.append(el("div", "Subjects", "inspector-label"));
    for (const h of hubs)
      panel.append(button(h.label, () => select(h.id), "topic"));
  }
  panel.append(el("div", n.path, "reader-path"));
}
async function readNote(n, container, token) {
  try {
    const r = await fetch("/api/note?path=" + encodeURIComponent(n.path));
    if (!r.ok) throw Error("Note unavailable");
    const data = await r.json();
    if (token !== noteRequest) return;
    container.replaceChildren();
    markdown(container, data.text, n.path);
  } catch (e) {
    if (token === noteRequest)
      container.textContent =
        "Could not read this note. Try selecting it again.";
  }
}
function resolveNote(target, base) {
  let value = target.split("|")[0].split("#")[0].trim();
  if (!value) return null;
  const candidates = [value, value + ".md"];
  try {
    const p = new URL(value, "http://local/" + base).pathname.slice(1);
    candidates.push(decodeURIComponent(p));
  } catch {}
  let found = graph.nodes.find(
    (n) => n.kind === "note" && candidates.includes(n.path),
  );
  if (found) return found;
  const name = value.replace(/\.md$/, "").split("/").pop().toLowerCase(),
    matches = graph.nodes.filter(
      (n) =>
        n.kind === "note" &&
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
      /^(Entity|Name|Project|Aliases|Reviewed|Status|Source|Recorded|Date|Confidence|Worlds|Topics|Kind):\s*(.*)$/,
    );
    if (metadata) {
      flush();
      if (!details) {
        details = el("details");
        details.append(el("summary", "Note details"));
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
    const r = await fetch("/api/navigation"),
      n = await r.json();
    if (!n.token || lastNavigation === n.token) return;
    const target = graph.nodes.find((v) => v.path === n.path);
    if (target) {
      lastNavigation = n.token;
      select(target.id, { view: "notes" });
    }
  } catch {}
}
$("brand").onclick = home;
$("overview-link").onclick = home;
$("refresh").onclick = () => load(true);
$("back").onclick = back;
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
  searchIds = null;
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
      searchIds = null;
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
