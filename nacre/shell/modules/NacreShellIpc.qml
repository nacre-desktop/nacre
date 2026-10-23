import QtQuick
import Quickshell
import Quickshell.Io
import qs.services

Scope {
    id: root
    property var pendingDashboard: null
    function daemonCall(method, params) {
        if (method === "ui.dispatch") {
            const requiresActiveView = params?.target === "drawers" && params?.method === "toggle" || params?.target === "leftDrawer" && params?.method === "section" || params?.target === "nacre" && ["preview", "previewDashboard", "controls", "previewFeedback", "previewSliders", "previewLeft", "mode", "menu", "left", "launcher", "clipboard", "session", "popout", "close", "galleryStep", "tab"].includes(params?.method);
            if (requiresActiveView && !NacrePanelState.getForActive())
                return {
                    err: {
                        code: "unavailable",
                        message: NacreI18n.tr("ui.shell-ipc.error.no-active-display", "No active display is available for this UI action"),
                        retry: true
                    }
                };
            // N0048: `nacre settings [page]` is the provider-neutral Settings route
            // (Bead B0045 uses the same name). The legacy IPC handler takes no page.
            if (params?.target === "nacre" && params?.method === "settings" && Array.isArray(params.args) && params.args.length === 1 && typeof params.args[0] === "string") {
                NacrePanelState.openSettings(params.args[0]);
                return {
                    ok: null
                };
            }
            return NacreDaemon.dispatchIpc(params);
        }
        if (method !== "ui.menu.activate")
            return {
                err: {
                    code: "unknown_method",
                    message: NacreI18n.tr("ui.shell-ipc.error.ui-method-unavailable", "UI method is unavailable"),
                    retry: false
                }
            };
        const row = params?.row;
        if (!row || typeof row.id !== "string" || typeof row.operation !== "string" || row.disabled || !NacreMenu.daemonOperationSupported(row.operation))
            return {
                err: {
                    code: "validation",
                    message: NacreI18n.tr("ui.shell-ipc.error.unsupported-launcher-action", "Unsupported launcher action"),
                    retry: false
                }
            };
        const panel = root.activePanel();
        if (!panel?.launcher)
            return {
                err: {
                    code: "unavailable",
                    message: NacreI18n.tr("ui.shell-ipc.error.launcher-view-unavailable", "Launcher view is unavailable"),
                    retry: true
                }
            };
        if (!panel.launcher.activateFromDaemon(row))
            return {
                err: {
                    code: "busy",
                    message: NacreI18n.tr("ui.shell-ipc.error.launcher-view-busy", "Launcher view is busy"),
                    retry: true
                }
            };
        return {
            ok: {
                keep_open: true
            }
        };
    }
    Component.onCompleted: if (!NacreDaemon.registerUi(root, root.daemonCall))
        console.warn("NacreShellIpc: UI provider is already registered")
    Component.onDestruction: NacreDaemon.unregisterUi(root)
    function activePanel() {
        return NacrePanelState.panels[NacrePanelState.activeName()] || null;
    }
    function stateData() {
        const view = NacrePanelState.getForActive();
        return {
            active: NacreCompositor.activeWsId,
            workspaces: NacreCompositor.workspaces.values.length,
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
    function dashboardIndex(value) {
        const names = NacreDashboardRegistry.names;
        const index = Math.max(0, Math.min(names.length - 1, Math.trunc(Number(value)) || 0));
        return NacreDashboardState.tabs.includes(names[index]) ? index : Math.max(0, names.indexOf(NacreDashboardState.tabs[0]));
    }
    function setDashboardTab(view, value) {
        const index = Math.max(0, Math.min(NacreDashboardRegistry.names.length - 1, Math.trunc(Number(value)) || 0));
        if (NacreDashboardState.extrasReady === false && index >= 4) {
            pendingDashboard = {
                view: view,
                index: index
            };
            return;
        }
        pendingDashboard = null;
        view.dashboardTab = dashboardIndex(index);
    }
    Connections {
        target: NacreDashboardState
        function onExtrasReadyChanged() {
            if (NacreDashboardState.extrasReady && root.pendingDashboard) {
                const pending = root.pendingDashboard;
                root.pendingDashboard = null;
                if (pending.view)
                    pending.view.dashboardTab = root.dashboardIndex(pending.index);
            }
        }
    }
    function previewPanel(name, index = 0) {
        const view = NacrePanelState.getForActive();
        if (!view)
            return;
        NacrePanelState.clearPopouts();
        NacrePanelState.closeTransient(view);
        view.previewOnly = true;
        if (name === "dashboard") {
            root.setDashboardTab(view, index);
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
                root.setDashboardTab(view, saved.tab);
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
            NacreCompositor.switchWorkspace(id);
    }
    function tab(index) {
        const view = NacrePanelState.getForActive();
        if (!view || !Number.isInteger(index))
            return;
        root.setDashboardTab(view, index);
        view.dashboardPinned = false;
        view.dashboard = true;
        view.previewOnly = false;
    }
    IpcHandler {
        id: leftDrawerIpc
        Component.onCompleted: NacreDaemon.registerIpc("leftDrawer", leftDrawerIpc, {
            "section": ["s"],
            "state": []
        })
        Component.onDestruction: NacreDaemon.unregisterIpc("leftDrawer", leftDrawerIpc)
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
        id: nacreIpc
        Component.onCompleted: NacreDaemon.registerIpc("nacre", nacreIpc, {
            "state": [],
            "restoreViews": ["s"],
            "preview": ["s"],
            "previewDashboard": ["i"],
            "controls": ["s"],
            "previewFeedback": ["s"],
            "previewSliders": [],
            "previewLeft": [],
            "mode": ["s"],
            "menu": ["s", "s"],
            "settings": [],
            "left": [],
            "hide": [],
            "launcher": ["s"],
            "clipboard": [],
            "session": [],
            "popout": ["s", "r"],
            "close": [],
            "popupState": [],
            "galleryStep": ["i"],
            "galleryState": [],
            "workspace": ["i"],
            "tab": ["i"]
        })
        Component.onDestruction: NacreDaemon.unregisterIpc("nacre", nacreIpc)
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
        function clipboard(): void {
            if (DesktopSettings.data.nativeClipboard !== false)
                NacrePanelState.openMode("clipboard");
            else
                NacrePanelState.openMenu("actions.clipboard", "", false);
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
