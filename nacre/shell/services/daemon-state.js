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

// Own only mutable UI projections, without encoding a second JSON document.
function copy(value) {
    if (Array.isArray(value))
        return value.map(copy);
    if (!object(value))
        return value;
    const result = {};
    for (const key of Object.keys(value))
        Object.defineProperty(result, key, {
            value: copy(value[key]),
            enumerable: true,
            writable: true,
            configurable: true
        });
    return result;
}
function equal(left, right) {
    if (left === right)
        return true;
    if (left === null || right === null || typeof left !== "object" || typeof right !== "object" || Array.isArray(left) !== Array.isArray(right))
        return false;
    const keys = Object.keys(left);
    return keys.length === Object.keys(right).length && keys.every(key => Object.prototype.hasOwnProperty.call(right, key) && equal(left[key], right[key]));
}
