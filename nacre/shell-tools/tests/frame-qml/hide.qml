import QtQuick
import Quickshell
import Quickshell.Wayland
import qs.services
import qs.config
import qs.modules.background
import qs.modules.drawers

ShellRoot {
    id: root
    property int step: 0
    property var original: null
    NacreBackground {}
    NacreReservedEdges {
        screen: Quickshell.screens[0]
    }
    function check(value, message) {
        if (!value) {
            console.error(message);
            Qt.exit(1);
        }
    }
    Timer {
        interval: 50
        running: true
        repeat: true
        onTriggered: {
            const background = Environment.windows.filter(w => w.name === "background");
            const edges = Environment.windows.filter(w => w.name === "border-exclusion");
            root.check(background.length === 1 && edges.length === 4, "Expected wallpaper and all four edge reservations");
            if (root.step === 0) {
                root.original = background[0];
                root.check(edges.map(w => w.exclusiveZone).join(",") === "50,10,10,10", "Initial frame reservation missing");
                NacrePanelState.hidden = true;
            } else if (root.step % 2 === 1) {
                root.check(background[0] === root.original && background[0].visible, "Hiding shell removed or recreated wallpaper surface");
                root.check(Environment.scenesCreated === 1, "Hiding shell recreated wallpaper playback");
                root.check(edges.every(w => w.exclusiveZone === 0 && !w.visible), "Hidden frame still reserves tile space");
                NacrePanelState.hidden = false;
            } else {
                root.check(background[0].visible && Environment.scenesCreated === 1, "Showing shell disturbed wallpaper");
                root.check(edges.every(w => w.visible), "Frame reservations did not return");
                root.check(edges.map(w => w.exclusiveZone).join(",") === "50,10,10,10", "Restored frame changed configured extents");
                if (root.step === 4) {
                    console.log("HIDE_NATIVE_OK");
                    Qt.quit();
                    return;
                }
                NacrePanelState.hidden = true;
            }
            root.step++;
        }
    }
}
