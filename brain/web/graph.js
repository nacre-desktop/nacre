"use strict";
(() => {
  const $ = (id) => document.getElementById(id);
  const tr = (id, fallback) => window.NacreI18n.tr(id, fallback);
  const canvas = $("graph-canvas"),
    ctx = canvas.getContext("2d");
  let nodes = [],
    links = [],
    simulation,
    scope = "all",
    history = [],
    hover = null;
  let scale = 1,
    tx = 0,
    ty = 0,
    drag = null,
    dpr = 1,
    requestId = 0;
  let world = "all",
    kind = "all",
    query = "",
    selected = null,
    active = false,
    palette = {},
    themeSignature = "",
    themeColors = [];
  const color = (name) =>
    getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  const worldSlots = new Map();
  const paletteFor = (value) => {
    const world = value || "Brain";
    if (!worldSlots.has(world)) worldSlots.set(world, worldSlots.size);
    return themeColors[worldSlots.get(world) % themeColors.length];
  };
  const text = (node, value) => {
    node.textContent = value;
  };
  function resize() {
    const r = canvas.getBoundingClientRect();
    dpr = Math.min(2, devicePixelRatio || 1);
    canvas.width = Math.max(1, Math.round(r.width * dpr));
    canvas.height = Math.max(1, Math.round(r.height * dpr));
    if (nodes.length) fit();
    draw();
  }
  function size(n) {
    return Math.min(
      18,
      4 + Math.sqrt(Math.max(0, n.count || n.noteCount || 0)) * 1.25,
    );
  }
  function visible(n) {
    return (
      (world === "all" ||
        n.world === world ||
        n.category === world ||
        n.label === world) &&
      (kind === "all" || n.kind === kind)
    );
  }
  function point(n) {
    return {
      x: canvas.width / 2 + tx * dpr + (n.x || 0) * scale * dpr,
      y: canvas.height / 2 + ty * dpr + (n.y || 0) * scale * dpr,
    };
  }
  function draw() {
    if (!active) return;
    const w = canvas.width,
      h = canvas.height,
      line = color("--muted"),
      foreground = color("--text"),
      font = getComputedStyle(document.body).fontFamily;
    const base = [color("--accent"), color("--secondary"), color("--tertiary")];
    themeColors = [
      ...base,
      ...base.flatMap((token, i) => [
        `color-mix(in srgb, ${token} 65%, ${base[(i + 1) % 3]})`,
        `color-mix(in srgb, ${token} 35%, ${base[(i + 1) % 3]})`,
      ]),
    ];
    const signature = themeColors.join("|");
    if (signature !== themeSignature) {
      themeSignature = signature;
      palette = {};
    }
    ctx.clearRect(0, 0, w, h);
    ctx.save();
    const shown = new Set(nodes.filter(visible).map((n) => n.id));
    for (const l of links) {
      const a =
        typeof l.source === "object"
          ? l.source
          : nodes.find((n) => n.id === l.source);
      const b =
        typeof l.target === "object"
          ? l.target
          : nodes.find((n) => n.id === l.target);
      if (!a || !b || !shown.has(a.id) || !shown.has(b.id)) continue;
      const p = point(a),
        q = point(b);
      ctx.beginPath();
      ctx.moveTo(p.x, p.y);
      ctx.lineTo(q.x, q.y);
      const highlighted = hover && (a.id === hover.id || b.id === hover.id);
      ctx.strokeStyle = highlighted ? foreground : line;
      ctx.globalAlpha = highlighted
        ? 0.85
        : l.kind === "group" || l.kind === "membership"
          ? 0.45
          : 0.3;
      ctx.lineWidth = Math.max(0.7, dpr * 0.7);
      ctx.stroke();
    }
    ctx.globalAlpha = 1;
    const connected = new Set();
    if (hover)
      for (const l of links)
        if (
          l.source?.id === hover.id ||
          l.source === hover.id ||
          l.target?.id === hover.id ||
          l.target === hover.id
        ) {
          connected.add(typeof l.source === "object" ? l.source.id : l.source);
          connected.add(typeof l.target === "object" ? l.target.id : l.target);
        }
    const neighborLabels = new Set();
    if (hover)
      for (const id of connected) {
        if (neighborLabels.size >= 24) break;
        neighborLabels.add(id);
      }
    const positioned = [];
    for (const n of nodes) {
      if (!shown.has(n.id)) continue;
      const p = point(n),
        r = size(n) * dpr * Math.sqrt(scale),
        group = n.world || n.category || n.label;
      ctx.beginPath();
      ctx.arc(p.x, p.y, r, 0, Math.PI * 2);
      ctx.fillStyle = palette[group] || (palette[group] = paletteFor(group));
      ctx.globalAlpha = hover && n !== hover && !connected.has(n.id) ? 0.24 : 1;
      ctx.fill();
      if (n === hover) {
        ctx.lineWidth = 2 * dpr;
        ctx.strokeStyle = foreground;
        ctx.stroke();
      }
      const label =
        r >= 8 * dpr ||
        n === hover ||
        n === selected ||
        neighborLabels.has(n.id);
      if (label) {
        positioned.push({
          n,
          p,
          r,
          rank:
            n === hover
              ? 100000
              : n === selected
                ? 50000
                : neighborLabels.has(n.id)
                  ? 10000
                  : r,
        });
      }
    }
    // Labels are placed after every node so they always sit above their own node.
    const background = color("--panel") || color("--bg") || "#fff";
    const nodeDiscs = nodes
      .filter((n) => shown.has(n.id))
      .map((n) => ({ n, p: point(n), r: size(n) * dpr * Math.sqrt(scale) }));
    positioned.sort((a, b) => b.rank - a.rank);
    const labelBoxes = [];
    for (const item of positioned) {
      const { n, p, r } = item;
      const fontSize = Math.max(11, Math.min(18, 12 * Math.sqrt(scale))) * dpr;
      ctx.font = `${fontSize}px ${font}`;
      const labelWidth = Math.min(180 * dpr, ctx.measureText(n.label).width);
      const box = {
        x: p.x - labelWidth / 2 - 2 * dpr,
        y: p.y + r + 4 * dpr,
        w: labelWidth + 4 * dpr,
        h: fontSize + 4 * dpr,
      };
      const collides = (candidate) => {
        const labelCollision = labelBoxes.some(
          (b) =>
            candidate.x < b.x + b.w &&
            candidate.x + candidate.w > b.x &&
            candidate.y < b.y + b.h &&
            candidate.y + candidate.h > b.y,
        );
        const nodeCollision = nodeDiscs.some((d) => {
          if (d.n === n) return false;
          const cx = Math.max(
            candidate.x,
            Math.min(d.p.x, candidate.x + candidate.w),
          );
          const cy = Math.max(
            candidate.y,
            Math.min(d.p.y, candidate.y + candidate.h),
          );
          return Math.hypot(cx - d.p.x, cy - d.p.y) < d.r + 2 * dpr;
        });
        return labelCollision || nodeCollision;
      };
      if (n === hover && collides(box)) {
        for (
          let offset = 8 * dpr;
          offset <= 96 * dpr && collides(box);
          offset += 8 * dpr
        )
          box.y += 8 * dpr;
      }
      if (
        box.x < 0 ||
        box.x + box.w > canvas.width ||
        box.y < 0 ||
        box.y + box.h > canvas.height
      )
        continue;
      if (collides(box)) continue;
      labelBoxes.push(box);
      ctx.globalAlpha = n === hover ? 1 : 0.94;
      ctx.font = `${fontSize}px ${font}`;
      ctx.textAlign = "center";
      ctx.textBaseline = "top";
      ctx.lineJoin = "round";
      ctx.lineWidth = 4 * dpr;
      ctx.strokeStyle = background;
      ctx.strokeText(n.label, p.x, box.y + 2 * dpr, 180 * dpr);
      ctx.fillStyle = foreground;
      ctx.fillText(n.label, p.x, box.y + 2 * dpr, 180 * dpr);
    }
    ctx.globalAlpha = 1;
    drawLegend();
    ctx.restore();
  }
  function drawLegend() {
    const legend = $("graph-legend");
    if (!legend) return;
    const worlds = [
      ...new Set(nodes.map((n) => n.world || n.category).filter(Boolean)),
    ].sort();
    const rows = worlds.map((v) => [
      v,
      palette[v] || (palette[v] = paletteFor(v)),
    ]);
    const worldText = tr("brain.ui.text.graph-legend-world", "Colour = world");
    const sizeText = tr("brain.ui.text.graph-legend-size", "Size = note count");
    const kindText = tr("brain.ui.text.graph-legend-kind", "Entity / note");
    const signature = JSON.stringify([rows, worldText, sizeText, kindText]);
    if (legend.dataset.signature === signature) return;
    legend.dataset.signature = signature;
    legend.replaceChildren();
    const addExplanation = (value) => {
      const item = document.createElement("small");
      item.className = "graph-legend-explanation";
      item.textContent = value;
      legend.append(item);
    };
    addExplanation(worldText);
    for (const [name, swatch] of rows) {
      const item = document.createElement("span");
      const dot = document.createElement("i");
      dot.style.backgroundColor = swatch;
      const label = document.createElement("span");
      label.textContent = name;
      item.append(dot, label);
      legend.append(item);
    }
    addExplanation(sizeText);
    const kind = document.createElement("small");
    kind.className = "graph-legend-explanation graph-legend-kind";
    const dot = document.createElement("i");
    dot.className = "graph-kind-dot";
    kind.append(dot, document.createTextNode(kindText));
    legend.append(kind);
  }
  function hit(x, y) {
    let best = null,
      distance = Infinity;
    for (const n of nodes)
      if (visible(n)) {
        const p = point(n),
          d = Math.hypot(p.x - x * dpr, p.y - y * dpr);
        if (d < size(n) * dpr * Math.sqrt(scale) + 5 * dpr && d < distance) {
          best = n;
          distance = d;
        }
      }
    return best;
  }
  function fit() {
    const shown = nodes.filter(visible);
    if (!shown.length) return;
    const width = Math.max(1, canvas.clientWidth - 96),
      height = Math.max(1, canvas.clientHeight - 96);
    scale = Math.min(2.4, Math.max(0.04, scale || 1));
    let bounds;
    for (let i = 0; i < 4; i++) {
      const discs = shown.map((n) => ({
        x: (n.x || 0) * scale,
        y: (n.y || 0) * scale,
        r: size(n) * Math.sqrt(scale),
      }));
      bounds = {
        minX: Math.min(...discs.map((d) => d.x - d.r)),
        maxX: Math.max(...discs.map((d) => d.x + d.r)),
        minY: Math.min(...discs.map((d) => d.y - d.r)),
        maxY: Math.max(...discs.map((d) => d.y + d.r)),
      };
      const factor = Math.min(
        width / Math.max(1, bounds.maxX - bounds.minX),
        height / Math.max(1, bounds.maxY - bounds.minY),
      );
      scale = Math.min(2.4, Math.max(0.04, scale * factor));
    }
    const discs = shown.map((n) => ({
      x: (n.x || 0) * scale,
      y: (n.y || 0) * scale,
      r: size(n) * Math.sqrt(scale),
    }));
    bounds = {
      minX: Math.min(...discs.map((d) => d.x - d.r)),
      maxX: Math.max(...discs.map((d) => d.x + d.r)),
      minY: Math.min(...discs.map((d) => d.y - d.r)),
      maxY: Math.max(...discs.map((d) => d.y + d.r)),
    };
    tx = -(bounds.minX + bounds.maxX) / 2;
    ty = -(bounds.minY + bounds.maxY) / 2;
  }
  function loadData(data) {
    if (simulation) simulation.stop();
    selected = null;
    nodes = data.nodes.map((n) => ({
      ...n,
      kind:
        n.kind === "topic" && n.groupReason === "annotated" ? "entity" : n.kind,
    }));
    links = data.links.map((l) => ({ ...l }));
    // Until the registry supplies worlds, use the existing subject hierarchy.
    const byId = new Map(nodes.map((n) => [n.id, n]));
    const parents = new Map(
      nodes.filter((n) => n.parent).map((n) => [n.id, n.parent]),
    );
    for (const l of links)
      if (["group", "membership"].includes(l.kind) && !parents.has(l.target))
        parents.set(l.target, l.source);
    function worldFor(n, seen = new Set()) {
      if (n.world) return n.world;
      if (seen.has(n.id)) return n.category || n.label;
      seen.add(n.id);
      const parent = byId.get(parents.get(n.id));
      return parent
        ? worldFor(parent, seen)
        : ["hub", "world", "area"].includes(n.kind)
          ? n.label
          : n.category || "";
    }
    for (const n of nodes) n.world = worldFor(n);
    const valid = new Set(nodes.map((n) => n.id));
    links = links.filter(
      (l) =>
        valid.has(typeof l.source === "object" ? l.source.id : l.source) &&
        valid.has(typeof l.target === "object" ? l.target.id : l.target),
    );
    $("graph-view").dataset.layout = "running";
    const aspect = Math.max(
      0.5,
      Math.min(2.5, canvas.clientWidth / Math.max(1, canvas.clientHeight)),
    );
    simulation = d3Force
      .forceSimulation(nodes)
      .force(
        "link",
        d3Force
          .forceLink(links)
          .id((n) => n.id)
          .distance((l) =>
            l.kind === "group" || l.kind === "membership" ? 88 : 115,
          )
          .strength(0.16),
      )
      .force("charge", d3Force.forceManyBody().strength(-95).distanceMax(420))
      .force("x", d3Force.forceX(0).strength(0.014 / (aspect * aspect)))
      .force("y", d3Force.forceY(0).strength(0.014 * aspect * aspect))
      .force("collide", d3Force.forceCollide((n) => size(n) + 5).iterations(1))
      .alphaDecay(0.045)
      .on("tick", draw)
      .on("end", () => {
        fit();
        draw();
        $("graph-view").dataset.layout = "settled";
      });
    fillFilters();
    fit();
    draw();
  }
  function fillFilters() {
    const worlds = [
        ...new Set(nodes.map((n) => n.world || n.category).filter(Boolean)),
      ].sort(),
      kinds = [...new Set(nodes.map((n) => n.kind).filter(Boolean))].sort();
    const labels = {
      world: tr("brain.ui.label.world", "World"),
      hub: tr("brain.ui.text.graph-kind-category", "Category"),
      area: tr("brain.ui.text.graph-kind-category", "Category"),
      topic: tr("brain.ui.text.graph-kind-topic", "Topic"),
      entity: tr("brain.ui.text.graph-kind-entity", "Entity"),
      wiki: tr("brain.ui.text.graph-kind-wiki-page", "Wiki page"),
      note: tr("brain.ui.text.graph-kind-note", "Note"),
    };
    for (const [id, values, selected] of [
      ["graph-world", worlds, world],
      ["graph-kind", kinds, kind],
    ]) {
      const s = $(id),
        keep = selected,
        allLabel =
          id === "graph-world"
            ? tr("brain.ui.text.all-worlds", "All worlds")
            : tr("brain.ui.text.all-kinds", "All kinds");
      s.replaceChildren(new Option(allLabel, "all"));
      for (const v of values) s.add(new Option(labels[v] || v, v));
      s.value = [...s.options].some((o) => o.value === keep) ? keep : "all";
    }
  }
  async function open(next = "all", push = true) {
    const req = ++requestId;
    if (!active) history = next === "all" ? [] : ["all"];
    if (simulation) simulation.stop();
    if (push && active && next !== scope) history.push(scope);
    scope = next;
    active = true;
    query = "";
    world = kind = "all";
    $("graph-search").value = "";
    $("graph-world").value = "all";
    $("graph-kind").value = "all";
    $("graph-view").dataset.scope = next;
    $("graph-view").hidden = false;
    $("content").hidden = true;
    $("inspector").hidden = true;
    $("content-scroll").classList.remove("reading");
    $("back").hidden = true;
    $("graph-context").hidden = true;
    $("title").textContent = tr("brain.ui.text.graph", "Graph");
    $("subtitle").textContent = "";
    document.querySelector(".viewbar").hidden = true;
    $("graph-back").hidden = history.length === 0;
    $("graph-toggle").setAttribute("aria-pressed", "true");
    text(
      $("graph-crumb"),
      next === "all"
        ? tr("brain.ui.text.all-knowledge", "All knowledge")
        : next.startsWith("note:")
          ? tr("brain.ui.label.note", "Note")
          : next.slice(next.indexOf(":") + 1),
    );
    text(
      $("graph-status"),
      tr("brain.ui.status.loading-graph", "Loading graph…"),
    );
    resize();
    try {
      const data = await window.BrainGraphUI.request(
        "/api/graph?scope=" + encodeURIComponent(next),
      );
      if (req !== requestId || !active) return;
      if (data.state === "building") {
        text(
          $("graph-status"),
          tr("brain.ui.status.building-graph", "Building graph…"),
        );
        setTimeout(() => {
          if (active && req === requestId) open(next, false);
        }, 1500);
        return;
      }
      if (data.state !== "ready") throw Error("Graph unavailable");
      loadData(data);
      const target = nodes.find(
        (n) => n.id === next.slice(next.indexOf(":") + 1),
      );
      if (target) text($("graph-crumb"), target.label);
      text($("graph-status"), "");
    } catch (_) {
      if (req !== requestId || !active) return;
      text(
        $("graph-status"),
        tr("brain.ui.error.graph", "Graph could not be loaded."),
      );
    }
  }
  function close() {
    requestId++;
    active = false;
    if (simulation) simulation.stop();
    simulation = null;
    nodes = [];
    links = [];
    hover = null;
    selected = null;
    drag = null;
    palette = {};
    canvas.width = canvas.height = 1;
    $("graph-view").hidden = true;
    $("content").hidden = false;
    $("graph-toggle").setAttribute("aria-pressed", "false");
    window.BrainGraphUI?.onClose?.();
  }
  function narrow(n) {
    if (!n) return;
    const level =
      n.kind === "world"
        ? "world"
        : ["area", "hub", "topic", "entity"].includes(n.kind)
          ? "area"
          : null;
    if (level) open(level + ":" + n.label);
  }
  canvas.addEventListener("pointerdown", (e) => {
    canvas.setPointerCapture(e.pointerId);
    const n = hit(e.offsetX, e.offsetY);
    if (n && simulation) {
      selected = n;
      drag = {
        node: n,
        id: e.pointerId,
        startX: e.clientX,
        startY: e.clientY,
        moved: false,
      };
      n.fx = n.x;
      n.fy = n.y;
      simulation.alphaTarget(0.22).restart();
    } else
      drag = {
        pan: true,
        id: e.pointerId,
        x: e.offsetX,
        y: e.offsetY,
        startX: e.clientX,
        startY: e.clientY,
        moved: false,
        tx,
        ty,
      };
  });
  canvas.addEventListener("pointermove", (e) => {
    if (!drag) {
      hover = hit(e.offsetX, e.offsetY);
      canvas.style.cursor = hover ? "pointer" : "grab";
      $("graph-tip").textContent = hover?.label || "";
      $("graph-tip").hidden = !hover;
      draw();
      return;
    }
    if (drag.id === e.pointerId) {
      if (Math.hypot(e.clientX - drag.startX, e.clientY - drag.startY) > 4) {
        drag.moved = true;
      }
      if (drag.node) {
        const r = canvas.getBoundingClientRect();
        drag.node.fx = (e.offsetX - r.width / 2 - tx) / scale;
        drag.node.fy = (e.offsetY - r.height / 2 - ty) / scale;
      } else if (drag.pan) {
        tx = drag.tx + e.offsetX - drag.x;
        ty = drag.ty + e.offsetY - drag.y;
        draw();
      }
    }
  });
  const end = () => {
    if (drag?.node) {
      drag.node.fx = null;
      drag.node.fy = null;
      simulation?.alphaTarget(0);
    }
    drag = null;
  };
  canvas.addEventListener("pointerup", (e) => {
    const n = hit(e.offsetX, e.offsetY),
      click = !drag?.moved;
    if (n && click) {
      if (n.kind === "note" || n.kind === "wiki")
        window.BrainGraphUI?.openNote(n);
      else narrow(n);
    }
    end();
  });
  canvas.addEventListener("pointercancel", end);
  canvas.addEventListener("pointerleave", () => {
    if (!drag) {
      hover = null;
      $("graph-tip").hidden = true;
      draw();
    }
  });
  canvas.addEventListener(
    "wheel",
    (e) => {
      e.preventDefault();
      const old = scale;
      scale = Math.max(0.08, Math.min(3, scale * Math.exp(-e.deltaY * 0.001)));
      tx =
        (tx + canvas.clientWidth / 2 - e.offsetX) * (scale / old) -
        canvas.clientWidth / 2 +
        e.offsetX;
      ty =
        (ty + canvas.clientHeight / 2 - e.offsetY) * (scale / old) -
        canvas.clientHeight / 2 +
        e.offsetY;
      draw();
    },
    { passive: false },
  );
  $("graph-toggle").addEventListener("click", () =>
    active ? close() : open("all"),
  );
  $("graph-back").addEventListener("click", () =>
    open(history.pop() || "all", false),
  );
  $("graph-search").addEventListener("input", (e) => {
    query = e.target.value.trim().toLowerCase();
    if (query) {
      const n = nodes.find(
        (v) =>
          v.label.toLowerCase().includes(query) ||
          v.id.toLowerCase().includes(query),
      );
      if (n) {
        tx = -(n.x || 0) * scale;
        ty = -(n.y || 0) * scale;
        hover = n;
        selected = n;
      }
    }
    draw();
  });
  $("graph-world").addEventListener("change", (e) => {
    world = e.target.value;
    fit();
    draw();
  });
  $("graph-kind").addEventListener("change", (e) => {
    kind = e.target.value;
    fit();
    draw();
  });
  $("graph-fit")?.addEventListener("click", () => {
    fit();
    draw();
  });
  new ResizeObserver(resize).observe($("graph-stage"));
  new MutationObserver(() => {
    if (active) draw();
  }).observe(document.documentElement, {
    attributes: true,
    attributeFilter: ["class", "style"],
  });
  window.BrainGraphUI = {
    open,
    close,
    fit: () => {
      fit();
      draw();
    },
    isActive: () => active,
    isRunning: () => !!simulation && simulation.alpha() > simulation.alphaMin(),
    get nodes() {
      return nodes;
    },
    screenNode(id) {
      const n = nodes.find((v) => v.id === id);
      if (!n || !active) return null;
      const p = point(n),
        r = canvas.getBoundingClientRect();
      return { x: p.x / dpr + r.left, y: p.y / dpr + r.top };
    },
    openNote(node) {
      window.BrainGraphUI?.onOpenNote?.(node);
    },
    onOpenNote: null,
    onClose: null,
    request: null,
  };
})();
