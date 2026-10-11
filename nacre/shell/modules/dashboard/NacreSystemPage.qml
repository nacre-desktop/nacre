import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Quickshell
import qs.widgets
import qs.services
import "media/media.js" as Model

Flickable {
    id: root
    required property bool shouldUpdate
    property int pendingPid: 0
    property string pendingStart: ""
    implicitWidth: 940
    implicitHeight: 850
    contentWidth: width
    contentHeight: sections.height
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: NacreScrollBar {}
    FastScroll {
        view: root
    }
    Column {
        id: sections
        width: root.width
        spacing: 18
        NacreDashboardHero {
            width: parent.width
            height: root.width < 650 ? 390 : 215
            Grid {
                anchors.fill: parent
                anchors.margins: 16
                columns: root.width < 650 ? 2 : 4
                Repeater {
                    model: [
                        {
                            label: "CPU",
                            value: NacreSystemUsage.cpuPerc,
                            detail: Model.temperature(NacreSystemUsage.cpuTemp)
                        },
                        {
                            label: "Memory",
                            value: NacreSystemUsage.memPerc,
                            detail: Model.size(NacreSystemUsage.memUsed)
                        },
                        {
                            label: "GPU",
                            value: NacreSystemUsage.gpuUsageAvailable ? NacreSystemUsage.gpuPerc : NaN,
                            detail: Model.temperature(NacreSystemUsage.gpuTemp)
                        },
                        {
                            label: "Storage",
                            value: NacreSystemUsage.storagePerc,
                            detail: Model.size(NacreSystemUsage.storageUsed)
                        }
                    ]
                    NacreChamberGauge {
                        required property var modelData
                        width: parent.width / parent.columns
                        height: 185
                        label: modelData.label
                        value: modelData.value
                        detail: modelData.detail
                    }
                }
            }
        }
        RowLayout {
            width: parent.width
            NacreText {
                text: Updates.count + " package updates"
                font.pointSize: 14
                color: NacreTokens.accent
                Layout.fillWidth: true
            }
            ActionButton {
                text: "Check"
                compact: true
                onClicked: Updates.refresh()
            }
            ActionButton {
                text: "Update"
                icon: "system_update"
                compact: true
                onClicked: Quickshell.execDetached([Quickshell.env("HOME") + "/.local/share/nacre/shell/tools/control.sh", "updates"])
            }
            ActionButton {
                text: "Maintenance"
                compact: true
                onClicked: NacrePanelState.openSettings("maintenance")
            }
        }
        NacreText {
            width: parent.width
            text: Updates.message || ""
            visible: text !== ""
            font.pointSize: 10
            color: NacreTokens.mutedInk
            wrapMode: Text.Wrap
        }
        NacreText {
            text: "Network"
            font.pointSize: 15
            color: NacreTokens.accent
        }
        Canvas {
            width: parent.width
            height: 95
            property var samples: NacreDashboardSystem.network
            property color accent: NacreTokens.accent
            onSamplesChanged: requestPaint()
            onAccentChanged: requestPaint()
            onWidthChanged: requestPaint()
            onPaint: {
                const ctx = getContext("2d");
                ctx.reset();
                const samples = NacreDashboardSystem.network;
                if (samples.length < 2)
                    return;
                const max = Math.max(1024, ...samples.map(p => Math.max(p.up, p.down)));
                for (const [key, color] of [["down", NacreTokens.accent], ["up", NacreTokens.orientSecondary]]) {
                    ctx.strokeStyle = color;
                    ctx.lineWidth = 2;
                    ctx.beginPath();
                    samples.forEach((p, i) => {
                        const x = i / (samples.length - 1) * width, y = height - 8 - p[key] / max * (height - 16);
                        if (i === 0)
                            ctx.moveTo(x, y);
                        else
                            ctx.lineTo(x, y);
                    });
                    ctx.stroke();
                }
            }
        }
        NacreText {
            text: NacreDashboardSystem.network.length ? "Down " + Model.size(NacreDashboardSystem.network[NacreDashboardSystem.network.length - 1].down / 1024) + "/s    Up " + Model.size(NacreDashboardSystem.network[NacreDashboardSystem.network.length - 1].up / 1024) + "/s" : "Waiting for a network sample"
            font.pointSize: 10
            color: NacreTokens.mutedInk
        }
        NacreText {
            text: "Drives"
            font.pointSize: 15
            color: NacreTokens.accent
        }
        Repeater {
            model: NacreDashboardSystem.disks
            RowLayout {
                required property var modelData
                width: sections.width
                spacing: 14
                NacreText {
                    Layout.preferredWidth: 150
                    text: parent.modelData.path
                    font.pointSize: 10
                    elide: Text.ElideMiddle
                }
                Rectangle {
                    Layout.fillWidth: true
                    implicitHeight: 6
                    radius: 3
                    color: Qt.alpha(NacreTokens.mutedInk, .15)
                    Rectangle {
                        width: parent.width * parent.parent.modelData.used / Math.max(1, parent.parent.modelData.total)
                        height: parent.height
                        radius: 3
                        gradient: Gradient {
                            orientation: Gradient.Horizontal
                            GradientStop {
                                position: 0
                                color: NacreTokens.accent
                            }
                            GradientStop {
                                position: 1
                                color: NacreTokens.orientSecondary
                            }
                        }
                    }
                }
                NacreText {
                    text: Model.size(parent.modelData.used / 1024) + " / " + Model.size(parent.modelData.total / 1024)
                    font.pointSize: 10
                    color: NacreTokens.mutedInk
                }
            }
        }
        NacreText {
            text: "Processes · memory use"
            font.pointSize: 15
            color: NacreTokens.accent
        }
        Repeater {
            model: NacreDashboardSystem.processes
            RowLayout {
                required property var modelData
                width: sections.width
                spacing: 12
                NacreText {
                    Layout.fillWidth: true
                    text: parent.modelData.name
                    font.pointSize: 10
                    elide: Text.ElideRight
                }
                NacreText {
                    text: Model.size(parent.modelData.memory / 1024)
                    font.pointSize: 10
                    color: NacreTokens.mutedInk
                }
                ActionButton {
                    text: root.pendingPid === parent.modelData.pid && root.pendingStart === parent.modelData.start ? "Confirm end" : "End"
                    compact: true
                    onClicked: {
                        if (root.pendingPid === parent.modelData.pid && root.pendingStart === parent.modelData.start) {
                            NacreDashboardSystem.end(parent.modelData.pid, parent.modelData.start);
                            root.pendingPid = 0;
                        } else {
                            root.pendingPid = parent.modelData.pid;
                            root.pendingStart = parent.modelData.start;
                            confirmTimeout.restart();
                        }
                    }
                }
            }
        }
        Timer {
            id: confirmTimeout
            interval: 5000
            onTriggered: root.pendingPid = 0
        }
        NacreText {
            width: parent.width
            text: NacreDashboardSystem.message
            visible: text !== ""
            font.pointSize: 10
            color: NacreTokens.mutedInk
            wrapMode: Text.Wrap
        }
        NacreText {
            text: "Kernel " + (NacreSystemUsage.kernel || "—") + "    Load " + (NacreSystemUsage.loadAverage || "—")
            font.pointSize: 9
            color: NacreTokens.mutedInk
        }
    }
}
