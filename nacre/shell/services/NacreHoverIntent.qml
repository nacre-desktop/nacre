pragma Singleton
import QtQuick
import Quickshell

Singleton {
    id: root
    readonly property int edgeWidth: 3
    readonly property int dashboardDepth: Math.max(edgeWidth, Math.floor((40 + (DesktopSettings.data.frameWidth ?? 10)) / 2))
    property var positions: ({})
    property var lipRegions: ({})
    property var blocked: ({})
    property var headers: ({})
    property var headerSources: ({})
    property var popupHovered: ({})
    property var popupRegions: ({})
    property var owners: ({})
    function validScreen(screen) {
        return !!screen && typeof screen.name === "string" && screen.name.length > 0;
    }
    function regionContains(box, point) {
        return !!box && !!point && point.x >= box.x && point.y >= box.y && point.x < box.x + box.width && point.y < box.y + box.height;
    }
    function markedContains(name, point) {
        const boxes = lipRegions[point?.name];
        if (boxes)
            return regionContains(boxes[name], point) || regionContains(boxes.resting?.[name], point);
        if (name === "dashboard")
            return !!point && point.y >= 0 && point.y <= dashboardDepth && Math.abs(point.x - point.width / 2) <= Math.min(350, point.width * .225);
        return !!point && (name === "left" ? point.x < edgeWidth : point.x >= point.width - edgeWidth);
    }
    function setHeaderHover(name, source, hovered) {
        headerSources[name] = Object.assign({}, headerSources[name] || {}, {
            [source]: hovered
        });
        headers[name] = Object.values(headerSources[name]).some(Boolean);
    }
    function titleContains(point) {
        return markedContains("dashboard", point);
    }
    function fullscreenFor(name) {
        return NacreHyprland.focusedMonitor?.name === name && Number(NacreHyprland.activeClient?.lastIpcObject?.fullscreen) === 2;
    }
    function canAuto(screen, buttons) {
        return validScreen(screen) && buttons === Qt.NoButton && !fullscreenFor(screen.name);
    }
    function canOpen(name, screen, buttons) {
        return ["dashboard", "left", "osd", "popouts"].includes(name) && canAuto(screen, buttons) && !blocked[screen.name]?.[name];
    }
    function observe(screen, x, y) {
        if (!validScreen(screen) || ![x, y, screen.width, screen.height].every(Number.isFinite) || screen.width <= 0 || screen.height <= 0)
            return;
        const point = {
            name: screen.name,
            x: x,
            y: y,
            width: screen.width,
            height: screen.height
        };
        positions[screen.name] = point;
        const gates = blocked[screen.name];
        if (!gates)
            return;
        if (!titleContains(point))
            gates.dashboard = false;
        if (!markedContains("left", point))
            gates.left = false;
        if (!markedContains("osd", point))
            gates.osd = false;
    }
    function recordPopupRegion(screen, x, y, width, height) {
        if (validScreen(screen) && [x, y, width, height].every(Number.isFinite) && width > 0 && height > 0)
            popupRegions[screen.name] = {
                x: x,
                y: y,
                width: width,
                height: height
            };
    }
    function popupAtPointer(name) {
        const point = positions[name], box = popupRegions[name];
        return !!point && !!box && point.x >= box.x && point.y >= box.y && point.x < box.x + box.width && point.y < box.y + box.height;
    }
    function dismiss(screen, pointerPresent = true) {
        if (!validScreen(screen))
            return;
        const point = positions[screen.name];
        blocked[screen.name] = {
            dashboard: pointerPresent && titleContains(point),
            left: pointerPresent && markedContains("left", point),
            osd: pointerPresent && markedContains("osd", point),
            popouts: popupHovered[screen.name] === true || popupAtPointer(screen.name)
        };
    }
    function rearm(name, screen) {
        if (validScreen(screen) && blocked[screen.name] && ["dashboard", "left", "osd", "popouts"].includes(name) && (name === "popouts" || !markedContains(name, positions[screen.name])))
            blocked[screen.name][name] = false;
    }
    function register(name, owner) {
        if (name && owner)
            owners[name] = owner;
    }
    function release(name, owner) {
        if (owners[name] !== owner)
            return;
        for (const map of [positions, blocked, headers, headerSources, popupHovered, popupRegions, lipRegions, owners])
            delete map[name];
    }
}
