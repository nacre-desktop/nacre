.pragma library

function merge(fragments) {
    const result = {
        version: 1,
        groups: [],
        pages: [],
        sections: [],
        settings: [],
        actions: [],
        aliases: {}
    };
    for (const fragment of fragments || []) {
        for (const key of ["groups", "pages", "sections", "settings", "actions"])
            result[key] = result[key].concat(fragment?.[key] || []);
        Object.assign(result.aliases, fragment?.aliases || {});
    }
    return result;
}
function allowed(condition, capabilities) {
    if (!condition)
        return true;
    if (condition.capability)
        return capabilities?.[condition.capability] === true;
    if (condition.all)
        return condition.all.every(item => allowed(item, capabilities));
    if (condition.any)
        return condition.any.some(item => allowed(item, capabilities));
    if (condition.not)
        return !allowed(condition.not, capabilities);
    return false;
}
function visiblePage(schema, page, capabilities) {
    if (!page || !allowed(page.visibleWhen, capabilities))
        return false;
    return !page.parent || visiblePage(schema, schema.pages.find(item => item.id === page.parent), capabilities);
}
function settingsFor(schema, page, capabilities) {
    if (!visiblePage(schema, schema.pages.find(item => item.id === page), capabilities))
        return [];
    return schema.settings.filter(item => item.page === page && allowed(item.visibleWhen, capabilities) && allowed(schema.sections.find(section => section.id === item.section)?.visibleWhen, capabilities));
}
function resolve(schema, link, capabilities) {
    if (typeof link !== "string" || link.length > 256 || !/^[a-z0-9/-]+(?:#[a-zA-Z0-9._/-]+)?$/.test(link))
        return null;
    const parts = link.split("#");
    let route = schema.aliases[parts[0]] || parts[0];
    let page = schema.pages.find(item => item.id === route);
    if (route === "nacre-ai" && !allowed({
        capability: "ai"
    }, capabilities)) {
        route = "addons";
        page = schema.pages.find(item => item.id === route);
    }
    if (!visiblePage(schema, page, capabilities))
        return null;
    const setting = parts[1] ? settingsFor(schema, route, capabilities).find(item => item.key === parts[1] || item.id === parts[1]) : null;
    if (parts[1] && !setting)
        return null;
    return {
        page: route,
        setting: setting?.id || ""
    };
}
function breadcrumb(schema, page, text) {
    const item = schema.pages.find(row => row.id === page);
    if (!item)
        return "";
    const group = schema.groups.find(row => row.id === item.group);
    const parent = schema.pages.find(row => row.id === item.parent);
    return [group && text(group.labelId), parent && text(parent.labelId), text(item.labelId)].filter(Boolean).join(" / ");
}
function search(schema, query, capabilities, text) {
    const words = query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
    if (!words.length)
        return [];
    return schema.settings.filter(setting => {
        const page = schema.pages.find(row => row.id === setting.page);
        const section = schema.sections.find(row => row.id === setting.section);
        if (!visiblePage(schema, page, capabilities) || !allowed(setting.visibleWhen, capabilities) || !allowed(section?.visibleWhen, capabilities))
            return false;
        const haystack = [text(setting.labelId), text(setting.descriptionId), breadcrumb(schema, setting.page, text), text(section?.labelId), ...(setting.synonyms || []), ...(setting.options || []).map(option => text(option.labelId))].join(" ").toLocaleLowerCase();
        return words.every(word => haystack.includes(word));
    }).map(setting => ({
                id: setting.id,
                page: setting.page,
                label: text(setting.labelId),
                description: text(setting.descriptionId),
                breadcrumb: breadcrumb(schema, setting.page, text) + " / " + text(schema.sections.find(section => section.id === setting.section)?.labelId)
            }));
}
function validValue(setting, value) {
    const type = setting.type;
    if (type === "boolean" && typeof value !== "boolean" || type === "string" && typeof value !== "string" || type === "array" && !Array.isArray(value) || type === "integer" && !Number.isInteger(value) || type === "number" && (typeof value !== "number" || !Number.isFinite(value)))
        return false;
    if (type === "enum" && !setting.options.some(option => option.value === value))
        return false;
    if (setting.range && (value < setting.range.min || value > setting.range.max || Math.abs((value - setting.range.min) / setting.range.step - Math.round((value - setting.range.min) / setting.range.step)) > .00001))
        return false;
    return ["boolean", "string", "array", "integer", "number", "enum"].includes(type);
}
function validPath(path) {
    // QVariant sequence wrappers cross QML signals as array-like objects.
    // JSON normalization restores the portable data type before validation.
    path = path ? JSON.parse(JSON.stringify(path)) : null;
    return Array.isArray(path) && path.length > 0 && path.every(key => typeof key === "string" && /^[A-Za-z0-9_-]+$/.test(key) && !["__proto__", "constructor", "prototype"].includes(key));
}
function readPath(document, path, fallback) {
    path = path ? JSON.parse(JSON.stringify(path)) : null;
    if (!validPath(path))
        return fallback;
    let value = document;
    for (const key of path)
        value = value?.[key];
    return value === undefined ? fallback : value;
}
function writePath(document, path, value) {
    path = path ? JSON.parse(JSON.stringify(path)) : null;
    if (!validPath(path))
        throw new Error("Invalid store path");
    const result = JSON.parse(JSON.stringify(document || {}));
    let item = result;
    for (const key of path.slice(0, -1)) {
        if (!item[key] || typeof item[key] !== "object" || Array.isArray(item[key]))
            item[key] = {};
        item = item[key];
    }
    item[path[path.length - 1]] = value;
    return result;
}
