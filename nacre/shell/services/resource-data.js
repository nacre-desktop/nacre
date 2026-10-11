function cpu(text) {
    const row = text.split(/\n/).find(line => /^cpu\s/.test(line));
    if (!row)
        return null;
    const columns = row.trim().split(/\s+/).slice(1, 9).map(Number);
    if (columns.length < 4 || columns.some(value => !Number.isFinite(value) || value < 0))
        return null;
    return {
        total: columns.reduce((sum, value) => sum + value, 0),
        idle: columns[3] + (columns[4] || 0)
    };
}
function fraction(before, after) {
    if (!before || !after)
        return NaN;
    const total = after.total - before.total, idle = after.idle - before.idle;
    if (total <= 0 || idle < 0 || idle > total)
        return NaN;
    return Math.max(0, Math.min(1, (total - idle) / total));
}
function memory(text) {
    const values = {};
    for (const line of text.split(/\n/)) {
        const match = /^(\w+):\s+(\d+)\s+kB\s*$/.exec(line);
        if (match)
            values[match[1]] = Number(match[2]);
    }
    const total = values.MemTotal;
    if (!Number.isFinite(total) || total <= 0)
        return null;
    const available = values.MemAvailable ?? (values.MemFree + (values.Buffers || 0) + (values.Cached || 0) + (values.SReclaimable || 0) - (values.Shmem || 0));
    if (!Number.isFinite(available))
        return null;
    return {
        total: total,
        used: total - Math.max(0, Math.min(total, available))
    };
}
