#!/usr/bin/env python3
"""Private knowledge index and browser actions."""

import argparse
import fcntl
import datetime as dt
import hashlib
import json
import math
import importlib
import importlib.util
import threading
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from http.cookies import SimpleCookie, CookieError
from urllib.parse import urlparse, parse_qs, quote, unquote

# Shared state is independent of the palette Python environment.
for _state_directory in (
    Path(__file__).resolve().parents[1] / "nacre/shell-tools",
    Path.home() / ".local/share/nacre/shell/tools",
):
    if (_state_directory / "nacre_state.py").is_file():
        sys.path.insert(0, str(_state_directory))
        break
_state = importlib.import_module("nacre_state")

ROOT = Path(__file__).resolve().parent
HOME = Path.home()
STATE = HOME / ".local/state/nacre/brain"
VAULT = Path(
    os.environ.get(
        "NACRE_BRAIN_DIR",
        os.environ.get("NACRE_BRAIN", str(HOME / "Documents/Nacre-Brain")),
    )
)
PORT = int(os.environ.get("NACRE_BRAIN_PORT", "17843"))
if not 1024 <= PORT <= 65535:
    raise ValueError("Invalid Brain port")
THEMES = {
    "ai": (
        "AI & learning",
        "#a9c9f2",
        [
            "ai",
            "artificial intelligence",
            "machine learning",
            "inference",
            "tensorrt",
            "neural",
            "codex",
            "claude",
            "llm",
        ],
    ),
    "electronics": (
        "Electronics & hardware",
        "#efbd87",
        [
            "electronics",
            "circuit",
            "solder",
            "sensor",
            "camera",
            "gun",
            "bosch",
            "firmware",
            "motor",
            "battery",
        ],
    ),
    "software": (
        "Software & games",
        "#c3b2ec",
        [
            "software",
            "frontend",
            "backend",
            "api",
            "game",
            "parser",
            "code",
            "release",
            "package",
        ],
    ),
    "infrastructure": (
        "Systems & networks",
        "#87cbbb",
        ["network", "server", "deployment", "sync", "ssh", "wifi", "latency"],
    ),
    "personal": (
        "Personal life",
        "#e6c796",
        [
            "personal",
            "bouldering",
            "climbing",
            "goals",
            "background",
            "education",
            "preference",
        ],
    ),
    "music": (
        "Music & sound",
        "#dda9c8",
        ["music", "musikki", "spotify", "audio", "sound"],
    ),
    "research": (
        "Research & science",
        "#a4d6dc",
        ["research", "science", "experiment", "investigation", "comparison"],
    ),
    "general": ("Mixed knowledge", "#a2b8c4", []),
}


def compositor():
    """Load the common facade from a source checkout or the installed shell."""
    for directory in (
        ROOT.parent / "nacre/shell-tools",
        Path.home() / ".local/share/nacre/shell/tools",
    ):
        if (directory / "compositor.py").is_file():
            if str(directory) not in sys.path:
                sys.path.insert(0, str(directory))
            return importlib.import_module("compositor")
    raise RuntimeError("The Nacre compositor service is not installed")


def compositor_action(name, *args, **options):
    # Optional window housekeeping had always ignored transient IPC failures.
    try:
        return getattr(compositor(), name)(*args, timeout=3, **options)
    except (OSError, subprocess.SubprocessError):
        return None


def slug(value):
    return re.sub(r"[^a-z0-9]+", "-", str(value).lower()).strip("-")[:80]


def fields(body):
    clean = re.sub(r"(?ms)^```[^\n]*\n.*?^```[^\n]*$", "", body)
    clean = re.split(r"^##\s", clean, maxsplit=1, flags=re.M)[0]
    return {
        k.lower(): v.strip().strip("\"'")
        for k, v in re.findall(
            r"(?mi)^(Entity|Name|Parent|Project|Worlds|Topics|Tags|Category|Aliases):\s*([^\n]+)",
            clean,
        )
    }


def items(value):
    return [
        s.strip().strip("\"'") for s in str(value).strip("[]").split(",") if s.strip()
    ]


def mentions(text, term):
    if term.endswith("/"):
        return text.lower().startswith(term.lower())
    return bool(
        re.search(r"(?<!\w)" + re.escape(term.lower()) + r"(?!\w)", text.lower())
    )


