import QtQuick
import Quickshell.Services.Pipewire
import qs.services

QtObject {
    id: root
    required property var player
    property bool active: false
    readonly property real level: active && meter.node ? Math.max(0, Math.min(1, meter.peak)) : 0
    property PwNodePeakMonitor meter: PwNodePeakMonitor {
        node: root.active ? NacrePlayers.streamFor(root.player) : null
        enabled: root.active && node !== null
    }
}
