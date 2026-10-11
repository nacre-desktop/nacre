.pragma library

const groups = [
    {
        id: "favorites",
        label: "Favorites",
        icon: "favorite"
    },
    {
        id: "actions",
        label: "Actions",
        icon: "bolt"
    },
    {
        id: "all",
        label: "All apps",
        icon: "apps"
    },
    {
        id: "development",
        label: "Development",
        icon: "code",
        categories: ["Development"]
    },
    {
        id: "media",
        label: "Media",
        icon: "movie",
        categories: ["AudioVideo", "Audio", "Video"]
    },
    {
        id: "games",
        label: "Games",
        icon: "sports_esports",
        categories: ["Game"]
    },
    {
        id: "graphics",
        label: "Graphics",
        icon: "palette",
        categories: ["Graphics"]
    },
    {
        id: "internet",
        label: "Internet",
        icon: "public",
        categories: ["Network"]
    },
    {
        id: "office",
        label: "Office",
        icon: "description",
        categories: ["Office"]
    },
    {
        id: "education",
        label: "Education",
        icon: "school",
        categories: ["Education", "Science"]
    },
    {
        id: "system",
        label: "System",
        icon: "settings",
        categories: ["System", "Settings"]
    },
    {
        id: "utilities",
        label: "Utilities",
        icon: "build",
        categories: ["Utility"]
    }
];
function belongs(app, group) {
    const rule = groups.find(item => item.id === group);
    return !!rule && (rule.categories || []).some(category => (app.categories || []).includes(category));
}
function available(apps) {
    return groups.filter(group => !group.categories || apps.some(app => belongs(app, group.id)));
}
function words(value) {
    return String(value || "").toLowerCase().replace(/[-_:]/g, " ").trim().split(/\s+/).filter(Boolean);
}
function matches(item, query) {
    const haystack = words([item.name, item.description, item.action, item.value, ...(item.aliases || [])].join(" ")).join(" ");
    return words(query).every(word => haystack.includes(word));
}
function alphabetical(items) {
    return items.slice().sort((left, right) => left.name.toLowerCase().localeCompare(right.name.toLowerCase()) || left.id.localeCompare(right.id));
}
function rankedActions(items, query) {
    const term = words(query).join(" ");
    function score(item) {
        if (words(item.action).join(" ") === term)
            return 100;
        if ((item.aliases || []).some(alias => words(alias).join(" ") === term))
            return 80;
        return words(item.name).join(" ").startsWith(term) ? 60 : 40;
    }
    const hits = items.filter(item => matches(item, query));
    if (!term)
        return hits;
    // Qt's JavaScript sort need not retain input order for equal scores.
    return hits.map((item, index) => ({
                item: item,
                index: index,
                score: score(item)
            })).sort((left, right) => right.score - left.score || left.index - right.index).map(hit => hit.item);
}

function actions(items) {
    return items.filter(item => item.action !== "power").map(item => Object.assign({}, item, {
            id: "nacre-action:" + item.action + ":" + encodeURIComponent(item.value || ""),
            kind: "action"
        }));
}
function favorite(id, apps, actions) {
    return id.startsWith("nacre-action:") ? actions.find(item => item.id === id) : apps.find(item => item.id === id);
}
