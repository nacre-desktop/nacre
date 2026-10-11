.pragma library

function fraction(value) {
    return Number.isFinite(value) ? Math.max(0, Math.min(1, value)) : 0;
}
function duration(value) {
    if (!Number.isFinite(value) || value < 0)
        return "—";
    const seconds = Math.floor(value), minutes = Math.floor(seconds / 60);
    return (minutes >= 60 ? Math.floor(minutes / 60) + ":" + String(minutes % 60).padStart(2, "0") : String(minutes)) + ":" + String(seconds % 60).padStart(2, "0");
}
function percent(value) {
    return Number.isFinite(value) ? Math.round(fraction(value) * 100) + "%" : "—";
}
function temperature(value) {
    return Number.isFinite(value) ? Math.round(value) + "°C" : "—°C";
}
function size(kib) {
    if (!Number.isFinite(kib) || kib < 0)
        return "—";
    if (kib >= 1048576)
        return (kib / 1048576).toFixed(1) + " GiB";
    if (kib >= 1024)
        return (kib / 1024).toFixed(1) + " MiB";
    return Math.round(kib) + " KiB";
}
