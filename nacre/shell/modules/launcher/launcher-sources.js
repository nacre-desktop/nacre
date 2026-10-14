.pragma library

function defaults() {
    return {
        version: 1,
        brain: true,
        files: true,
        calculator: true,
        actions: true,
        ai: true,
        webEngine: "duckduckgo"
    };
}
function decode(text) {
    const result = defaults();
    if (!text.trim())
        return result;
    try {
        const value = JSON.parse(text);
        if (!value || typeof value !== "object" || Array.isArray(value) || value.version !== 1)
            return result;
        for (const key of ["brain", "files", "calculator", "actions", "ai"])
            if (value[key] !== undefined && typeof value[key] !== "boolean")
                return result;
        if (value.webEngine !== undefined && !["duckduckgo", "google", "brave"].includes(value.webEngine))
            return result;
        for (const key of ["brain", "files", "calculator", "actions", "ai", "webEngine"])
            if (value[key] !== undefined)
                result[key] = value[key];
        return result;
    } catch (error) {
        return result;
    }
}
function webUrl(engine, query) {
    const base = {
        duckduckgo: "https://duckduckgo.com/?q=",
        google: "https://www.google.com/search?q=",
        brave: "https://search.brave.com/search?q="
    };
    return (base[engine] || base.duckduckgo) + encodeURIComponent(query);
}
