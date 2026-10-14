.pragma library

const operations = ["tool", "route", "calculator", "legacy", "scope", "toggle", "next-wallpaper", "personality", "background", "palette-mode", "setting-toggle", "settings", "config", "form", "maintenance", "info", "url", "power", "power-profile", "command", "media"];
const providers = ["apps", "wallpapers", "accents", "fonts", "software", "packages", "installed", "windows", "wifi", "bluetooth", "clipboard", "keys", "emoji", "timers", "webapps", "audio", "software-themes", "software-extensions", "environments", "software-ai", "tui-apps", "health-fixits", "system-setup"];
const fields = ["labelId", "descriptionId", "confirmId", "id", "kind", "label", "description", "icon", "operation", "value", "provider", "capability", "state", "confirm", "brand", "aliases", "hidden", "arguments"];
const aliases = {
    favorites: "home",
    all: "apps",
    actions: "actions",
    settings: "setup",
    system: "power",
    clipboard: "actions.clipboard",
    keys: "learn.shortcuts",
    overview: "actions.windows",
    palette: "style",
    wallpaper: "style.wallpapers"
};
const railOrder = ["home", "apps", "actions", "style", "setup", "software", "health", "learn", "power"];

function parentOf(id) {
    const index = id.lastIndexOf(".");
    return index < 0 ? "" : id.slice(0, index);
}
function plain(value) {
    return typeof value === "string" && value.length <= 4096 && !value.includes("\0");
}
function validId(id) {
    return typeof id === "string" && /^[a-z][a-z0-9-]*(\.[a-z0-9-]+)*$/.test(id) && id.length <= 120;
}
function validate(row, partial) {
    if (!row || typeof row !== "object" || Array.isArray(row) || !validId(row.id))
        throw new Error(tr("launcher.validation.menu-entries-need-a-lowercase-dotted-id", "Menu entries need a lowercase dotted ID"));
    for (const key of Object.keys(row)) {
        if (!fields.includes(key))
            throw new Error(tr("launcher.validation.unknown-menu-field", "Unknown menu field: {key}", {
                key: key
            }));
        if (["hidden"].includes(key) && typeof row[key] !== "boolean")
            throw new Error(tr("launcher.validation.hidden-must-be-true-or-false", "Hidden must be true or false"));
        if (!["hidden", "arguments", "aliases"].includes(key) && !plain(row[key]))
            throw new Error(tr("launcher.validation.invalid-menu-text", "Invalid menu text: {key}", {
                key: key
            }));
    }
    if (!partial && (!["group", "action"].includes(row.kind) || !row.label?.trim()))
        throw new Error(tr("launcher.validation.every-menu-entry-needs-a-kind-and-label", "Every menu entry needs a kind and label"));
    if (row.kind !== undefined && !["group", "action"].includes(row.kind))
        throw new Error(tr("launcher.validation.unknown-menu-kind", "Unknown menu kind"));
    if (row.operation !== undefined && !operations.includes(row.operation))
        throw new Error(tr("launcher.validation.unknown-menu-operation", "Unknown menu operation"));
    if (row.provider !== undefined && !providers.includes(row.provider))
        throw new Error(tr("launcher.validation.unknown-menu-provider", "Unknown menu provider"));
    for (const field of ["aliases", "arguments"]) {
        if (row[field] !== undefined && (!Array.isArray(row[field]) || row[field].length > 32 || row[field].some(value => !plain(value) || !value)))
            throw new Error(tr("launcher.validation.invalid-field", "Invalid {field}", {
                field: field
            }));
    }
    if (!partial && row.kind === "action" && !operations.includes(row.operation))
        throw new Error(tr("launcher.validation.an-action-needs-a-supported-operation", "An action needs a supported operation"));
    if (row.operation === "command" && (!row.arguments?.length || row.arguments[0].startsWith("-")))
        throw new Error(tr("launcher.validation.commands-use-an-explicit-executable-and-argument-array", "Commands use an explicit executable and argument array"));
}
function document(text) {
    if (typeof text !== "string" || text.length > 2097152)
        throw new Error(tr("launcher.validation.menu-file-is-too-large", "Menu file is too large"));
    const value = JSON.parse(text);
    if (!value || value.version !== 1 || !Array.isArray(value.entries) || value.entries.length > 1000)
        throw new Error(tr("launcher.validation.unsupported-menu-format", "Unsupported menu format"));
    return value;
}
function merge(baseText, userText) {
    const base = document(baseText), rows = base.entries.map(row => Object.assign({}, row));
    rows.forEach(row => validate(row, false));
    const ids = new Map(rows.map((row, index) => [row.id, index]));
    if (ids.size !== rows.length)
        throw new Error(tr("launcher.validation.duplicate-menu-id", "Duplicate menu ID"));
    let error = "";
    if (userText?.trim()) {
        try {
            const extension = document(userText), candidate = rows.map(row => Object.assign({}, row));
            const positions = new Map(ids), seen = new Set();
            for (const row of extension.entries) {
                validate(row, positions.has(row.id));
                if (seen.has(row.id))
                    throw new Error(tr("launcher.validation.duplicate-extension-id", "Duplicate extension ID"));
                seen.add(row.id);
                if (positions.has(row.id)) {
                    const previous = Object.assign({}, candidate[positions.get(row.id)]);
                    for (const field of ["label", "description", "confirm"])
                        if (row[field] !== undefined && row[field + "Id"] === undefined)
                            delete previous[field + "Id"];
                    candidate[positions.get(row.id)] = Object.assign(previous, row);
                } else {
                    positions.set(row.id, candidate.length);
                    candidate.push(Object.assign({}, row));
                }
            }
            checkTree(candidate);
            return {
                rows: visibleRows(candidate),
                software: base.software || [],
                fonts: base.fonts || [],
                error: ""
            };
        } catch (failure) {
            error = tr("launcher.validation.extension", "Menu extension: {error}. Built-in entries remain available.", {
                error: failure.message
            });
        }
    }
    checkTree(rows);
    return {
        rows: visibleRows(rows),
        software: base.software || [],
        fonts: base.fonts || [],
        error: error
    };
}
function visibleRows(rows) {
    return rows.filter(row => !row.hidden && !rows.some(parent => parent.hidden && row.id.startsWith(parent.id + ".")));
}
function checkTree(rows) {
    const entries = new Map(rows.map(row => [row.id, row]));
    const routeAliases = new Map();
    if (entries.get("home")?.kind !== "group" || entries.get("home").hidden)
        throw new Error(tr("launcher.validation.home-must-remain-available", "Home must remain available"));
    for (const row of rows) {
        validate(row, false);
        const parent = parentOf(row.id);
        if (parent && entries.get(parent)?.kind !== "group")
            throw new Error(tr("launcher.validation.missing-group-for", "Missing group for ") + row.id);
        for (const alias of row.aliases || []) {
            const key = alias.trim().toLowerCase();
            if (routeAliases.has(key) && routeAliases.get(key) !== row.id)
                throw new Error(tr("launcher.validation.ambiguous-menu-alias", "Ambiguous menu alias"));
            if (aliases[key] && aliases[key] !== row.id)
                throw new Error(tr("launcher.validation.reserved-route-alias", "Reserved route alias"));
            routeAliases.set(key, row.id);
        }
    }
}
function rail(rows) {
    return rows.filter(row => !parentOf(row.id) && row.kind === "group").map((row, index) => ({
                row: row,
                index: index,
                order: railOrder.indexOf(row.id)
            })).sort((a, b) => (a.order < 0 ? 99 : a.order) - (b.order < 0 ? 99 : b.order) || a.index - b.index).map(item => item.row);
}
function resolve(value, rows) {
    const route = String(value || "home").trim().toLowerCase();
    const exact = rows.find(row => row.id === route);
    if (exact)
        return exact.id;
    if (aliases[route] && rows.some(row => row.id === aliases[route]))
        return aliases[route];
    return rows.find(row => (row.aliases || []).some(alias => alias.toLowerCase() === route))?.id || "";
}
function trail(id, rows) {
    const parts = id.split("."), result = [];
    for (let index = 1; index <= parts.length; index++) {
        const row = rows.find(item => item.id === parts.slice(0, index).join("."));
        if (row)
            result.push({
                id: row.id,
                label: row.label
            });
    }
    return result;
}
function children(route, rows) {
    return rows.filter(row => parentOf(row.id) === route);
}
function scope(query) {
    const trimmed = String(query || "").replace(/^\s+/, ""), prefix = trimmed.charAt(0);
    const kind = ({
            ">": "actions",
            "=": "calculator",
            "#": "brain",
            "?": "ai",
            "/": "help",
            ":": "files"
        })[prefix] || "all";
    return {
        kind: kind,
        text: kind === "all" ? trimmed.trim() : trimmed.slice(1).trim(),
        prefix: kind === "all" ? "" : prefix
    };
}
function normalized(value) {
    return String(value || "").toLowerCase().replace(/[-_:]/g, " ").replace(/\s+/g, " ").trim();
}
function distance(a, b, limit) {
    if (Math.abs(a.length - b.length) > limit)
        return limit + 1;
    let prior = Array.from({
        length: b.length + 1
    }, (_, i) => i);
    for (let i = 1; i <= a.length; i++) {
        const next = [i];
        let minimum = i;
        for (let j = 1; j <= b.length; j++) {
            next[j] = Math.min(next[j - 1] + 1, prior[j] + 1, prior[j - 1] + (a[i - 1] === b[j - 1] ? 0 : 1));
            minimum = Math.min(minimum, next[j]);
        }
        if (minimum > limit)
            return limit + 1;
        prior = next;
    }
    return prior[b.length];
}
function searchIndex(row) {
    const name = normalized(row.name || row.label), alternate = (row.aliases || []).map(normalized);
    return {
        name: name,
        alternate: alternate,
        haystack: normalized([name, row.description, row.breadcrumb, row.id, row.matchedQuery, ...alternate].join(" ")),
        initials: name.split(/[^a-z0-9]+/).filter(Boolean).map(word => word[0]).join(""),
        tokens: [name, ...alternate].join(" ").split(/[^a-z0-9]+/).filter(Boolean)
    };
}
function matchScore(row, term) {
    const words = term.split(" ").filter(Boolean), prepared = row.searchIndex || searchIndex(row);
    const name = prepared.name, alternate = prepared.alternate, haystack = prepared.haystack;
    // Clipboard titles are excerpts of copied content, rather than item names.
    // Body matches should not displace an application name such as Discord.
    if (row.clipboard)
        return words.every(word => haystack.includes(word)) ? 55 : -1;
    if (name === term)
        return 120;
    if (name.startsWith(term))
        return 100;
    if (alternate.includes(term))
        // Leave more than the maximum ten-point usage boost below a name prefix.
        return 89;
    if (words.every(word => name.includes(word)))
        return 85;
    const initials = prepared.initials;
    if (term.length >= 2 && initials.startsWith(term))
        return 80;
    if (words.every(word => haystack.includes(word)))
        return 55;
    const tokens = prepared.tokens;
    if (words.every(word => tokens.some(token => token.includes(word) || word.length >= 4 && distance(word, token, word.length >= 7 ? 2 : 1) <= (word.length >= 7 ? 2 : 1))))
        return 48;
    return -1;
}
function ranked(rows, query, usage = {}, recent = [], now = Date.now() / 1000) {
    const term = normalized(query);
    if (!term)
        return rows.slice();
    const result = [];
    rows.forEach((row, index) => {
        const score = matchScore(row, term);
        if (score < 0)
            return;
        const id = stable(row), history = usage[id] || {}, position = recent.indexOf(id);
        const count = Number.isFinite(history.count) ? Math.max(0, history.count) : 0;
        const age = Number.isFinite(history.last) ? Math.max(0, now - history.last) : Infinity;
        const boost = Math.min(6, Math.log2(count + 1)) + Math.max(position < 0 ? 0 : 4 * (1 - position / 12), 4 * Math.exp(-age / (7 * 86400)));
        result.push({
            row: row,
            index: index,
            score: score + boost + (row.kind === "app" ? 3 : 0)
        });
    });
    return result.sort((a, b) => b.score - a.score || a.index - b.index).slice(0, 100).map(hit => hit.row);
}
function firstEnabled(rows) {
    return rows.findIndex(row => !row.disabled);
}
function step(rows, index, delta) {
    if (!rows.length)
        return -1;
    let next = index < 0 ? (delta < 0 ? rows.length - 1 : 0) : index + delta;
    while (next >= 0 && next < rows.length) {
        if (!rows[next].disabled)
            return next;
        next += delta < 0 ? -1 : 1;
    }
    return index;
}
function stable(row) {
    return row.kind === "app" ? row.id : row.favoriteId || "nacre-menu:" + row.id;
}
function favorite(id, apps, rows, legacy) {
    if (id.startsWith("nacre-menu:"))
        return rows.find(row => row.id === id.slice(11));
    if (id.startsWith("nacre-action:"))
        return legacy.find(row => row.id === id);
    return apps.find(row => row.id === id);
}
function alphabetical(rows) {
    return rows.slice().sort((a, b) => (a.name || a.label).toLowerCase().localeCompare((b.name || b.label).toLowerCase()) || stable(a).localeCompare(stable(b)));
}
function application(entry) {
    const brand = ({
            "nacre-ai": "ai",
            "nacre-brain": "brain",
            "nacre-colors": "colors",
            "nacre-settings": "settings",
            "nacre-welcome": "welcome"
        })[entry.id.replace(/\.desktop$/, "")] || "";
    return {
        id: entry.id,
        kind: "app",
        name: entry.name,
        label: entry.name,
        description: entry.comment || "Open this application",
        icon: entry.icon,
        brand: brand,
        aliases: entry.keywords || [],
        app: entry,
        breadcrumb: "All apps",
        operation: "app",
        disabled: false
    };
}
function entry(row, rows) {
    return Object.assign({}, row, {
        name: row.label,
        breadcrumb: trail(parentOf(row.id), rows).map(item => item.label).join(" › ")
    });
}

var translator = null;
function useTranslations(value) {
    translator = value;
}
function tr(id, fallback, args = {}) {
    return translator ? translator(id, fallback, args) : fallback.replace(/\{([A-Za-z_][A-Za-z0-9_]*)\}/g, (match, key) => Object.prototype.hasOwnProperty.call(args, key) ? String(args[key]) : match);
}
