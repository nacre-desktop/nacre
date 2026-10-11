import QtQuick
import qs.widgets
import qs.services

NacreSettingsPage {
    id: root
    property string page: "maintenance"
    readonly property var state: Maintenance.data
    readonly property var serviceNames: ({
            "nacre-shell.service": "Desktop",
            "nacre-sidebar-ai.service": "AI chat",
            "nacre-brain.service": "Brain",
            "nacre-session-watch.service": "Session monitor",
            "hypridle.service": "Sleep locking",
            "nacre-brain-sync.timer": "Knowledge sync",
            "xdg-desktop-portal.service": "Desktop portal",
            "xdg-desktop-portal-hyprland.service": "Screen sharing",
            "xdg-document-portal.service": "File access"
        })
    Component.onCompleted: Maintenance.refresh()
    function act(action) {
        if (!Maintenance.busy)
            Maintenance.request(action);
    }
    function recovery(action) {
        if (!Maintenance.busy)
            Maintenance.recover(action);
    }
    NacreSettingsSection {
        title: "Desktop health"
        NacreText {
            width: parent.width
            text: "Installed release: " + (root.state.release?.revision?.slice(0, 12) || "Unknown")
            font.pointSize: 11
            wrapMode: Text.Wrap
        }
        Flow {
            width: parent.width
            spacing: 16
            Repeater {
                model: Object.entries(root.state.services || {})
                NacreText {
                    required property var modelData
                    text: (root.serviceNames[modelData[0]] || modelData[0]) + ": " + modelData[1]
                    font.pointSize: 10
                    color: NacreTokens.mutedInk
                }
            }
        }
        NacreText {
            width: parent.width
            text: root.state.configErrors ? "Compositor errors: " + root.state.configErrors : "Compositor configuration is valid"
            font.pointSize: 11
            wrapMode: Text.Wrap
        }
        NacreText {
            width: parent.width
            text: (root.state.drift || []).length + " locally changed managed files"
            font.pointSize: 10
            color: NacreTokens.mutedInk
            wrapMode: Text.Wrap
        }
        NacreText {
            width: parent.width
            text: "Knowledge sync: " + (root.state.brainSync?.message || root.state.brainSync?.state || "Not recorded")
            font.pointSize: 10
            color: NacreTokens.mutedInk
            wrapMode: Text.Wrap
        }
        NacreText {
            width: parent.width
            text: root.state.performance?.cpuCorePercent !== undefined ? "Last sample: " + root.state.performance.cpuCorePercent + "% of one core · " + root.state.performance.memoryMiB + " MiB (" + (root.state.performance.label || "manual") + ")" : "No resource sample recorded"
            font.pointSize: 10
            color: NacreTokens.mutedInk
            wrapMode: Text.Wrap
        }
        NacreText {
            width: parent.width
            text: "Session: " + (root.state.session?.event || "Not recorded") + " · Update cache: " + (root.state.updatesAgeSeconds === null || root.state.updatesAgeSeconds === undefined ? "not recorded" : Math.floor(root.state.updatesAgeSeconds / 60) + " minutes old")
            font.pointSize: 10
            color: NacreTokens.mutedInk
            wrapMode: Text.Wrap
        }
        Flow {
            width: parent.width
            spacing: 8
            ActionButton {
                text: "Refresh status"
                enabled: !Maintenance.busy
                onClicked: Maintenance.refresh()
            }
            ActionButton {
                text: "Sample resource use"
                enabled: !Maintenance.busy
                onClicked: root.act("profile")
            }
            ActionButton {
                text: "Check session"
                enabled: !Maintenance.busy
                onClicked: root.act("session")
            }
            ActionButton {
                text: "Retry knowledge sync"
                enabled: !Maintenance.busy
                onClicked: root.act("sync-now")
            }
            ActionButton {
                text: "Repair failed portals"
                enabled: !Maintenance.busy
                onClicked: root.act("repair-portals")
            }
            ActionButton {
                text: "Restart desktop"
                enabled: !Maintenance.busy
                onClicked: root.recovery("restart")
            }
            ActionButton {
                text: "Restore previous release"
                enabled: !Maintenance.busy
                onClicked: root.recovery("rollback")
            }
        }
        NacreText {
            width: parent.width
            text: Maintenance.message || ""
            font.pointSize: 10
            color: NacreTokens.accent
            wrapMode: Text.Wrap
        }
        NacreText {
            width: parent.width
            text: root.state.checked ? "Checked " + new Date(root.state.checked).toLocaleString() : ""
            font.pointSize: 9
            color: NacreTokens.mutedInk
            wrapMode: Text.Wrap
        }
    }
}
