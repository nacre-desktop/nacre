"""Evidence-led subject discovery. No project registry, fixed subjects or folder taxonomy."""

import collections
import hashlib
import math
import re
import unicodedata

STOP = set(
    "a an the and or but of to in on for with from by at as is are was were be been being this that these those it its my your our their we you they i me he she them have has had do does did can could should would will may into about after before through more most some any all each only other no not new now then than also just how what when where which who why use used using make made first last one two current latest source recorded reviewed confidence entity name parent project worlds topics tags category aliases status evidence note notes knowledge verified reported unverified sourcepage test tests passed check checks result results update updated updating completed complete ready fixed fix added add deployed deploy deployment implement implementation source code files file path paths branch worktree commit git user assistant workflow context saved private local scope preserve existing unchanged previous version changes changed change correct documented requested default actual explicit native shell runtime process processes window windows gui ui cli service services script scripts command commands configuration config settings clean done shows show setup verified validation milestone historical observations observation captured capture memory suggestions suggested subject subjects".split()
)


def key(value):
    return unicodedata.normalize("NFKC", str(value)).casefold().strip()


def slug(value):
    value = re.sub(
        r"[^a-z0-9]+",
        "-",
        unicodedata.normalize("NFKD", str(value))
        .encode("ascii", "ignore")
        .decode()
        .lower(),
    ).strip("-")[:80]
    return value or "subject-" + hashlib.sha256(key(value).encode()).hexdigest()[:12]


def items(value):
    return [
        v.strip().strip("\"'") for v in str(value).strip("[]").split(",") if v.strip()
    ]


def clean(text):
    text = re.sub(r"(?ms)^```[^\n]*\n.*?^```[^\n]*$", " ", text)
    text = re.sub(r"`[^`]+`", " ", text)
    text = re.sub(
        r"(?mi)^(Source|Recorded|Reviewed|Confidence|Entity|Name|Parent|Project|Worlds|Topics|Tags|Category|Aliases|Status):[^\n]*",
        " ",
        text,
    )
    text = re.sub(
        r"https?://\S+|(?:~|/)[\w./%-]+|\b[0-9a-f]{12,}\b", " ", text, flags=re.I
    )
    return text


def tokens(text):
    return [
        w.casefold()
        for w in re.findall(r"[^\W\d_]{3,}", text)
        if w.casefold() not in STOP
    ]


def features(note):
    body = clean(note["text"])
    title = note["label"]
    counts = collections.Counter(tokens(title) * 3 + tokens(body[:5000]))
    return counts, title + "\n" + body[:1100] + "\n" + body[-350:]


def normalized(vector):
    norm = math.sqrt(sum(v * v for v in vector.values()))
    return {k: v / norm for k, v in vector.items()} if norm else {}


def lexical(notes):
    counts = {n["id"]: features(n)[0] for n in notes}
    df = collections.Counter(w for c in counts.values() for w in c)
    return {
        ident: normalized(
            {
                w: (1 + math.log(tf)) * math.log(1 + len(notes) / (1 + df[w]))
                for w, tf in count.items()
            }
        )
        for ident, count in counts.items()
    }


def cosine(a, b):
    return sum(value * b.get(term, 0) for term, value in a.items())


def dense_cos(a, b):
    return sum(x * y for x, y in zip(a, b, strict=True)) if a and b else 0


def centroid(ids, vectors):
    result = collections.Counter()
    for ident in ids:
        result.update(vectors.get(ident, {}))
    return normalized(result)


def dense_centroid(ids, vectors):
    values = [vectors[i] for i in ids if i in vectors]
    if not values:
        return []
    result = [sum(v[j] for v in values) / len(values) for j in range(len(values[0]))]
    norm = math.sqrt(sum(v * v for v in result))
    return [v / norm for v in result] if norm else []


def similarity(a, b, lex, dense):
    sparse = cosine(lex.get(a, {}), lex.get(b, {}))
    semantic = dense_cos(dense.get(a, []), dense.get(b, []))
    return 0.65 * semantic + 0.35 * sparse if a in dense and b in dense else sparse


