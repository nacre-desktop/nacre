pragma Singleton
import QtQuick
import Quickshell

Singleton {
    id: root
    property bool settingsVisible: false
    property string settingsPage: "appearance"
    property bool hidden: false
    property real reveal: hidden ? 0 : 1
    property var screens: ({})
    property var panels: ({})
    readonly property var providerViews: Object.values(screens).concat(NacrePearls.views)
    readonly property var modes: ["apps", "wallpaper", "palette", "legacy", "overview", "clipboard", "keys"]
    readonly property var pages: ["appearance", "desktop", "displays", "sound", "network", "bluetooth", "notifications", "workflows", "lock", "time", "ai", "maintenance"]
    function activeName() {
        return screens[NacreCompositor.focusedMonitor?.name] ? NacreCompositor.focusedMonitor.name : Object.keys(screens)[0] || "";
    }
    function getForActive() {
        return screens[activeName()] || null;
    }
    function clearPopouts() {
        for (const panel of Object.values(panels))
            if (panel?.popouts) {
                panel.popouts.hasCurrent = false;
                panel.popouts.pinned = false;
            }
    }
    function dockAsHover(view) {
        view.edgeMenu = "";
        view.hoverDocked = true;
    }
    function closeTransient(view) {
        view.hoverDocked = false;
        view.launcher = false;
        view.session = false;
        view.dashboard = false;
        view.dashboardPinned = false;
        view.osd = false;
        view.edgeMenu = "";
    }
    function openEdge(name, screenName) {
        const view = screenName ? screens[screenName] : getForActive();
        if (!view || !["dashboard", "left", "osd"].includes(name))
            return false;
        clearPopouts();
        closeTransient(view);
        if (name === "left" || !view.leftPinned)
            view.left = false;
        if (name === "left")
            view.leftPinned = false;
        view.previewOnly = false;
        view.edgeMenu = name;
        if (name === "osd")
            view.controlSection = "home";
        view[name] = true;
        hidden = false;
        return true;
    }
    function popout(name, center, screenName) {
        const view = screens[screenName], panel = panels[screenName];
        if (!view || !panel?.popouts || view.session || !["audio", "network", "bluetooth", "battery", "calendar", "notifications"].includes(name) || !Number.isFinite(center))
            return false;
        view.dashboard = false;
        view.dashboardPinned = false;
        view.osd = false;
        if (panel.popouts.currentName !== name)
            panel.popouts.pinned = false;
        panel.popouts.currentName = name;
        panel.popouts.currentCenter = center;
        panel.popouts.hasCurrent = true;
        return true;
    }
    function openMenu(route = "home", query = "", toggle = false) {
        const view = getForActive();
        if (!view || typeof route !== "string" || typeof query !== "string")
            return false;
        const same = view.launcher && view.launcherMode === "apps" && view.launcherRoute === route && view.launcherQuery === query;
        clearPopouts();
        closeTransient(view);
        view.previewOnly = false;
        view.launcherMode = "apps";
        view.launcherRoute = route;
        view.launcherQuery = query;
        view.launcherRequest = (view.launcherRequest || 0) + 1;
        view.launcher = !(toggle && same);
        hidden = false;
        return true;
    }
    function openMode(mode, query = "", preview = false, screenName = "", forceOpen = false) {
        const view = screenName ? screens[screenName] : getForActive();
        if (!view || !modes.includes(mode) || typeof query !== "string")
            return false;
        const same = view.launcher && view.launcherMode === mode && view.launcherQuery === query && view.previewOnly === preview;
        clearPopouts();
        closeTransient(view);
        view.previewOnly = preview;
        view.launcherMode = mode;
        if (mode === "apps")
            view.launcherRoute = "home";
        view.launcherQuery = query;
        view.launcherRequest = (view.launcherRequest || 0) + 1;
        view.launcher = forceOpen || !same;
        hidden = false;
        return true;
    }
    function hoverEdge(name, screenName) {
        const view = screens[screenName];
        if (!view || !["dashboard", "left", "osd"].includes(name) || view.session || view.launcher || view.dashboardPinned)
            return false;
        view.hoverDocked = false;
        clearPopouts();
        view.dashboard = name === "dashboard";
        view.osd = name === "osd";
        if (!view.leftPinned)
            view.left = name === "left";
        if (name === "osd")
            view.controlSection = "home";
        view.edgeMenu = "";
        view.previewOnly = false;
        hidden = false;
        return true;
    }
    function openControls(section = "home", screenName = "", forceOpen = false) {
        const view = screenName ? screens[screenName] : getForActive();
        if (!view || !["home", "audio", "network", "bluetooth", "battery", "notifications"].includes(section))
            return false;
        const wasOpen = view.osd && view.controlSection === section && view.edgeMenu === "osd";
        clearPopouts();
        closeTransient(view);
        view.previewOnly = false;
        view.controlSection = section;
        view.osd = forceOpen || !wasOpen;
        view.edgeMenu = view.osd ? "osd" : "";
        hidden = false;
        return true;
    }
    function openDeviceSettings(icon) {
        const page = {
            audio: "sound",
            network: "network",
            bluetooth: "bluetooth"
        }[icon];
        return page ? openControls(icon) : false;
    }
    function openSettings(page) {
        NacreSettingsApp.open(page || "");
        return true;
    }
    function toggleLeft() {
        const view = getForActive();
        if (!view)
            return false;
        const opening = !view.left;
        clearPopouts();
        closeTransient(view);
        view.previewOnly = false;
        view.left = opening;
        view.leftPinned = false;
        view.edgeMenu = opening ? "left" : "";
        hidden = false;
        return true;
    }
    function toggleSession() {
        const view = getForActive();
        if (!view)
            return false;
        const opening = !view.session;
        clearPopouts();
        closeTransient(view);
        view.previewOnly = false;
        view.session = opening;
        hidden = false;
        return true;
    }
    function close() {
        const name = activeName(), view = screens[name];
        if (!view)
            return;
        const screen = Quickshell.screens.find(item => item.name === name);
        if (screen)
            NacreHoverIntent.dismiss(screen, panels[name]?.input?.hovered === true);
        closeTransient(view);
        view.left = false;
        view.leftPinned = false;
        view.previewOnly = false;
        clearPopouts();
    }
    Behavior on reveal {
        enabled: DesktopSettings.data.animations !== false
        NumberAnimation {
            duration: 200
            easing.type: Easing.OutCubic
        }
    }
}
