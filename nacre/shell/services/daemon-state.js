// RFC 7396 with structural sharing. Only changed branches acquire new identities.
function object(value) {
    return value !== null && typeof value === "object" && !Array.isArray(value);
}
function merge(previous, patch) {
    if (!object(patch))
        return patch;
    const original = object(previous) ? previous : Object.create(null);
    let result = original;
    for (const key of Object.keys(patch)) {
        const present = Object.prototype.hasOwnProperty.call(original, key);
        const value = patch[key] === null ? undefined : merge(present ? original[key] : undefined, patch[key]);
        if (patch[key] === null ? !present : present && value === original[key])
            continue;
        if (result === original)
            result = Object.assign(Object.create(null), original);
        if (patch[key] === null)
            delete result[key];
        else
            result[key] = value;
    }
    return result;
}
