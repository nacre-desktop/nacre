.pragma library

function pool(entries, kind) {
    return entries.filter(e => kind === "all" || e.dynamic === (kind === "dynamic")).map(e => e.path);
}

function next(entries, current, shuffle, remaining, random) {
    if (entries.length < 2)
        return {
            path: "",
            remaining: []
        };
    if (!shuffle) {
        const index = entries.indexOf(current);
        return {
            path: entries[(index + 1) % entries.length],
            remaining: []
        };
    }
    let bag = remaining.filter(path => entries.includes(path) && path !== current);
    if (!bag.length)
        bag = entries.filter(path => path !== current);
    const index = Math.min(bag.length - 1, Math.floor(random * bag.length));
    const path = bag.splice(index, 1)[0];
    return {
        path: path,
        remaining: bag
    };
}