def label_cluster(ids, notes, lex):
    scores = collections.Counter()
    phrases = collections.defaultdict(set)
    spellings = {}
    for ident in ids:
        n = notes[ident]
        scores.update(lex.get(ident, {}))
        words = re.findall(r"[^\W\d_]{3,}", n["label"])
        for word in words:
            spellings.setdefault(key(word), word)
        for size in (2, 3):
            for i in range(len(words) - size + 1):
                phrase = " ".join(words[i : i + size])
                if all(key(w) not in STOP for w in words[i : i + size]):
                    phrases[phrase].add(ident)
    best = max(scores, key=scores.get, default="Unsorted knowledge")
    options = [
        (
            sum(scores[key(w)] for w in phrase.split()) / len(phrase.split()) * 1.2,
            phrase,
        )
        for phrase, members in phrases.items()
        if len(members) >= 2 and best in tokens(phrase)
    ]
    if options:
        return max(options)[1]
    word = spellings.get(best, best)
    return word if any(c.isupper() for c in word[1:]) else word.capitalize()


def communities(ids, lex, dense, minimum=3):
    # Average-link merging plus a weakest-pair guard prevents one bridging note joining unrelated groups.
    groups = [{i} for i in sorted(ids)]
    pairs = {}

    def sim(a, b):
        k = tuple(sorted((a, b)))
        if k not in pairs:
            pairs[k] = similarity(a, b, lex, dense)
        return pairs[k]

    threshold = 0.53 if dense else 0.32
    while True:
        best = None
        for i, a in enumerate(groups):
            for j in range(i + 1, len(groups)):
                values = [sim(x, y) for x in a for y in groups[j]]
                score = sum(values) / len(values)
                if (
                    score >= threshold
                    and min(values) >= threshold * 0.65
                    and (best is None or score > best[0])
                ):
                    best = (score, i, j)
        if best is None:
            break
        _, i, j = best
        groups[i] |= groups.pop(j)
    return [sorted(g) for g in groups if len(g) >= minimum]


def organize(notes, dense=None, prior=None, overrides=None):
    """Build grounded memberships through explicit, ordered discovery stages."""
    return _Discovery(notes, dense, prior, overrides).run()


