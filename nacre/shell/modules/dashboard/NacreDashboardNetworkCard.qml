import QtQuick
import qs.widgets
import qs.services

NacreSurface {
    id: root
    readonly property bool dense: height < 230
    readonly property var samples: NacreDashboardSystem.network || []
    readonly property real down: samples.length ? samples[samples.length - 1].down : 0
    readonly property real up: samples.length ? samples[samples.length - 1].up : 0
    readonly property real peak: Math.max(0, ...samples.map(p => Math.max(p.up, p.down)))
    function rate(value) {
        const mib = value / 1048576;
        return NacreI18n.tr("dashboard.network-card.rate", "{value} {unit}/s", {
            value: mib >= 1 ? mib.toFixed(1) : (value / 1024).toFixed(0),
            unit: mib >= 1 ? "MiB" : "KiB"
        });
    }
    radius: 16
    color: NacreTokens.raised
    NacreText {
        x: 16
        y: 14
        text: NacreI18n.tr("dashboard.network-card.title", "Network")
        font.pointSize: root.dense ? 11 : 13
        font.weight: Font.DemiBold
    }
    NacreText {
        x: 16
        y: root.dense ? 36 : 40
        width: parent.width - 32
        text: (NacreDashboardSystem.interfaceName === NacreNetwork.wifiInterface ? NacreNetwork.active?.ssid : "") || NacreDashboardSystem.interfaceName || NacreI18n.tr("dashboard.network-card.interfaces", "All active interfaces")
        color: NacreTokens.mutedInk
        font.pointSize: 9
        elide: Text.ElideRight
    }
    Row {
        x: 16
        y: root.dense ? 58 : 68
        spacing: 16
        NacreText {
            text: NacreI18n.tr("dashboard.network-card.down", "Down {rate}", {
                rate: root.rate(root.down)
            })
            color: NacreTokens.accent
            font.pointSize: root.dense ? 9 : 11
        }
        NacreText {
            text: NacreI18n.tr("dashboard.network-card.up", "Up {rate}", {
                rate: root.rate(root.up)
            })
            color: NacreTokens.orientSecondary
            font.pointSize: root.dense ? 9 : 11
        }
    }
    NacreDashboardSparkline {
        x: 16
        y: root.dense ? 82 : 98
        width: parent.width - 32
        height: Math.max(20, parent.height - y - 35)
        values: root.samples.map(p => p.down)
        secondaryValues: root.samples.map(p => p.up)
        maximum: Math.max(1024, root.peak)
    }
    NacreText {
        x: 16
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 12
        text: NacreI18n.tr("dashboard.network-card.peak", "Peak {rate}", {
            rate: root.rate(root.peak)
        })
        font.pointSize: 9
        color: NacreTokens.mutedInk
    }
}
