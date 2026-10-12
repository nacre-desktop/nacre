import QtQuick
import Quickshell
import qs.widgets
import qs.services
import "overview" as Cards

Item {
    id: root
    property bool active: false
    signal systemRequested
    readonly property bool narrow: width < 650
    readonly property real gap: 16
    readonly property real calendarWidth: width * .31
    readonly property real hostWidth: width * .38 - gap
    readonly property real mediaWidth: width - calendarWidth - hostWidth - gap * 2
    readonly property real gaugeHeight: Math.max(92, Math.min(112, width / 12))
    implicitHeight: narrow ? media.y + media.height : 300
    Cards.NacreCalendarCard {
        id: calendar
        objectName: "dashboardCalendar"
        width: root.narrow ? root.width : root.calendarWidth
        height: root.narrow ? 320 : root.height
        color: NacreTokens.raised
    }
    NacreSurface {
        objectName: "dashboardSystemSurface"
        x: host.x
        y: host.y
        width: host.width
        height: host.height + 12 + metrics.height
        radius: 18
        color: NacreTokens.raised
    }
    Cards.NacreHostCard {
        id: host
        objectName: "dashboardHost"
        x: root.narrow ? 0 : calendar.width + root.gap
        y: root.narrow ? calendar.height + root.gap : 0
        width: root.narrow ? root.width : root.hostWidth
        height: root.narrow ? 210 : Math.max(176, root.height - root.gaugeHeight - 12)
        active: root.active
        updateCount: Updates.count
        kernel: NacreSystemUsage.kernel
        color: "transparent"
        onSystemRequested: root.systemRequested()
        onUpdateRequested: Quickshell.execDetached([Quickshell.env("HOME") + "/.local/share/nacre/shell/tools/control.sh", "updates"])
    }
    Row {
        id: metrics
        objectName: "dashboardMetrics"
        x: host.x
        y: root.narrow ? host.y + host.height + 12 : root.height - height
        width: host.width
        height: root.gaugeHeight
        Repeater {
            model: [
                {
                    label: "CPU",
                    value: NacreSystemUsage.cpuPerc
                },
                {
                    label: "Memory",
                    value: NacreSystemUsage.memPerc
                },
                {
                    label: "Storage",
                    value: NacreSystemUsage.storagePerc
                }
            ]
            NacreCompactMetric {
                required property var modelData
                width: metrics.width / 3
                height: metrics.height
                label: modelData.label
                value: modelData.value
            }
        }
    }
    Cards.NacreMediaCard {
        id: media
        objectName: "dashboardMedia"
        x: root.narrow ? 0 : host.x + host.width + root.gap
        y: root.narrow ? metrics.y + metrics.height + root.gap : 0
        width: root.narrow ? root.width : root.mediaWidth
        height: root.narrow ? 300 : root.height
        active: root.active
        compact: true
        audioHalo: true
        spectrum: NacreMediaDetails.spectrum
        color: NacreTokens.raised
    }
}
