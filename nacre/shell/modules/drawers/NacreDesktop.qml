import QtQuick
import Quickshell
import Quickshell.Io
import qs.services

Scope {
    Variants {
        model: DisplayRecovery.surfaceScreens
        NacreScreen {
            required property ShellScreen modelData
            screen: modelData
            outputName: modelData.name
        }
    }
    IpcHandler {
        target: "leftEdge"
        function state(): string {
            const name = NacreHyprland.focusedMonitor?.name ?? Object.keys(NacrePanelState.panels)[0];
            const input = NacrePanelState.panels[name]?.input;
            return JSON.stringify({
                registered: !!input,
                visible: input?.leftEdgeAvailable ?? false,
                width: input?.leftEdgeRect.width ?? 0,
                height: input?.leftEdgeRect.height ?? 0
            });
        }
    }
    IpcHandler {
        target: "frame"
        function state(): string {
            return JSON.stringify(Object.entries(NacrePanelState.panels).map(entry => Object.assign({
                    screen: entry[0]
                }, entry[1].input.describe())));
        }
    }
}
