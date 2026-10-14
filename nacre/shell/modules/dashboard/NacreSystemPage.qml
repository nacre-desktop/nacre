import QtQuick
import QtQuick.Controls.Basic
import Quickshell
import Quickshell.Io
import Quickshell.Services.UPower
import qs.utils
import qs.widgets
import qs.services
import "media/media.js" as Model

Item {
    id: root
    required property bool shouldUpdate
    readonly property bool compact: width < 680
    readonly property var battery: UPower.displayDevice
    readonly property bool laptop: !!battery && battery.ready && battery.isLaptopBattery && battery.isPresent !== false
    property int diskIndex: 0
    readonly property var selectedDisk: (NacreDashboardSystem.filesystems || [])[diskIndex] || null
    property bool sortCpu: false
    property bool profilesAvailable: false
    Process {
        id: profileProbe
        command: ["powerprofilesctl", "get"]
        onExited: code => root.profilesAvailable = code === 0
    }
    readonly property var processes: [...NacreDashboardSystem.processes].sort((a, b) => sortCpu ? (NacreDashboardSystem.cpuByPid[b.pid + ":" + b.start] || 0) - (NacreDashboardSystem.cpuByPid[a.pid + ":" + a.start] || 0) : b.memory - a.memory).slice(0, 5)
    readonly property real metricsHeight: compact ? Math.min(230, height * .43) : Math.min(150, height * .28)
    readonly property real updateHeight: laptop ? 88 : 64
    readonly property real bottomY: metricsHeight + 16
    readonly property real bottomHeight: height - bottomY - updateHeight - 12
    implicitWidth: 940
    implicitHeight: 562
    Component.onCompleted: if (laptop && shouldUpdate) {
        profileProbe.running = true;
        NacreControlTools.refresh();
    }
    NacreGpuMemory {
        id: gpuMemory
        active: root.shouldUpdate
        gpu: [...(NacreSystemUsage.gpus || [])].sort((a, b) => (b.usage || 0) - (a.usage || 0))[0] || null
    }
    NacreSurface {
        width: parent.width
        height: root.metricsHeight
        radius: 20
        color: NacreTokens.raised
    }
    Grid {
        width: parent.width
        height: root.metricsHeight
        columns: root.compact ? 2 : 4
        spacing: 12
        NacreDashboardMetric {
            color: "transparent"
            width: (parent.width - (parent.columns - 1) * parent.spacing) / parent.columns
            height: root.compact ? (root.metricsHeight - 12) / 2 : root.metricsHeight
            label: NacreI18n.tr("dashboard.system-page.label.cpu", "CPU")
            icon: "memory"
            fraction: NacreSystemUsage.cpuPerc
            value: Number.isFinite(NacreSystemUsage.cpuTemp) ? Model.temperature(NacreSystemUsage.cpuTemp) : "—"
            detail: NacreI18n.tr("dashboard.system-page.temperature", "Temperature")
            history: NacreSystemUsage.cpuHistory || []
        }
        NacreDashboardMetric {
            color: "transparent"
            width: (parent.width - (parent.columns - 1) * parent.spacing) / parent.columns
            height: root.compact ? (root.metricsHeight - 12) / 2 : root.metricsHeight
            label: NacreI18n.tr("dashboard.system-page.label.memory", "Memory")
            icon: "storage"
            value: Model.size(NacreSystemUsage.memUsed)
            detail: NacreI18n.tr("dashboard.system-page.total", "{value} total", {
                value: Model.size(NacreSystemUsage.memTotal)
            })
            history: NacreSystemUsage.memoryHistory || []
            fraction: NacreSystemUsage.memPerc
        }
        NacreDashboardMetric {
            color: "transparent"
            width: (parent.width - (parent.columns - 1) * parent.spacing) / parent.columns
            height: root.compact ? (root.metricsHeight - 12) / 2 : root.metricsHeight
            label: NacreI18n.tr("dashboard.system-page.label.gpu", "GPU")
            icon: "developer_board"
            value: gpuMemory.display
            valueSize: root.compact ? 11 : 12
            detail: Number.isFinite(NacreSystemUsage.gpuTemp) ? Model.temperature(NacreSystemUsage.gpuTemp) : NacreI18n.tr("dashboard.system-page.gpu-unavailable", "Unavailable")
            fraction: NacreSystemUsage.gpuUsageAvailable ? NacreSystemUsage.gpuPerc : NaN
        }
        NacreDashboardMetric {
            id: diskTile
            color: "transparent"
            width: (parent.width - (parent.columns - 1) * parent.spacing) / parent.columns
            height: root.compact ? (root.metricsHeight - 12) / 2 : root.metricsHeight
            label: NacreI18n.tr("dashboard.system-page.label.storage", "Storage") + (root.selectedDisk && root.selectedDisk.path !== "/" ? " " + root.selectedDisk.path : "")
            icon: "hard_drive"
            value: Model.size(root.selectedDisk ? root.selectedDisk.used / 1024 : NacreSystemUsage.storageUsed)
            detail: NacreI18n.tr("dashboard.system-page.total", "{value} total", {
                value: Model.size(root.selectedDisk ? root.selectedDisk.total / 1024 : NacreSystemUsage.storageTotal)
            })
            fraction: root.selectedDisk ? root.selectedDisk.used / Math.max(1, root.selectedDisk.total) : NacreSystemUsage.storagePerc
            ActionButton {
                x: parent.width - 42
                y: 9
                width: 28
                height: 26
                compact: true
                text: ""
                icon: "expand_more"
                enabled: (NacreDashboardSystem.filesystems || []).length > 0
                accessibleLabel: NacreI18n.tr("dashboard.system-page.choose-filesystem", "Choose filesystem")
                onClicked: disks.open()
            }
            Popup {
                id: disks
                x: Math.min(0, diskTile.width - width)
                y: diskTile.height + 6
                width: Math.min(300, root.width - 44)
                padding: 8
                background: NacreSurface {
                    radius: 14
                    color: NacreTokens.raised
                }
                contentItem: ListView {
                    implicitHeight: Math.min(contentHeight, Math.max(100, root.height - diskTile.height - 24))
                    clip: true
                    model: NacreDashboardSystem.filesystems || []
                    delegate: ActionButton {
                        required property var modelData
                        required property int index
                        width: parent.width
                        height: 34
                        text: modelData.path
                        selected: index === root.diskIndex
                        compact: true
                        onClicked: {
                            root.diskIndex = index;
                            disks.close();
                        }
                    }
                }
            }
        }
    }
    NacreDashboardNetworkCard {
        id: network
        x: 0
        y: root.bottomY
        width: root.compact ? parent.width * .47 : parent.width * .38
        height: root.bottomHeight
    }
    NacreSurface {
        id: activity
        x: network.width + 14
        y: root.bottomY
        width: parent.width - x
        height: root.bottomHeight
        radius: 16
        color: NacreTokens.raised
        NacreText {
            x: 16
            y: 14
            text: NacreI18n.tr("dashboard.system-page.top-processes", "Top processes")
            font.pointSize: root.compact ? 10 : 13
            font.weight: Font.DemiBold
        }
        Row {
            anchors.right: parent.right
            anchors.rightMargin: 12
            y: 10
            spacing: 4
            ActionButton {
                text: NacreI18n.tr("dashboard.system-page.label.cpu", "CPU")
                compact: true
                selected: root.sortCpu
                onClicked: root.sortCpu = true
            }
            ActionButton {
                text: NacreI18n.tr("dashboard.system-page.label.memory", "Memory")
                compact: true
                selected: !root.sortCpu
                onClicked: root.sortCpu = false
            }
        }
        Column {
            x: 16
            y: root.compact ? 44 : 54
            width: parent.width - 32
            spacing: root.compact ? 3 : 5
            Repeater {
                model: root.processes
                Item {
                    required property var modelData
                    width: parent.width
                    height: root.compact ? 17 : 25
                    NacreText {
                        width: parent.width * .65
                        text: modelData.name
                        font.pointSize: root.compact ? 9 : 10
                        elide: Text.ElideRight
                    }
                    NacreText {
                        anchors.right: parent.right
                        text: root.sortCpu ? (NacreDashboardSystem.cpuByPid[modelData.pid + ":" + modelData.start] === undefined ? "—" : NacreI18n.tr("dashboard.system-page.percent", "{value}%", {
                                value: NacreDashboardSystem.cpuByPid[modelData.pid + ":" + modelData.start].toFixed(1)
                            })) : Model.size(modelData.memory / 1024)
                        font.pointSize: 9
                        color: NacreTokens.mutedInk
                    }
                }
            }
        }
    }
    NacreSurface {
        y: parent.height - root.updateHeight
        width: parent.width
        height: root.updateHeight
        radius: 16
        color: NacreTokens.raised
        Column {
            x: 16
            y: 10
            width: parent.width * .42
            spacing: 3
            NacreText {
                text: Updates.count === 0 ? NacreI18n.tr("dashboard.system-page.up-to-date", "Up to date") : NacreI18n.tr("dashboard.system-page.updates-count", "{count} updates", {
                    count: Updates.count
                })
                color: NacreTokens.ink
                font.pointSize: 12
                font.weight: Font.DemiBold
            }
            NacreText {
                width: parent.width
                text: Updates.checked > 0 ? NacreI18n.tr("dashboard.system-page.last-checked", "Checked {time}", {
                    time: NacreI18n.dateTime(new Date(Updates.checked * 1000), "ddd HH:mm")
                }) : Updates.message || NacreI18n.tr("dashboard.system-page.not-checked", "Not checked yet")
                font.pointSize: 9
                color: NacreTokens.mutedInk
                elide: Text.ElideRight
            }
        }
        Row {
            anchors.right: parent.right
            anchors.rightMargin: 12
            y: 12
            spacing: 6
            ActionButton {
                text: NacreI18n.tr("dashboard.system-page.text.check", "Check")
                compact: true
                onClicked: Updates.refresh()
            }
            ActionButton {
                text: NacreI18n.tr("dashboard.system-page.text.update", "Update")
                compact: true
                onClicked: Quickshell.execDetached([NacrePaths.tools + "/control.sh", "updates"])
            }
            ActionButton {
                text: root.compact ? "" : NacreI18n.tr("dashboard.system-page.text.maintenance", "Maintenance")
                icon: root.compact ? "build" : ""
                accessibleLabel: NacreI18n.tr("dashboard.system-page.text.maintenance", "Maintenance")
                compact: true
                onClicked: NacrePanelState.openSettings("maintenance")
            }
        }
        Row {
            x: 16
            y: 54
            spacing: 12
            visible: root.laptop
            NacreText {
                text: NacreI18n.tr("dashboard.system-page.battery-percent", "Battery {value}%", {
                    value: Math.round((root.battery?.percentage || 0) * 100)
                }) + " · " + (UPower.onBattery ? NacreI18n.tr("dashboard.system-page.on-battery", "On battery") : NacreI18n.tr("dashboard.system-page.connected-power", "Connected to power"))
                color: NacreTokens.mutedInk
                font.pointSize: 9
                anchors.verticalCenter: parent.verticalCenter
            }
            ComboBox {
                id: powerProfile
                objectName: "systemPowerProfile"
                width: root.compact ? 125 : 160
                height: 26
                visible: root.profilesAvailable
                textRole: "name"
                model: [
                    {
                        name: NacreI18n.tr("dashboard.system-page.power-saver", "Power saver"),
                        value: PowerProfile.PowerSaver
                    },
                    {
                        name: NacreI18n.tr("dashboard.system-page.balanced", "Balanced"),
                        value: PowerProfile.Balanced
                    },
                    ...(PowerProfiles.hasPerformanceProfile ? [
                            {
                                name: NacreI18n.tr("dashboard.system-page.performance", "Performance"),
                                value: PowerProfile.Performance
                            }
                        ] : [])]
                currentIndex: model.findIndex(item => item.value === PowerProfiles.profile)
                Accessible.name: NacreI18n.tr("dashboard.system-page.power-profile", "Power profile")
                onActivated: index => {
                    if (root.shouldUpdate)
                        PowerProfiles.profile = model[index].value;
                }
                background: Rectangle {
                    radius: 13
                    color: Qt.alpha(NacreTokens.accent, .08)
                    border.width: powerProfile.activeFocus ? 2 : 0
                    border.color: NacreTokens.accent
                }
                contentItem: NacreText {
                    text: powerProfile.displayText
                    leftPadding: 10
                    rightPadding: 24
                    verticalAlignment: Text.AlignVCenter
                    color: NacreTokens.mutedInk
                    font.pointSize: 9
                    elide: Text.ElideRight
                }
                indicator: NacreIcon {
                    x: powerProfile.width - 24
                    anchors.verticalCenter: parent.verticalCenter
                    text: "expand_more"
                    font.pointSize: 12
                    color: NacreTokens.mutedInk
                }
                delegate: ItemDelegate {
                    required property var modelData
                    required property int index
                    width: powerProfile.width
                    highlighted: powerProfile.highlightedIndex === index
                    contentItem: NacreText {
                        text: parent.modelData.name
                        font.pointSize: 10
                        color: NacreTokens.ink
                    }
                    background: Rectangle {
                        color: parent.highlighted ? Qt.alpha(NacreTokens.accent, .12) : NacreTokens.raised
                    }
                }
                popup: Popup {
                    y: -implicitHeight - 4
                    width: powerProfile.width
                    padding: 6
                    implicitHeight: contentItem.implicitHeight + 12
                    background: NacreSurface {
                        radius: 14
                        color: NacreTokens.raised
                    }
                    contentItem: ListView {
                        implicitHeight: contentHeight
                        model: powerProfile.popup.visible ? powerProfile.delegateModel : null
                        currentIndex: powerProfile.highlightedIndex
                        clip: true
                    }
                }
            }
        }
    }
}
