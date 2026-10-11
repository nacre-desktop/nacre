.pragma library

function group(entries) {
    const best = new Map();
    for (const network of entries) {
        if (!network.ssid)
            continue;
        // Read both fields for every AP so QML also tracks changes on duplicates.
        const active = network.active;
        const strength = network.strength;
        const previous = best.get(network.ssid);
        if (!previous || (active && !previous.active) || (active === previous.active && strength > previous.strength))
            best.set(network.ssid, network);
    }
    return Array.from(best.values()).sort((a, b) => Number(b.active) - Number(a.active) || b.strength - a.strength);
}
