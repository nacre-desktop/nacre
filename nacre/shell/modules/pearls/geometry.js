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

// One magnetic target for layer lifts and native windows. Distances are logical
// pixels from the panel edge to the frame, independent of where it was grabbed.
function magnet(kind, body, point, monitors, previous = null, anchor = body) {
    const side = edge(kind);
    if (!side || !point || !Number.isFinite(point.x) || !Number.isFinite(point.y) || !body || ![body.x, body.y, body.width, body.height].every(Number.isFinite) || body.width <= 0 || body.height <= 0)
        return null;
    for (const monitor of monitors) {
        if (point.x < monitor.x || point.y < monitor.y || point.x >= monitor.x + monitor.width || point.y >= monitor.y + monitor.height)
            continue;
        const horizontal = side === "top" || side === "bottom";
        const low = horizontal ? monitor.x : monitor.y;
        const high = low + (horizontal ? monitor.width : monitor.height);
        const panelLow = horizontal ? body.x : body.y;
        const span = horizontal ? body.width : body.height;
        if (Math.min(high, panelLow + span) - Math.max(low, panelLow) < Math.min(96, span / 3))
            continue;
        const frame = monitor.frame || {};
        const across = side === "top" ? monitor.y + (frame.top || 0) : side === "bottom" ? monitor.y + monitor.height - (frame.bottom || 0) : side === "left" ? monitor.x + (frame.left || 0) : monitor.x + monitor.width - (frame.right || 0);
        const leading = side === "top" ? body.y : side === "bottom" ? body.y + body.height : side === "left" ? body.x : body.x + body.width;
        const distance = Math.abs(leading - across);
        if (distance > 170)
            continue;
        const armedBefore = previous?.armed && previous.screen === monitor.name && previous.edge === side;
        const along = clamp(panelLow + span / 2, low + 24, high - 24);
        const anchorAlong = horizontal ? anchor.x + anchor.width / 2 : anchor.y + anchor.height / 2;
        const originAlong = clamp(anchorAlong, Math.max(low + 24, along - 96), Math.min(high - 24, along + 96));
        return {
            screen: monitor.name,
            edge: side,
            armed: distance <= (armedBefore ? 104 : 72),
            distance: distance,
            origin: horizontal ? {
                x: originAlong,
                y: across
            } : {
                x: across,
                y: originAlong
            },
            tip: horizontal ? {
                x: along,
                y: leading
            } : {
                x: leading,
                y: along
            }
        };
    }
    return null;
}

// The same stationary landing field works even when a compositor hides the
// moving window's coordinates. It is a target, not a guessed window position.
function landing(kind, monitor) {
    const side = edge(kind);
    if (!side || !monitor)
        return null;
    const frame = monitor.frame || {};
    const horizontal = side === "top" || side === "bottom";
    const across = side === "top" ? monitor.y + (frame.top || 0) : side === "bottom" ? monitor.y + monitor.height - (frame.bottom || 0) : side === "left" ? monitor.x + (frame.left || 0) : monitor.x + monitor.width - (frame.right || 0);
    const along = horizontal ? monitor.x + monitor.width / 2 : monitor.y + (frame.top || 0) + (monitor.height - (frame.top || 0) - (frame.bottom || 0)) / 2;
    return {
        screen: monitor.name,
        edge: side,
        origin: horizontal ? {
            x: along,
            y: across
        } : {
            x: across,
            y: along
        }
    };
}
