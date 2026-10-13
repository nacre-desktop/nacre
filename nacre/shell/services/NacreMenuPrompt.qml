pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

Singleton {
    id: root
    property var request: null
    property string message: ""
    property var queued: []
    readonly property bool active: !!request
    readonly property var ownerView: request ? NacrePanelState.screens[request.screenName] || null : null
    readonly property string cli: Quickshell.env("HOME") + "/.local/share/nacre/shell/tools/menu-cli.py"
    function open(ident) {
        if (!/^[0-9a-f]{32}$/.test(ident))
            return;
        const view = NacrePanelState.getForActive();
        const blocked = active || !!view && (view.launcher || view.session || view.left || view.dashboard && view.dashboardPinned);
        if (reader.running || writer.running) {
            if (!queued.includes(ident) && queued.length < 2)
                queued = [...queued, ident];
            return;
        }
        reader.ident = ident;
        reader.blocked = blocked;
        reader.running = true;
    }
    function finish(value, cancelled = false, error = "") {
        if (!request || writer.running)
            return;
        writer.ident = request.id;
        writer.response = {
            value: value,
            cancelled: cancelled,
            error: error
        };
        writer.running = true;
        const view = ownerView;
        request = null;
        if (view)
            NacrePanelState.closeTransient(view);
    }
    function cancel(ident) {
        if (request?.id === ident)
            finish("", true);
    }
    function next() {
        if (!reader.running && !writer.running && queued.length) {
            const ident = queued[0];
            queued = queued.slice(1);
            open(ident);
        }
    }
    Timer {
        id: followup
        interval: 1
        onTriggered: root.next()
    }
    Process {
        id: reader
        property string ident: ""
        property bool blocked: false
        onExited: followup.restart()
        command: ["python3", root.cli, "read-request", ident]
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const data = JSON.parse(text);
                    if (data.id !== reader.ident || !["pick", "input"].includes(data.kind))
                        throw new Error();
                    const view = NacrePanelState.getForActive();
                    if (reader.blocked || root.active || !!view && (view.launcher || view.session || view.left)) {
                        writer.ident = data.id;
                        writer.response = {
                            value: "",
                            cancelled: true,
                            error: NacreI18n.tr("ui.menu-prompt.error.close-the-current-menu-or-drawer-before-opening-a-selec", "Close the current menu or drawer before opening a selector")
                        };
                        writer.running = true;
                    } else {
                        data.screenName = NacrePanelState.activeName();
                        root.request = data;
                        NacrePanelState.openMenu("home", "", false);
                    }
                } catch (failure) {
                    root.message = NacreI18n.tr("ui.menu-prompt.message.the-selector-request-could-not-be-read", "The selector request could not be read");
                }
            }
        }
    }
    Process {
        id: writer
        property string ident: ""
        property var response: ({})
        onExited: followup.restart()
        stdinEnabled: true
        command: ["python3", root.cli, "respond", ident]
        onStarted: write(JSON.stringify(response) + "\n")
        stderr: StdioCollector {
            onStreamFinished: if (text.trim())
                root.message = NacreI18n.tr("ui.menu-prompt.message.the-requesting-tool-is-no-longer-available", "The requesting tool is no longer available")
        }
    }
    Timer {
        interval: 1000
        repeat: true
        running: root.active
        onTriggered: if (Date.now() / 1000 >= root.request.expires)
            root.finish("", true, "Selector timed out")
    }
    Connections {
        target: root.ownerView
        function onLauncherChanged() {
            if (root.active && !root.ownerView.launcher)
                root.finish("", true);
        }
    }
    IpcHandler {
        target: "menuPrompt"
        function open(ident: string): void {
            root.open(ident);
        }
        function cancel(ident: string): void {
            root.cancel(ident);
        }
        function state(): string {
            return JSON.stringify({
                active: root.active,
                kind: root.request?.kind || "",
                message: root.message
            });
        }
    }
}
