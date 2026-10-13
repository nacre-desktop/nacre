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
    readonly property real metricsHeight: compact ? Math.min(270, height * (laptop ? .43 : .47)) : Math.min(laptop ? 160 : 182, height * .33)
    readonly property real updateHeight: laptop ? 100 : 64
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
    Grid {
        width: parent.width
        height: root.metricsHeight
        columns: root.compact ? 2 : root.laptop ? 5 : 4
        spacing: 12
        NacreDashboardMetric {
            width: (parent.width - (parent.columns - 1) * parent.spacing) / parent.columns
            height: root.compact ? (root.metricsHeight - 12) / 2 : root.metricsHeight
            label: NacreI18n.tr("dashboard.system-page.label.cpu", "CPU")
            icon: "memory"
            value: Number.isFinite(NacreSystemUsage.cpuPerc) ? NacreI18n.tr("dashboard.system-page.percent", "{value}%", {
                value: Math.round(NacreSystemUsage.cpuPerc * 100)
            }) : "—"
            detail: Model.temperature(NacreSystemUsage.cpuTemp)
            history: NacreSystemUsage.cpuHistory || []
        }
        NacreDashboardMetric {
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
            width: (parent.width - (parent.columns - 1) * parent.spacing) / parent.columns
            height: root.compact ? (root.metricsHeight - 12) / 2 : root.metricsHeight
            label: NacreI18n.tr("dashboard.system-page.label.gpu", "GPU")
            icon: "developer_board"
            value: NacreSystemUsage.gpuUsageAvailable ? NacreI18n.tr("dashboard.system-page.percent", "{value}%", {
                value: Math.round(NacreSystemUsage.gpuPerc * 100)
            }) : "—"
            detail: Model.temperature(NacreSystemUsage.gpuTemp) + " · " + gpuMemory.display
            fraction: NacreSystemUsage.gpuUsageAvailable ? NacreSystemUsage.gpuPerc : 0
        }
        NacreDashboardMetric {
            id: diskTile
            width: (parent.width - (parent.columns - 1) * parent.spacing) / parent.columns
            height: root.compact ? (root.metricsHeight - 12) / 2 : root.metricsHeight
            label: NacreI18n.tr("dashboard.system-page.label.storage", "Storage")
            icon: "hard_drive"
            value: Model.size(root.selectedDisk ? root.selectedDisk.used / 1024 : NacreSystemUsage.storageUsed)
            detail: NacreI18n.tr("dashboard.system-page.total", "{value} total", {
                value: Model.size(root.selectedDisk ? root.selectedDisk.total / 1024 : NacreSystemUsage.storageTotal)
            })
            fraction: root.selectedDisk ? root.selectedDisk.used / Math.max(1, root.selectedDisk.total) : NacreSystemUsage.storagePerc
            ActionButton {
                x: 14
                y: parent.height - (root.compact ? 32 : 56)
                width: parent.width - 28
                height: 26
                compact: true
                text: (root.selectedDisk?.path || "/").slice(0, 20)
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
        NacreDashboardMetric {
            visible: root.laptop && !root.compact
            width: (parent.width - (parent.columns - 1) * parent.spacing) / parent.columns
            height: root.metricsHeight
            label: NacreI18n.tr("dashboard.system-page.battery", "Battery")
            icon: UPower.onBattery ? "battery_full" : "power"
            value: NacreI18n.tr("dashboard.system-page.percent", "{value}%", {
                value: Math.round((root.battery?.percentage || 0) * 100)
            })
            detail: UPower.onBattery ? NacreI18n.tr("dashboard.system-page.on-battery", "On battery") : NacreI18n.tr("dashboard.system-page.connected-power", "Connected to power")
            fraction: root.battery?.percentage || 0
        }
    }
    NacreDashboardNetworkCard {
        id: network
        x: 0
        y: root.bottomY
        width: root.compact ? parent.width * .47 : parent.width * .39
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
        Column {
            x: 16
            y: Math.max(210, parent.height - 75)
            width: parent.width - 32
            spacing: 7
            visible: parent.height >= 260
            Repeater {
                model: (NacreDashboardSystem.filesystems || NacreDashboardSystem.disks).slice(0, 2)
                Item {
                    required property var modelData
                    width: parent.width
                    height: 27
                    NacreText {
                        text: modelData.path
                        width: parent.width * .5
                        font.pointSize: 9
                        elide: Text.ElideMiddle
                    }
                    NacreText {
                        anchors.right: parent.right
                        text: NacreI18n.tr("dashboard.system-page.drive-use", "{used}/{total}", {
                            used: Model.size(modelData.used / 1024),
                            total: Model.size(modelData.total / 1024)
                        })
                        color: NacreTokens.mutedInk
                        font.pointSize: 9
                    }
                    Rectangle {
                        anchors.bottom: parent.bottom
                        width: parent.width
                        height: 4
                        radius: 2
                        color: Qt.alpha(NacreTokens.accent, .12)
                        Rectangle {
                            width: parent.width * Math.max(0, Math.min(1, parent.parent.modelData.used / Math.max(1, parent.parent.modelData.total)))
                            height: parent.height
                            radius: 2
                            color: NacreTokens.accent
                        }
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
            width: parent.width * .45
            spacing: 3
            NacreText {
                text: NacreI18n.tr("dashboard.system-page.updates-count", "{count} updates", {
                    count: Updates.count
                })
                color: NacreTokens.accent
                font.pointSize: 13
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
                text: NacreI18n.tr("dashboard.system-page.text.maintenance", "Maintenance")
                compact: true
                onClicked: NacrePanelState.openSettings("maintenance")
            }
        }
        Row {
            x: 16
            y: 60
            spacing: 6
            visible: root.laptop
            NacreText {
                text: (root.compact ? NacreI18n.tr("dashboard.system-page.battery-percent", "Battery {value}%", {
                        value: Math.round((root.battery?.percentage || 0) * 100)
                    }) + " · " : "") + (UPower.onBattery ? NacreI18n.tr("dashboard.system-page.on-battery", "On battery") : NacreI18n.tr("dashboard.system-page.connected-power", "Connected to power"))
                color: NacreTokens.mutedInk
                font.pointSize: 9
                anchors.verticalCenter: parent.verticalCenter
            }
            Repeater {
                model: [
                    {
                        name: NacreI18n.tr("dashboard.system-page.power-saver", "Power saver"),
                        value: PowerProfile.PowerSaver
                    },
                    {
                        name: NacreI18n.tr("dashboard.system-page.balanced", "Balanced"),
                        value: PowerProfile.Balanced
                    },
                    {
                        name: NacreI18n.tr("dashboard.system-page.performance", "Performance"),
                        value: PowerProfile.Performance
                    }
                ]
                ActionButton {
                    required property var modelData
                    text: root.compact ? "" : modelData.name
                    icon: modelData.value === PowerProfile.PowerSaver ? "eco" : modelData.value === PowerProfile.Performance ? "bolt" : "balance"
                    accessibleLabel: modelData.name
                    compact: true
                    selected: PowerProfiles.profile === modelData.value
                    enabled: root.profilesAvailable && (modelData.value !== PowerProfile.Performance || PowerProfiles.hasPerformanceProfile)
                    onClicked: PowerProfiles.profile = modelData.value
                }
            }
        }
    }
}