def category(label, body="", declared=""):
    if declared in THEMES:
        return declared, "declared"
    body = re.sub(
        r"(?mi)^(Source|Recorded|Reviewed|Confidence|Status|Entity|Name|Project|Parent|Category|Aliases|Topics|Tags):[^\n]*",
        "",
        body,
    )
    scores = {
        key: sum(
            4 * int(mentions(label, t)) + int(mentions(body[:1800], t)) for t in terms
        )
        for key, (_, _, terms) in THEMES.items()
    }
    best = max(scores, key=scores.get)
    return (best, "inferred") if scores[best] else ("general", "inferred")


def local_module(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / (name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


DISCOVERY = local_module("discovery")
SEMANTICS = local_module("semantic-client").SemanticClient(ROOT)
GRAPH_LOCK = threading.RLock()
GRAPH_CACHE = {}


def run(argv, fallback="", timeout=3):
    try:
        return subprocess.check_output(
            argv, text=True, stderr=subprocess.DEVNULL, timeout=timeout
        ).strip()
    except (OSError, subprocess.SubprocessError):
        return fallback


def launch(argv):
    subprocess.Popen(
        argv,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        start_new_session=True,
    )


def launch_app(argv):
    launch(
        [
            "systemd-run",
            "--user",
            "--scope",
            "--collect",
            "--quiet",
            "env",
            "-u",
            "LD_LIBRARY_PATH",
            "-u",
            "QML_IMPORT_PATH",
            "-u",
            "QT_PLUGIN_PATH",
            *argv,
        ]
    )


def note_path(relative):
    p = (VAULT / relative).resolve()
    if (
        not p.is_relative_to(VAULT.resolve())
        or p.suffix != ".md"
        or not p.is_file()
        or any(x.startswith(".") for x in Path(relative).parts)
    ):
        raise ValueError("Note is outside the managed Markdown vault")
    return p


def note_date(body):
    match = re.search(r"^(?:Recorded|Reviewed):\s*(\d{4}-\d{2}-\d{2})", body, re.M)
    try:
        return dt.date.fromisoformat(match.group(1)) if match else None
    except ValueError:
        return None


def note_timestamp(body):
    match = re.search(r"^Recorded:\s*(\S+)", body, re.M)
    try:
        value = (
            dt.datetime.fromisoformat(match.group(1).replace("Z", "+00:00"))
            if match
            else None
        )
        if value:
            return (
                value.replace(tzinfo=value.tzinfo or dt.timezone.utc)
                .astimezone(dt.timezone.utc)
                .isoformat()
            )
    except ValueError:
        pass
    return ""


def _graph_notes(today):
    notes = []
    lookup = {}
    if VAULT.exists():
        for p in sorted(VAULT.rglob("*.md")):
            rel = p.relative_to(VAULT).as_posix()
            if not p.resolve().is_relative_to(VAULT.resolve()):
                continue
            if any(x.startswith(".") for x in Path(rel).parts) or rel in (
                "AGENTS.md",
                "CLAUDE.md",
                "README.md",
                "INDEX.md",
            ):
                continue
            body = p.read_text(errors="replace")
            title = next(
                (x[2:].strip() for x in body.splitlines() if x.startswith("# ")), p.stem
            )
            date = note_date(body)
            age = max(0, (today - date).days) if date else None
            weight = math.exp(-math.log(2) * age / 21) if age is not None else 0
            ident = "note:" + hashlib.sha256(rel.encode()).hexdigest()[:16]
            confidence = re.search(r"^Confidence:\s*(.*)", body, re.M)
            meta = fields(body)
            theme, reason = category(title, body, meta.get("category", ""))
            item = dict(
                id=ident,
                label=title,
                kind="note",
                path=rel,
                date=date.isoformat() if date else "",
                recorded=note_timestamp(body),
                confidence=confidence.group(1) if confidence else "source page",
                text=body[:60000],
                activity=round(weight, 4),
                revision=hashlib.sha256(body.encode()).hexdigest()[:16],
                meta=meta,
                category=theme,
                color=THEMES[theme][1],
            )
            notes.append(item)
            lookup[rel] = ident
    return notes, lookup


def _graph_source_links(notes, lookup):
    source_pairs = set()
    stems = {}
    for path, ident in lookup.items():
        stems.setdefault(Path(path).stem.casefold(), []).append(ident)
    for n in notes:
        targets = [
            (value, False)
            for value in re.findall(r"\]\(([^)]+\.md)(?:#[^)]*)?\)", n["text"])
        ]
        targets += [
            (value, True) for value in re.findall(r"\[\[([^]\n]+)\]\]", n["text"])
        ]
        for value, wiki in targets:
            target = unquote(value.split("|")[0].split("#")[0]).strip()
            if not target or "://" in target:
                continue
            if wiki and not target.endswith(".md"):
                target += ".md"
            candidates = (
                [VAULT / target, (VAULT / n["path"]).parent / target]
                if wiki
                else [(VAULT / n["path"]).parent / target]
            )
            ident = None
            for path in candidates:
                try:
                    rel = path.resolve().relative_to(VAULT.resolve()).as_posix()
                except ValueError:
                    continue
                if rel in lookup:
                    ident = lookup[rel]
                    break
            if not ident and wiki and "/" not in target:
                matches = stems.get(Path(target).stem.casefold(), [])
                if len(matches) == 1:
                    ident = matches[0]
            if ident and ident != n["id"]:
                source_pairs.add((n["id"], ident))
    links = list(
        dict(source=a, target=b, kind="source") for a, b in sorted(source_pairs)
    )
    for n in notes:
        n["references"] = [b for a, b in source_pairs if a == n["id"]]
        n["featureText"] = DISCOVERY.features(n)[1]
    return links


def _graph_group_nodes(subjects, notes, links):
    nodes = []
    by_id = {note["id"]: note for note in notes}
    for subject in subjects:
        members = [by_id[ident] for ident in subject["members"]]
        display_category = (
            subject["category"]
            if subject["category"] in THEMES
            else category(subject["label"], " ".join(n["label"] for n in members))[0]
        )
        days = {}
        unique = set()
        for n in members:
            signature = hashlib.sha256(
                DISCOVERY.clean(n["text"]).casefold().encode()
            ).hexdigest()
            if n.get("recorded") and signature not in unique:
                days[n["date"]] = days.get(n["date"], 0) + n["activity"]
                unique.add(signature)
        activity = sum(math.log1p(value) for value in days.values())
        nodes.append(
            dict(
                id=subject["id"],
                label=subject["label"],
                kind="hub",
                count=len(members),
                activity=round(activity, 3),
                radius=20 + 8 * math.log1p(activity),
                category=display_category,
                color=THEMES[display_category][1],
                themeLabel=THEMES[display_category][0],
                themeReason="declared" if subject["category"] else "inferred",
                origin=subject["origin"],
                parent=subject["parent"],
                summary="Grouped from saved knowledge; automatic suggestions are not verified facts.",
            )
        )
        topic_covered = set()
        for topic in subject["topics"]:
            topic_category = (
                topic["category"]
                if topic["category"] in THEMES
                else category(
                    topic["label"],
                    " ".join(by_id[i]["label"] for i in topic["members"]),
                )[0]
            )
            nodes.append(
                dict(
                    id=topic["id"],
                    label=topic["label"],
                    kind="topic",
                    parent=subject["id"],
                    count=len(topic["members"]),
                    activity=0,
                    radius=12,
                    category=topic_category,
                    color=THEMES[topic_category][1],
                    groupReason=topic["origin"],
                    summary="Topics emerge from annotations and coherent note communities.",
                )
            )
            links.append(
                dict(
                    source=subject["id"],
                    target=topic["id"],
                    kind="group",
                    reason=topic["origin"],
                )
            )
            for ident in topic["members"]:
                links.append(
                    dict(
                        source=topic["id"],
                        target=ident,
                        kind="group",
                        reason=topic["origin"],
                    )
                )
                topic_covered.add(ident)
        for ident, assignment in subject["members"].items():
            if ident not in topic_covered:
                links.append(
                    dict(
                        source=subject["id"],
                        target=ident,
                        kind="group",
                        **assignment,
                    )
                )
        if subject["parent"]:
            links.append(
                dict(
                    source=subject["parent"],
                    target=subject["id"],
                    kind="group",
                    reason="promoted topic",
                )
            )
    return nodes


def graph():
    """Derived topic grouping, explicit source links, dated-note activity proxy.

    Never counts file mtimes or claims to read private chat transcripts. Shared
    evidence appears once and may relate to more than one hub.
    """
    today = dt.datetime.now(dt.timezone.utc).date()
    notes, lookup = _graph_notes(today)
    links = _graph_source_links(notes, lookup)
    derived = VAULT / ".brain-state/discovery"
    try:
        overrides = (
            _state.read_json_file(derived / "overrides.json")
            if (derived / "overrides.json").exists()
            else {}
        )
    except (OSError, ValueError):
        overrides = {}
    fingerprint = (
        hashlib.sha256(
            json.dumps([(n["id"], n["revision"]) for n in notes]).encode()
        ).hexdigest()
        + str(today)
        + hashlib.sha256(json.dumps(overrides, sort_keys=True).encode()).hexdigest()
    )
    with GRAPH_LOCK:
        cache_key = str(VAULT.resolve())
        if GRAPH_CACHE.get("key") == (cache_key, fingerprint):
            cached = GRAPH_CACHE["data"].copy()
            cached.update(
                theme=desktop_theme(),
                accent=palette(),
                generated=dt.datetime.now().isoformat(timespec="seconds"),
            )
            return cached
        try:
            prior = (
                _state.read_json_file(derived / "subjects.json")
                if (derived / "subjects.json").exists()
                else {}
            )
        except (OSError, ValueError):
            prior = {}
        dense = SEMANTICS.vectors(notes, derived / "vectors.json")
        subjects, report = DISCOVERY.organize(notes, dense, prior, overrides)
        derived.mkdir(parents=True, exist_ok=True, mode=0o700)
        tmp = derived / "subjects.next"
        tmp.write_text(json.dumps(report))
        tmp.chmod(0o600)
        try:
            _state.write(derived / "subjects.json", report)
        finally:
            tmp.unlink(missing_ok=True)
        nodes = _graph_group_nodes(subjects, notes, links)
        nodes.extend(
            {
                k: v
                for k, v in n.items()
                if k not in ("text", "meta", "references", "featureText")
            }
            for n in notes
        )
        result = dict(
            nodes=nodes,
            links=links,
            noteCount=len(notes),
            theme=desktop_theme(),
            accent=palette(),
            generated=dt.datetime.now().isoformat(timespec="seconds"),
            activityModel="Meaningful saved evidence, 21-day half-life, deduplicated captures and diminishing daily returns. Raw message counting is not enabled.",
            discovery={**report, "embedding": SEMANTICS.health},
        )
        GRAPH_CACHE.update(key=(cache_key, fingerprint), data=result)
        return result


def search_notes(query):
    words = str(query)[:200].casefold().split()
    if not words:
        return []
    result = []
    for p in VAULT.rglob("*.md"):
        rel = p.relative_to(VAULT).as_posix()
        if (
            not p.resolve().is_relative_to(VAULT.resolve())
            or any(x.startswith(".") for x in Path(rel).parts)
            or rel in ("AGENTS.md", "CLAUDE.md", "README.md", "INDEX.md")
        ):
            continue
        content = (rel + "\n" + p.read_text(errors="replace")).casefold()
        if all(word in content for word in words):
            result.append("note:" + hashlib.sha256(rel.encode()).hexdigest()[:16])
    return result


def desktop_theme():
    try:
        return _state.read_json_file(HOME / ".local/state/nacre/scheme.json")["colours"]
    except (OSError, ValueError, KeyError):
        return {}


def palette():
    p = HOME / ".config/nacre/colors/primary"
    try:
        value = p.read_text().strip()
        return value if re.fullmatch(r"#[0-9a-fA-F]{6}", value) else "#808080"
    except OSError:
        return "#808080"


def theme_css():
    """Supply the current palette before the first paint, without indexing notes."""
    colours = desktop_theme()
    if not isinstance(colours, dict):
        return ""
    roles = {
        "bg": "surface",
        "panel": "surfaceContainer",
        "raised": "surfaceContainerHigh",
        "text": "onSurface",
        "muted": "onSurfaceVariant",
        "line": "outlineVariant",
        "accent": "primary",
        "secondary": "secondary",
        "tertiary": "tertiary",
        "on-accent": "onPrimary",
    }
    valid = {
        role: "#" + value.lstrip("#")
        for role, value in colours.items()
        if isinstance(value, str) and re.fullmatch(r"#?[0-9a-fA-F]{6}", value)
    }
    declarations = [
        "--" + name + ":" + valid[role] + ";"
        for name, role in roles.items()
        if role in valid
    ]
    surface = valid.get("surface", "#151310")
    light = sum(int(surface[i : i + 2], 16) for i in (1, 3, 5)) > 420
    declarations.extend(
        (
            "color-scheme:" + ("light" if light else "dark") + ";",
            "--logo-highlight:"
            + valid.get("primary" if light else "primaryFixed", "#eee9f4")
            + ";",
        )
    )
    return "\n:root{" + "".join(declarations) + "}\n"


def action(name, value=""):
    if name == "notes":
        launch_app(["env", "NACRE_BRAIN_VIEW=notes", "nacre-ai", "brain"])
        return
    if name == "brain":
        ensure_brain(focus=True)
        return
    if name in ("tasks", "new", "resume"):
        launch_app([str(HOME / ".local/bin/nacre-shell"), name])
        return
    if name == "assign-note":
        data = json.loads(value)
        p = note_path(data["path"])
        rel = p.relative_to(VAULT.resolve()).as_posix()
        if rel in ("AGENTS.md", "CLAUDE.md", "INDEX.md", "README.md"):
            raise ValueError("Not an indexed note")
        labels = data.get("subjects", [])
        if (
            not isinstance(labels, list)
            or len(labels) > 8
            or any(
                not isinstance(label, str)
                or not 2 <= len(label.strip()) <= 80
                or any(ord(c) < 32 for c in label)
                for label in labels
            )
        ):
            raise ValueError("Invalid subject labels")
        path = VAULT / ".brain-state/discovery/overrides.json"
        path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        with (path.parent / "override.lock").open("w") as guard:
            fcntl.flock(guard, fcntl.LOCK_EX)
            overrides = _state.read_json_file(path) if path.exists() else {}
            ident = "note:" + hashlib.sha256(rel.encode()).hexdigest()[:16]
            if data.get("reset"):
                overrides.pop(ident, None)
            elif labels:
                overrides[ident] = list(
                    dict.fromkeys(label.strip() for label in labels)
                )
            else:
                raise ValueError("Choose at least one subject")
            temp = path.with_suffix(".tmp")
            temp.write_text(json.dumps(overrides))
            temp.chmod(0o600)
            _state.write(path, overrides)
            temp.unlink(missing_ok=True)
        return
    if name == "open-note":
        p = note_path(value)
        launch_app(["obsidian", "obsidian://open?path=" + quote(str(p), safe="")])
        return
    if name == "capture":
        text = run(
            [
                "zenity",
                "--text-info",
                "--editable",
                "--title=Capture a thought",
                "--width=580",
                "--height=340",
            ],
            timeout=3600,
        )
        if text.strip():
            title = text.strip().splitlines()[0][:90]
            subprocess.run(
                [
                    "nacre-brain",
                    "note",
                    "--kind",
                    "personal",
                    "--title",
                    title,
                    "--source",
                    "User quick capture in Nacre Brain",
                    "--confidence",
                    "reported",
                ],
                input=text,
                text=True,
                stdout=subprocess.DEVNULL,
                check=True,
            )
        return
    raise ValueError("Unknown Brain action")


def auth_module():
    spec = importlib.util.spec_from_file_location(
        "brain_browser_auth", ROOT / "auth.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class BrainServer(ThreadingHTTPServer):
    def __init__(self, address, state=STATE):
        super().__init__(address, Handler)
        self.auth = auth_module().BrowserAuth(state, self.server_port)


def browser_command(path, handoff=False):
    browser = next(
        (
            p
            for p in ["/opt/google/chrome/chrome", shutil.which("chromium")]
            if p and Path(p).exists()
        ),
        None,
    )
    if not browser:
        raise ValueError("Chrome or Chromium is required for the knowledge browser")
    return [
        browser,
        "--no-first-run",
        "--no-default-browser-check",
        "--ozone-platform=wayland",
        "--class=" + ("nacre-brain-auth" if handoff else "nacre-brain"),
        "--user-data-dir=" + str(HOME / ".local/share/nacre/brain-browser"),
        "--app=" + Path(path).as_uri(),
    ]


def bootstrap_existing_browser():
    directory = STATE / "auth"
    if not (directory / "session.json").exists():
        return
    # A cookie issued once remains valid across server restarts. Only the initial
    # upgrade of an already-open old browser needs a tiny, auto-closing handoff.
    auth = object.__new__(auth_module().BrowserAuth)
    auth.directory = directory
    auth.cookie = _state.read_json_file(directory / "session.json")["cookie"]
    if not auth.browser_ready():
        launch(browser_command(directory / "handoff.html", True))


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def allowed(self):
        return self.headers.get("Host") in (
            "127.0.0.1:" + str(self.server.server_port),
            "localhost:" + str(self.server.server_port),
        )

    def authenticated(self):
        try:
            cookies = SimpleCookie(self.headers.get("Cookie", ""))
            value = cookies.get("nacre_brain")
            return bool(value and self.server.auth.cookie_valid(value.value))
        except (CookieError, AttributeError):
            return False

    def send(self, body, mime="application/json", code=200):
        self.send_response(code)
        self.send_header("Content-Type", mime)
        self.send_header("Cache-Control", "no-store")
        self.send_header("X-Content-Type-Options", "nosniff")
        self.send_header(
            "Content-Security-Policy",
            "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; object-src 'none'; frame-ancestors 'none'",
        )
        self.end_headers()
        self.wfile.write(body if isinstance(body, bytes) else body.encode())

    def do_GET(self):
        if not self.allowed():
            return self.send("{}", code=403)
        path = urlparse(self.path)
        if path.path == "/" and "token" in parse_qs(path.query):
            query = parse_qs(path.query)
            if not self.server.auth.token_valid(query["token"][0]):
                return self.send("{}", code=403)
            self.server.auth.mark_browser()
            self.send_response(303)
            self.send_header(
                "Set-Cookie",
                "nacre_brain="
                + self.server.auth.cookie
                + "; Path=/; HttpOnly; SameSite=Strict",
            )
            self.send_header(
                "Location", "/auth-complete" if query.get("handoff") == ["1"] else "/"
            )
            self.send_header("Cache-Control", "no-store")
            self.send_header("Referrer-Policy", "no-referrer")
            self.end_headers()
            return
        if (
            path.path.startswith("/api/")
            and path.path != "/api/health"
            and not self.authenticated()
        ):
            return self.send("{}", code=403)
        if path.path == "/auth-complete":
            return self.send(
                '<!doctype html><script src="/auth-close.js"></script>', "text/html"
            )
        if path.path == "/auth-close.js":
            return self.send("window.close();", "text/javascript")
        try:
            if path.path == "/api/brain":
                return self.send(json.dumps(graph()))
            if path.path == "/api/search":
                return self.send(
                    json.dumps(
                        dict(ids=search_notes(parse_qs(path.query).get("q", [""])[0]))
                    )
                )
            if path.path == "/api/navigation":
                nav = HOME / ".local/state/nacre/shell/extras/brain-focus.json"
                value = _state.read_json_file(nav) if nav.exists() else {}
                if time.time() - value.get("created", 0) > 60:
                    value = {}
                return self.send(json.dumps(value))
            if path.path == "/api/note":
                p = note_path(parse_qs(path.query).get("path", [""])[0])
                return self.send(json.dumps(dict(text=p.read_text(errors="replace"))))
            if path.path == "/api/health":
                return self.send('{"ready":true,"authVersion":1}')
            assets = {
                "/": "index.html",
                "/app.js": "app.js",
                "/style.css": "style.css",
                "/i18n.js": "i18n.js",
                "/i18n-data.js": "i18n-data.js",
                "/translation-engine.js": "translation-engine.js",
            }
            if path.path in assets:
                name = assets[path.path]
                content = (ROOT / "web" / name).read_bytes()
                if name == "style.css":
                    content += theme_css().encode()
                if name == "index.html":
                    requested = os.environ.get("NACRE_LANGUAGE", "")
                    if not requested:
                        try:
                            preference = _state.read_json_file(
                                HOME / ".config/nacre/desktop.json"
                            )
                            requested = (
                                preference.get("language", "system")
                                if isinstance(preference, dict)
                                else "system"
                            )
                        except (OSError, ValueError):
                            requested = "system"
                    if not isinstance(requested, str) or not re.fullmatch(
                        r"system|pseudo|[a-z]{2,3}(?:[-_][A-Za-z0-9]+)*", requested
                    ):
                        requested = "system"
                    content = content.replace(
                        b'<html lang="en"',
                        (
                            '<html lang="en" data-nacre-language="' + requested + '"'
                        ).encode(),
                    )
                return self.send(
                    content,
                    {
                        "index.html": "text/html; charset=utf-8",
                        "app.js": "text/javascript",
                        "i18n.js": "text/javascript",
                        "i18n-data.js": "text/javascript",
                        "translation-engine.js": "text/javascript",
                        "style.css": "text/css",
                    }[name],
                )
            self.send("{}", code=404)
        except (ValueError, OSError):
            self.send("{}", code=404)

    def do_POST(self):
        # Same-origin JSON requests only; no cross-origin action endpoints.
        origin = self.headers.get("Origin")
        if (
            not self.allowed()
            or not self.authenticated()
            or origin != "http://127.0.0.1:" + str(self.server.server_port)
            or self.headers.get("Content-Type") != "application/json"
        ):
            return self.send("{}", code=403)
        if self.path != "/api/action":
            return self.send("{}", code=404)
        try:
            size = int(self.headers.get("Content-Length", "0"))
            if not 0 < size <= 4096:
                raise ValueError("Invalid length")
            data = json.loads(self.rfile.read(size))
            # Web interface only exposes its own narrow navigation actions.
            if data.get("name") not in (
                "open-note",
                "capture",
                "tasks",
                "new",
                "resume",
                "notes",
                "assign-note",
            ):
                raise ValueError("Invalid action")
            if data["name"] == "assign-note":
                action("assign-note", str(data.get("value", "")))
            else:
                launch_app(
                    [
                        sys.executable,
                        str(ROOT / "control.py"),
                        "action",
                        data["name"],
                        str(data.get("value", "")),
                    ]
                )
            self.send('{"ok":true}')
        except (ValueError, KeyError):
            self.send("{}", code=400)


def ensure_server():
    import urllib.request

    url = "http://127.0.0.1:" + str(PORT) + "/api/health"
    try:
        with urllib.request.urlopen(url, timeout=1) as r:
            if json.load(r).get("ready"):
                return
    except (OSError, ValueError):
        pass
    launch([sys.executable, str(ROOT / "control.py"), "serve"])
    for _ in range(30):
        time.sleep(0.1)
        try:
            with urllib.request.urlopen(url, timeout=1) as r:
                if json.load(r).get("ready"):
                    return
        except (OSError, ValueError):
            pass
    raise ValueError("Nacre Brain server did not start")


def browser_profile_pids():
    profiles = {
        ("--user-data-dir=" + str(HOME / path)).encode()
        for path in (".local/share/nacre/brain-browser",)
    }
    profile_pids = set()
    for process in Path("/proc").iterdir():
        if not process.name.isdigit():
            continue
        try:
            if profiles.intersection((process / "cmdline").read_bytes().split(b"\0")):
                profile_pids.add(int(process.name))
        except OSError:
            pass
    return profile_pids


def brain_window():
    # A failed query is not evidence that the window is absent. In particular,
    # resume/startup may briefly make the compositor unavailable.
    try:
        clients = compositor().query("windows", timeout=3)
    except (OSError, ValueError, subprocess.SubprocessError) as exc:
        raise RuntimeError("Cannot query Brain windows yet") from exc
    if not isinstance(clients, list):
        raise RuntimeError("Invalid compositor window list")
    profile_pids = browser_profile_pids()
    windows = [
        c
        for c in clients
        if c.get("class") == "nacre-brain"
        or (
            c.get("pid") in profile_pids
            and (
                c.get("class") == "chrome-127.0.0.1__-Default"
                or any(
                    fragment in c.get("class", "")
                    for fragment in ("nacre_brain_auth_open.html",)
                )
            )
        )
    ]
    # Chrome's initial title is its URL, not the final page title. Prefer the
    # existing workspace-six instance when old duplicates are still present.
    return next(
        (
            c
            for c in windows
            if preferred_workspace() is not None
            and c.get("workspace", {}).get("id") == preferred_workspace()
        ),
        next(iter(windows), None),
    )


def brain_browser_running():
    # Chrome can be alive without a mapped window while the keyring dialog
    # blocks startup. Do not keep asking that same profile for another app.
    profiles = {
        ("--user-data-dir=" + str(HOME / path)).encode()
        for path in (".local/share/nacre/brain-browser",)
    }
    for process in Path("/proc").iterdir():
        if not process.name.isdigit():
            continue
        try:
            if process.stat().st_uid != os.getuid():
                continue
            args = (process / "cmdline").read_bytes().split(b"\0")
        except OSError:
            continue
        if profiles.intersection(args) and not any(
            arg.startswith(b"--type=") for arg in args
        ):
            return True
    return False


def preferred_workspace():
    try:
        value = _state.read_json_file(HOME / ".config/nacre/profile.json").get(
            "brainWorkspace"
        )
        return value if type(value) is int and 1 <= value <= 10 else None
    except (OSError, ValueError):
        return None


def ensure_brain(focus=False):
    target_workspace = preferred_workspace()
    STATE.mkdir(parents=True, exist_ok=True, mode=0o700)
    with (STATE / "brain-launch.lock").open("w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        ensure_server()
        window = brain_window()
        if not window:
            if not focus and brain_browser_running():
                return
            try:
                previous = compositor().query("active_workspace", timeout=3).get("id")
            except (OSError, subprocess.SubprocessError):
                previous = None
            launch(browser_command(STATE / "auth/open.html"))
            for _ in range(100):
                window = brain_window()
                if window:
                    break
                time.sleep(0.1)
            if window and compositor().valid_window_id(window["address"]):
                if compositor().capabilities.get("fullscreenState", True):
                    compositor_action(
                        "fullscreen_state", window["address"], internal=0, client=0
                    )
                if target_workspace is not None:
                    compositor_action(
                        "move_window",
                        window["address"],
                        workspace=str(target_workspace),
                        follow=False,
                    )
            if not focus and isinstance(previous, int) and previous > 0:
                compositor_action("switch_workspace", previous, current_monitor=True)
        if window:
            bootstrap_existing_browser()
        if focus:
            if target_workspace is not None:
                run(
                    [
                        str(HOME / ".local/bin/nacre-shell"),
                        "workspace",
                        str(target_workspace),
                    ]
                )
            if window:
                run(
                    [
                        str(HOME / ".local/bin/nacre-shell"),
                        "focus",
                        window["address"],
                    ]
                )


def watch_brain():
    STATE.mkdir(parents=True, exist_ok=True, mode=0o700)
    with (STATE / "brain-watch.lock").open("w") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            return
        while True:
            # Keep the local server ready; opening a window requires an explicit
            # launcher action. Closing the browser is not a recovery condition.
            try:
                ensure_server()
            except (OSError, ValueError, RuntimeError, subprocess.SubprocessError):
                pass  # Retry server startup next poll, without opening a window.
            time.sleep(3)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=["serve", "action", "graph", "watch-brain"])
    parser.add_argument("name", nargs="?")
    parser.add_argument("value", nargs="?", default="")
    args = parser.parse_args()
    if args.command == "serve":
        server = BrainServer(("127.0.0.1", PORT))

        def migrate_browser():
            time.sleep(1)
            try:
                if brain_window():
                    bootstrap_existing_browser()
            except (OSError, ValueError, RuntimeError):
                pass

        threading.Thread(target=migrate_browser, daemon=True).start()
        server.serve_forever()
    elif args.command == "watch-brain":
        watch_brain()
    elif args.command == "action":
        action(args.name, args.value)
    elif args.command == "graph":
        print(json.dumps(graph()))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.SubprocessError) as e:
        print("Observatory: " + str(e), file=sys.stderr)
        sys.exit(1)
