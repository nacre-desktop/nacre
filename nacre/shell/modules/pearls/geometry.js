.pragma library

function edge(kind) {
    return ({
            dashboard: "top",
            launcher: "bottom",
            ai: "left",
            controls: "right",
            media: "top"
        })[kind] || "";
}

function outward(kind, dx, dy) {
    return ({
            top: dy,
            bottom: -dy,
            left: dx,
            right: -dx
        })[edge(kind)] || 0;
}

function clamp(value, low, high) {
    return Math.max(low, Math.min(Math.max(low, high), value));
}

function place(point, size, monitor) {
    return {
        x: clamp(point.x - size.width / 2, monitor.x + 12, monitor.x + monitor.width - size.width - 12),
        y: clamp(point.y - 24, monitor.y + 12, monitor.y + monitor.height - size.height - 12)
    };
}

function docking(kind, point, monitors, band = 42) {
    for (const monitor of monitors) {
        if (point.x < monitor.x || point.y < monitor.y || point.x > monitor.x + monitor.width || point.y > monitor.y + monitor.height)
            continue;
        const side = edge(kind);
        const near = side === "top" ? point.y - monitor.y <= band : side === "bottom" ? monitor.y + monitor.height - point.y <= band : side === "left" ? point.x - monitor.x <= band : monitor.x + monitor.width - point.x <= band;
        if (near)
            return {
                screen: monitor.name,
                edge: side
            };
    }
    return null;
}