class _Discovery:
    """Per-query state; no shared cache or mutation outside the supplied notes."""

    def __init__(self, notes, dense, prior, overrides):
        self.notes = notes
        self.dense = dense or {}
        self.prior = prior or {}
        self.overrides = overrides or {}
        self.by = {n["id"]: n for n in notes}
        self.lex = lexical(notes)
        self.subjects = {}
        self.aliases = {}
        self.topic_specs = []
        self.assigned = collections.defaultdict(dict)

    def add(
        self, label, ident=None, alias=(), origin="annotated", category=None, path=None
    ):
        existing = self.aliases.get(key(label))
        if existing:
            return self.subjects[existing]
        ident = ident or next(
            (
                v["id"]
                for v in self.prior.get("discovered", [])
                if key(v["label"]) == key(label)
            ),
            slug(label),
        )
        if ident in self.subjects and key(self.subjects[ident]["label"]) != key(label):
            ident += "-" + hashlib.sha256(label.encode()).hexdigest()[:6]
        s = self.subjects.setdefault(
            ident,
            dict(
                id=ident,
                label=label,
                aliases=[],
                origin=origin,
                category=category,
                path=path,
                members={},
                topics=[],
                parent=None,
            ),
        )
        for name in [label, ident, *alias]:
            if name and key(name) not in self.aliases:
                self.aliases[key(name)] = ident
                s["aliases"].append(name)
        return s

    def resolve(self, value):
        return self.aliases.get(key(value)) or self.aliases.get(key(slug(value)))

    def attach(self, note, subject, reason, score=1):
        if note in self.overrides and reason != "manual grouping":
            return
        if subject in self.subjects:
            self.assigned[note][subject] = dict(reason=reason, score=round(score, 3))
            self.subjects[subject]["members"][note] = self.assigned[note][subject]

    def run(self):
        self.seed_annotations()
        self.apply_overrides()
        self.attach_curated_links()
        self.attach_named_evidence()
        self.discover_named_subjects()
        self.assign_profiles()
        self.discover_residual()
        self.build_topics()
        self.merge_topics()
        self.attach_topics()
        self.promote_topics()
        return self.finish()

    def seed_annotations(self):
        # Curated declarations and explicit annotations are authority; raw imports cannot declare entities.
        for n in self.notes:
            m = n["meta"]
            kind = m.get("entity", "")
            if (
                n["path"].startswith("wiki/")
                and (
                    kind in ("world", "project", "person", "interest", "idea", "device")
                    or not kind
                    and len(n["label"].split()) <= 5
                )
                and not m.get("parent")
            ):
                s = self.add(
                    m.get("name") or n["label"],
                    m.get("project")
                    or slug(n["path"].split("/")[-1].removesuffix(".md")),
                    items(m.get("aliases", ""))
                    + [n["path"].split("/")[-1].removesuffix(".md")],
                    category=m.get("category"),
                    path=n["path"],
                )
                self.attach(n["id"], s["id"], "curated")
        for n in self.notes:
            if n["path"].startswith("raw/"):
                continue
            m = n["meta"]
            for label in items(m.get("worlds", "")):
                s = self.add(label)
                self.attach(n["id"], s["id"], "annotation")
            if m.get("project") and m.get("entity") not in ("topic", "device"):
                ident = self.resolve(m["project"])
                s = (
                    self.subjects[ident]
                    if ident
                    else self.add(
                        m["project"].replace("-", " ").title(), slug(m["project"])
                    )
                )
                self.attach(n["id"], s["id"], "annotation")

    def apply_overrides(self):
        # A user's grouping correction overrides historical annotations and becomes a strong anchor.
        for n in self.notes:
            if n["id"] not in self.overrides:
                continue
            for subject in self.subjects.values():
                subject["members"].pop(n["id"], None)
            self.assigned[n["id"]] = {}
            for label in self.overrides[n["id"]]:
                s = self.add(label, origin="manual")
                self.attach(n["id"], s["id"], "manual grouping")

    def attach_curated_links(self):
        # Explicit curated evidence links give an anchor to legacy evidence without keyword shortcuts.
        paths = {n["path"]: n["id"] for n in self.notes}
        for s in list(self.subjects.values()):
            if not s["path"]:
                continue
            page = self.by[paths[s["path"]]]
            for target in page.get("references", []):
                if target not in self.overrides:
                    self.attach(target, s["id"], "curated reference")
        for n in self.notes:
            m = n["meta"]
            if n["path"].startswith("wiki/") and m.get("entity") in ("topic", "device"):
                parent = self.resolve(m.get("parent") or m.get("project", ""))
                if parent:
                    label = m.get("name") or n["label"]
                    self.topic_specs.append(
                        dict(
                            id=parent
                            + ":entity-"
                            + slug(n["path"].split("/")[-1].removesuffix(".md")),
                            label=label,
                            parent=parent,
                            category=m.get("category"),
                            members={n["id"]},
                            origin="annotated",
                        )
                    )
                    self.attach(n["id"], parent, "curated topic")

    def attach_named_evidence(self):
        # Exact named mentions in titles, longest first; ambiguous namespace prefixes are not personal matches.
        self.names = sorted(
            [
                (name, s["id"])
                for s in self.subjects.values()
                for name in s["aliases"]
                if len(name) >= 4
            ],
            key=lambda v: -len(v[0]),
        )
        for n in self.notes:
            if self.assigned[n["id"]]:
                continue
            title = key(n["label"])
            found = []
            for name, ident in self.names:
                ambiguous = any(
                    key(other["label"]).startswith(key(name) + " ")
                    and other["id"] != ident
                    for other in self.subjects.values()
                )
                if ambiguous and title != key(name):
                    continue
                if re.search(
                    r"(?<!\w)" + re.escape(key(name)) + r"(?!\w)", title
                ) and not any(key(name) in key(longer) for longer, _ in found):
                    found.append((name, ident))
            for _, ident in found[:2]:
                self.attach(n["id"], ident, "named title")
        for n in self.notes:
            if self.assigned[n["id"]]:
                continue
            intro = key(clean(n["text"])[:900])
            for name, ident in self.names:
                if any(
                    key(other["label"]).startswith(key(name) + " ")
                    and other["id"] != ident
                    for other in self.subjects.values()
                ):
                    continue
                match = re.search(r"(?<!\w)" + re.escape(key(name)) + r"(?!\w)", intro)
                if not match:
                    continue
                before = intro[max(0, match.start() - 45) : match.start()]
                if re.search(
                    r"\b(unrelated|unchanged|preserve|without|no|not|example|fixture)\b",
                    before,
                ):
                    continue
                self.attach(n["id"], ident, "named evidence")
                if len(self.assigned[n["id"]]) >= 2:
                    break

    def discover_named_subjects(self):
        # A repeated specific new name is an identity signal, not just another note in a familiar domain.
        # Detect it before prototype assignment so a new electronics project is not swallowed by an old one.
        novel = collections.defaultdict(set)
        for n in self.notes:
            if self.assigned[n["id"]]:
                continue
            for name in re.findall(
                r"\b(?:[A-Z][a-z]+[A-Z][A-Za-z]*|[A-Z]{4,})\b", n["label"]
            ):
                if not self.resolve(name):
                    novel[name].add(n["id"])
        for label, ids in sorted(novel.items(), key=lambda p: (-len(p[1]), p[0])):
            ids = [i for i in sorted(ids) if not self.assigned[i]]
            evidence = {
                hashlib.sha256(
                    (self.by[i]["label"] + "\n" + clean(self.by[i]["text"]))
                    .casefold()
                    .encode()
                ).hexdigest()
                for i in ids
            }
            if len(evidence) < 3:
                continue
            pairs = [
                similarity(a, b, self.lex, self.dense)
                for a in ids
                for b in ids
                if a < b
            ]
            if sum(pairs) / max(1, len(pairs)) < (0.35 if self.dense else 0.18):
                continue
            s = self.add(label, "discovered-" + slug(label), origin="discovered")
            for ident in ids:
                self.attach(ident, s["id"], "repeated named subject", 0.75)

    def assign_profiles(self):
        # Profiles learn only from grounded anchors, then generalize by local semantics + discriminative terms.
        self.profiles = {
            i: (
                centroid(s["members"], self.lex),
                dense_centroid(s["members"], self.dense),
            )
            for i, s in self.subjects.items()
            if s["members"]
        }
        for n in self.notes:
            if self.assigned[n["id"]]:
                continue
            scores = []
            for ident, (sparse, semantic) in self.profiles.items():
                score = cosine(self.lex[n["id"]], sparse)
                if n["id"] in self.dense and semantic:
                    score = (
                        0.65 * dense_cos(self.dense[n["id"]], semantic) + 0.35 * score
                    )
                scores.append((score, ident))
            scores.sort(reverse=True)
            if scores and scores[0][0] >= (0.47 if self.dense else 0.24):
                if (
                    len(scores) > 1
                    and scores[1][0] >= scores[0][0] * 0.91
                    and scores[0][0] - scores[1][0] < 0.055
                ):
                    n["suggestions"] = [
                        dict(subject=i, score=round(v, 3)) for v, i in scores[:2]
                    ]
                    continue
                self.attach(n["id"], scores[0][1], "learned similarity", scores[0][0])

    def discover_residual(self):
        # Residual coherent evidence becomes a subject without needing a registry or descriptor page.
        residual = [
            n["id"]
            for n in self.notes
            if not self.assigned[n["id"]] and not n["path"].startswith("raw/")
        ]
        self.discovered = [
            dict(id=s["id"], label=s["label"], members=sorted(s["members"]))
            for s in self.subjects.values()
            if s["origin"] == "discovered"
        ]
        named = collections.defaultdict(set)
        for ident in residual:
            for word in re.findall(r"\b[A-Z][A-Za-z]{3,}\b", self.by[ident]["label"]):
                if key(word) not in STOP:
                    named[word].add(ident)
        named_groups = []
        for label, members in sorted(
            named.items(), key=lambda pair: (-len(pair[1]), pair[0])
        ):
            if len(members) < 3:
                continue
            pairs = [
                similarity(a, b, self.lex, self.dense)
                for a in members
                for b in members
                if a < b
            ]
            if sum(pairs) / max(1, len(pairs)) < (0.35 if self.dense else 0.18):
                continue
            group = [i for i in sorted(members) if not self.assigned[i]]
            if (
                len(group) < 3
                or len(
                    {
                        hashlib.sha256(
                            (self.by[i]["label"] + "\n" + clean(self.by[i]["text"]))
                            .casefold()
                            .encode()
                        ).hexdigest()
                        for i in group
                    }
                )
                < 3
            ):
                continue
            named_groups.append((label, group))
        for label, group in named_groups:
            label = (
                label_cluster(group, self.by, self.lex)
                if not (label.isupper() or any(c.isupper() for c in label[1:]))
                else label
            )
            label = label[0].upper() + label[1:]
            related = []
            for ident, (sparse, semantic) in self.profiles.items():
                score = cosine(centroid(group, self.lex), sparse)
                if self.dense and semantic:
                    score = (
                        0.65 * dense_cos(dense_centroid(group, self.dense), semantic)
                        + 0.35 * score
                    )
                related.append((score, ident))
            related.sort(reverse=True)
            if related and related[0][0] >= (0.4 if self.dense else 0.2):
                parent = related[0][1]
                for ident in group:
                    self.attach(ident, parent, "suggested named topic", related[0][0])
                self.topic_specs.append(
                    dict(
                        id=parent + ":learned-" + slug(label),
                        label=label,
                        parent=parent,
                        category=None,
                        members=set(group),
                        origin="suggested",
                    )
                )
                continue
            s = self.add(label, "discovered-" + slug(label), origin="discovered")
            for ident in group:
                self.attach(ident, s["id"], "repeated named subject", 0.7)
            self.discovered.append(dict(id=s["id"], label=label, members=group))
        residual = [i for i in residual if not self.assigned[i]]
        for group in communities(residual, self.lex, self.dense):
            evidence = {
                hashlib.sha256(
                    (n["label"] + "\n" + clean(n["text"])).casefold().encode()
                ).hexdigest()
                for n in (self.by[i] for i in group)
            }
            if len(evidence) < 3:
                continue
            label = label_cluster(group, self.by, self.lex)
            label = label[0].upper() + label[1:]
            related = []
            for ident, (sparse, semantic) in self.profiles.items():
                sparse_score = cosine(centroid(group, self.lex), sparse)
                score = (
                    0.65 * dense_cos(dense_centroid(group, self.dense), semantic)
                    + 0.35 * sparse_score
                    if self.dense and semantic
                    else sparse_score
                )
                related.append((score, ident))
            related.sort(reverse=True)
            if (
                len(evidence) < 5
                and related
                and related[0][0] >= (0.4 if self.dense else 0.2)
            ):
                parent = related[0][1]
                for note in group:
                    self.attach(note, parent, "suggested community", related[0][0])
                self.topic_specs.append(
                    dict(
                        id=parent + ":learned-" + slug(label),
                        label=label,
                        parent=parent,
                        category=None,
                        members=set(group),
                        origin="suggested",
                    )
                )
                continue
            if len(evidence) < 5:
                continue
            old = None
            overlap = 0
            for candidate in self.prior.get("discovered", []):
                score = len(set(group) & set(candidate["members"])) / max(
                    1, min(len(group), len(candidate["members"]))
                )
                if score > 0.55 and score > overlap:
                    old = candidate
                    overlap = score
            ident = old["id"] if old else "discovered-" + slug(label)
            label = old["label"] if old else label
            s = self.add(label, ident, origin="discovered")
            for note in group:
                self.attach(note, s["id"], "discovered community", 0.65)
            self.discovered.append(dict(id=s["id"], label=s["label"], members=group))

    def build_topics(self):
        # Topic annotations are exact membership, not all notes mentioning a generic word.
        for s in list(self.subjects.values()):
            tags = collections.defaultdict(set)
            for ident in s["members"]:
                for label in items(
                    self.by[ident]["meta"].get(
                        "topics", self.by[ident]["meta"].get("tags", "")
                    )
                ):
                    tags[label].add(ident)
            for label, members in tags.items():
                self.topic_specs.append(
                    dict(
                        id=s["id"] + ":tag-" + slug(label),
                        label=label,
                        parent=s["id"],
                        members=members,
                        origin="annotated",
                        category=None,
                    )
                )
            covered = (
                set().union(
                    *(t["members"] for t in self.topic_specs if t["parent"] == s["id"])
                )
                if any(t["parent"] == s["id"] for t in self.topic_specs)
                else set()
            )
            for group in communities(
                [i for i in s["members"] if i not in covered], self.lex, self.dense
            ):
                label = label_cluster(group, self.by, self.lex)
                if key(label) == key(s["label"]):
                    continue
                self.topic_specs.append(
                    dict(
                        id=s["id"] + ":learned-" + slug(label),
                        label=label,
                        parent=s["id"],
                        members=set(group),
                        origin="suggested",
                        category=None,
                    )
                )

    def merge_topics(self):
        # Combine duplicate representations of the same topic; curated/annotated names win.
        unique = {}
        for topic in self.topic_specs:
            identity = (topic["parent"], key(topic["label"]))
            if identity not in unique:
                unique[identity] = topic
            else:
                old = unique[identity]
                members = old["members"] | topic["members"]
                if topic["origin"] == "annotated" and old["origin"] != "annotated":
                    unique[identity] = topic
                unique[identity]["members"] = members
        self.topic_specs = list(unique.values())

    def attach_topics(self):
        # Grounded topic pages can associate old notes through a profile, with a visible suggested origin.
        for topic in self.topic_specs:
            for n in self.notes:
                if (
                    topic["parent"] not in self.assigned[n["id"]]
                    or n["id"] in topic["members"]
                ):
                    continue
                if re.search(
                    r"(?<!\w)" + re.escape(key(topic["label"])) + r"(?!\w)",
                    key(n["label"]),
                ):
                    topic["members"].add(n["id"])
            self.subjects[topic["parent"]]["topics"].append(topic)

    def promote_topics(self):
        # Major promotion is support + diversity + relative focus, never one mention or one copied template.
        self.promotions = []
        for topic in self.topic_specs:
            group = [self.by[i] for i in topic["members"] if self.by[i].get("recorded")]
            days = {n["date"] for n in group}
            parent = self.subjects[topic["parent"]]
            support = {
                hashlib.sha256(clean(n["text"]).casefold().encode()).hexdigest()
                for n in group
            }
            total = sum(
                self.by[i]["activity"]
                for i in parent["members"]
                if self.by[i].get("recorded")
            )
            weight = sum(n["activity"] for n in group)
            if (
                (
                    len(support) >= 5
                    and len(days) >= 2
                    and weight / max(total, 0.001) >= 0.35
                )
                or (
                    "subject-" + slug(topic["label"]) in self.prior.get("promoted", [])
                    and len(support) >= 3
                )
            ) and not self.resolve(topic["label"]):
                major = self.add(
                    topic["label"], "subject-" + slug(topic["label"]), origin="promoted"
                )
                major["parent"] = parent["id"]
                for n in group:
                    self.attach(n["id"], major["id"], "promoted topic")
                self.promotions.append(major["id"])

    def finish(self):
        unresolved = [n["id"] for n in self.notes if not self.assigned[n["id"]]]
        if unresolved:
            s = self.add("Needs grouping", "review", origin="review")
            for ident in unresolved:
                self.attach(ident, s["id"], "unresolved", 0)
        for n in self.notes:
            n["memberships"] = [
                dict(subject=i, **reason)
                for i, reason in self.assigned[n["id"]].items()
            ]
        return [s for s in self.subjects.values() if s["members"]], dict(
            discovered=self.discovered,
            promoted=self.promotions,
            review=len(unresolved),
            method="semantic + lexical" if self.dense else "lexical fallback",
        )
