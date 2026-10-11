import QtQuick
import Quickshell
import Quickshell.Io
import qs.services

Scope {
    id: root
    function activePanel() {
        return NacrePanelState.panels[NacrePanelState.activeName()] || null;
    }
    function stateData() {
        const view = NacrePanelState.getForActive();
        return {
            active: NacreHyprland.activeWsId,
            workspaces: NacreHyprland.workspaces.values.length,
            dashboard: view?.dashboard ?? false,
            edgeMenu: view?.edgeMenu || "",
            clickEdgeMenus: DesktopSettings.data.clickEdgeMenus === true,
            tab: view?.dashboardTab ?? 0,
            launcher: view?.launcher ?? false,
            session: view?.session ?? false,
            osd: view?.osd ?? false,
            controlSection: view?.controlSection || "home",
            feedback: root.activePanel()?.feedback?.shown ?? false,
            hidden: NacrePanelState.hidden,
            left: view?.left ?? false,
            leftPinned: view?.leftPinned ?? false,
            query: view?.launcherQuery || "",
            launcherMode: view?.launcherMode || "apps",
            launcherRoute: view?.launcherRoute || "home",
            reveal: NacrePanelState.reveal,
            notificationsSuppressed: Object.values(NacrePanelState.panels).some(panel => panel.notifications?.suppressed === true)
        };
    }
    function previewPanel(name, index = 0) {
        const view = NacrePanelState.getForActive();
        if (!view)
            return;
        NacrePanelState.clearPopouts();
        NacrePanelState.closeTransient(view);
        view.previewOnly = true;
        if (name === "dashboard") {
            view.dashboardTab = Math.max(0, Math.min(4, index));
            view.dashboard = true;
        } else if (name === "osd")
            view.osd = true;
        else if (name === "left") {
            view.left = true;
            view.leftPinned = false;
        }
    }
    function restore(data) {
        try {
            if (typeof data !== "string" || data.length > 2097152)
                return false;
            const saved = JSON.parse(data);
            if (!saved || typeof saved !== "object" || Array.isArray(saved))
                return false;
            if (saved.version === 2) {
                if (!saved.screens || typeof saved.screens !== "object" || Array.isArray(saved.screens))
                    return false;
                DisplayRecovery.restoreSaved(saved);
                return true;
            }
            const view = NacrePanelState.getForActive();
            if (!view)
                return false;
            NacrePanelState.close();
            if (saved.launcher === true && NacrePanelState.modes.includes(saved.mode))
                NacrePanelState.openMode(saved.mode, "", false);
            if (saved.dashboard === true) {
                view.dashboardTab = Math.max(0, Math.min(4, Number(saved.tab) || 0));
                view.dashboard = true;
                view.dashboardPinned = false;
            }
            if (saved.left === true) {
                view.left = true;
                view.leftPinned = saved.leftPinned === true;
                const panel = activePanel();
                if (panel && ["chat", "chats", "brain", "settings"].includes(saved.leftSection))
                    panel.leftDrawer.section = saved.leftSection;
            }
            return true;
        } catch (error) {
            return false;
        }
    }
    function galleryData() {
        return Object.values(NacrePanelState.panels).map(panel => ({
                    count: panel.launcher?.galleryCount || 0,
                    index: panel.launcher?.galleryIndex ?? -1
                }));
    }
    function popupData() {
        return Object.values(NacrePanelState.panels).map(panel => ({
                    name: panel.popouts.currentName,
                    open: panel.popouts.hasCurrent,
                    height: panel.popouts.height,
                    width: panel.popouts.width,
                    center: panel.popouts.currentCenter,
                    header: panel.popouts.headerHovered
                }));
    }
    function workspace(id) {
        if (Number.isInteger(id) && id > 0)
            NacreHyprland.dispatch("workspace " + id);
    }
    function tab(index) {
        const view = NacrePanelState.getForActive();
        if (!view || !Number.isInteger(index) || index < 0 || index > 4)
            return;
        view.dashboardTab = index;
        view.dashboardPinned = false;
        view.dashboard = true;
        view.previewOnly = false;
    }
    IpcHandler {
        target: "leftDrawer"
        function section(name: string): void {
            const panel = root.activePanel();
            if (panel && ["chat", "chats", "brain", "settings"].includes(name))
                panel.leftDrawer.section = name;
        }
        function state(): string {
            const drawer = root.activePanel()?.leftDrawer;
            return JSON.stringify({
                section: drawer?.section,
                width: drawer?.width,
                height: drawer?.height
            });
        }
    }
    IpcHandler {
        target: "nacre"
        function state(): string {
            return JSON.stringify(root.stateData());
        }
        function restoreViews(data: string): void {
            root.restore(data);
        }
        function preview(name: string): void {
            NacrePanelState.openMode(name, "", true);
        }
        function previewDashboard(index: int): void {
            root.previewPanel("dashboard", index);
        }
        function controls(section: string): void {
            NacrePanelState.openControls(section || "home");
        }
        function previewFeedback(channel: string): void {
            if (["volume", "microphone", "display", "keyboard"].includes(channel))
                root.activePanel()?.feedback?.show(channel);
        }
        function previewSliders(): void {
            root.previewPanel("osd");
        }
        function previewLeft(): void {
            root.previewPanel("left");
        }
        function mode(name: string): void {
            NacrePanelState.openMode(name, "", false);
        }
        function menu(route: string, query: string): void {
            NacrePanelState.openMenu(route || "home", query || "", false);
        }
        function settings(): void {
            NacrePanelState.openSettings();
        }
        function left(): void {
            NacrePanelState.toggleLeft();
        }
        function hide(): void {
            NacrePanelState.hidden = !NacrePanelState.hidden;
        }
        function launcher(query: string): void {
            NacrePanelState.openMode(query.startsWith(">wallpaper") ? "wallpaper" : "apps", query, false);
        }
        function session(): void {
            NacrePanelState.toggleSession();
        }
        function popout(name: string, center: real): void {
            NacrePanelState.popout(name, center, NacrePanelState.activeName());
        }
        function close(): void {
            NacrePanelState.close();
        }
        function popupState(): string {
            return JSON.stringify(root.popupData());
        }
        function galleryStep(delta: int): void {
            for (const panel of Object.values(NacrePanelState.panels))
                panel.launcher?.galleryStep(delta);
        }
        function galleryState(): string {
            return JSON.stringify(root.galleryData());
        }
        function workspace(id: int): void {
            root.workspace(id);
        }
        function tab(index: int): void {
            root.tab(index);
        }
    }
}
