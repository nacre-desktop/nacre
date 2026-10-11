import QtQuick
import Quickshell
import Quickshell.Io
import qs.widgets
import qs.services

Scope {
    id: root
    property bool launcherInterrupted: false
    function dismissPassive() {
        for (const screen of Quickshell.screens) {
            const view = NacrePanelState.screens[screen.name], panel = NacrePanelState.panels[screen.name];
            if (!view || view.launcher || view.session || view.edgeMenu)
                continue;
            NacreHoverIntent.dismiss(screen);
            if (!view.dashboardPinned)
                view.dashboard = false;
            if (!view.leftPinned)
                view.left = false;
            view.osd = false;
            if (panel?.popouts && !panel.popouts.pinned)
                panel.popouts.hasCurrent = false;
        }
    }
    function launcherPress() {
        launcherInterrupted = false;
    }
    function launcherInterrupt() {
        launcherInterrupted = true;
    }
    function launcherRelease() {
        if (!launcherInterrupted)
            NacrePanelState.openMode("apps", "", false);
    }
    function toggle(name) {
        const view = NacrePanelState.getForActive();
        if (!view)
            return false;
        if (name === "launcher")
            return NacrePanelState.openMode("apps", "", false);
        if (name === "session")
            return NacrePanelState.toggleSession();
        if (name === "left")
            return NacrePanelState.toggleLeft();
        if (name === "dashboard") {
            const opening = !view.dashboard;
            NacrePanelState.clearPopouts();
            NacrePanelState.closeTransient(view);
            view.previewOnly = false;
            view.dashboard = opening;
            view.dashboardPinned = opening && view.dashboardTab === 4;
            return true;
        }
        if (name === "osd") {
            view.osd = !view.osd;
            view.previewOnly = false;
            return true;
        }
        if (["leftPinned", "dashboardPinned", "previewOnly"].includes(name)) {
            view[name] = !view[name];
            return true;
        }
        return false;
    }
    NacreShortcut {
        name: "dismissHoverEdges"
        description: "Dismiss passive hover menus"
        onPressed: root.dismissPassive()
    }
    NacreShortcut {
        name: "session"
        description: "Toggle session menu"
        onPressed: NacrePanelState.toggleSession()
    }
    NacreShortcut {
        name: "launcher"
        description: "Toggle launcher"
        onPressed: root.launcherPress()
        onReleased: root.launcherRelease()
    }
    NacreShortcut {
        name: "launcherInterrupt"
        description: "Interrupt launcher chord"
        onPressed: root.launcherInterrupt()
    }
    IpcHandler {
        target: "drawers"
        function toggle(drawer: string): void {
            root.toggle(drawer);
        }
        function list(): string {
            return ["previewOnly", "osd", "session", "launcher", "left", "leftPinned", "dashboard", "dashboardPinned"].join("\n");
        }
    }
}
