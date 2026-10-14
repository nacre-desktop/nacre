.pragma library

function group(networks, saved) {
    const groups = {
        connected: [],
        known: [],
        available: []
    };
    for (const point of networks) {
        const key = point.active ? "connected" : point.saved === true || saved.includes(point.ssid) ? "known" : "available";
        groups[key].push(point);
    }
    return groups;
}
