import QtQuick
import qs.widgets
import qs.services

NacreSettingsPage {
    id: root
    property string page: "maintenance"
    property string pendingRecovery: ""
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
            "xdg-desktop-portal-gnome.service": "Screen sharing",
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
        visible: !!root.pendingRecovery
        title: root.pendingRecovery === "restart" ? NacreI18n.tr("settings.registry.ui.confirm-restart", "Restart the desktop?") : NacreI18n.tr("settings.registry.ui.confirm-rollback", "Restore the previous desktop release?")
        Row {
            spacing: NacreTokens.space.sm
            ActionButton {
                text: NacreI18n.tr("settings.registry.ui.confirm", "Confirm")
                onClicked: {
                    root.recovery(root.pendingRecovery);
                    root.pendingRecovery = "";
                }
            }
            ActionButton {
                text: NacreI18n.tr("settings.registry.ui.cancel", "Cancel")
                onClicked: root.pendingRecovery = ""
            }
        }
    }
    NacreSettingsSection {
        title: NacreI18n.tr("ui.maintenance-page.title.desktop-health", "Desktop health")
        NacreText {
            width: parent.width
            text: NacreI18n.tr("ui.maintenance-page.text.installed-release-value0", "Installed release: {value0}", {
                value0: (root.state.release?.revision?.slice(0, 12) || NacreI18n.tr("ui.maintenance-page.text.unknown", "Unknown"))
            })
            font.pointSize: NacreTokens.textSize.body
            wrapMode: Text.Wrap
        }
        Flow {
            width: parent.width
            spacing: NacreTokens.space.lg
            Repeater {
                model: Object.entries(root.state.services || {})
                NacreText {
                    required property var modelData
                    text: (root.serviceNames[modelData[0]] || modelData[0]) + ": " + modelData[1]
                    font.pointSize: NacreTokens.textSize.caption
                    color: NacreTokens.mutedInk
                }
            }
        }
        NacreText {
            width: parent.width
            text: root.state.configErrors ? NacreI18n.tr("ui.maintenance-page.text.compositor-errors-value0", "Compositor errors: {value0}", {
                value0: root.state.configErrors
            }) : NacreI18n.tr("ui.maintenance-page.text.compositor-configuration-is-valid", "Compositor configuration is valid")
            font.pointSize: NacreTokens.textSize.body
            wrapMode: Text.Wrap
        }
        NacreText {
            width: parent.width
            text: NacreI18n.tr("ui.maintenance-page.text.value0-locally-changed-managed-files", "{value0} locally changed managed files", {
                value0: (root.state.drift || []).length
            })
            font.pointSize: NacreTokens.textSize.caption
            color: NacreTokens.mutedInk
            wrapMode: Text.Wrap
        }
        NacreText {
            width: parent.width
            text: NacreI18n.tr("ui.maintenance-page.text.knowledge-sync-value0", "Knowledge sync: {value0}", {
                value0: (root.state.brainSync?.message || root.state.brainSync?.state || NacreI18n.tr("ui.maintenance-page.text.not-recorded", "Not recorded"))
            })
            font.pointSize: NacreTokens.textSize.caption
            color: NacreTokens.mutedInk
            wrapMode: Text.Wrap
        }
        NacreText {
            width: parent.width
            text: root.state.performance?.cpuCorePercent !== undefined ? NacreI18n.tr("ui.maintenance-page.text.last-sample-value0-of-one-core-value1-mib-value2", "Last sample: {value0}% of one core · {value1} MiB ({value2})", {
                value0: root.state.performance.cpuCorePercent,
                value1: root.state.performance.memoryMiB,
                value2: (root.state.performance.label || "manual")
            }) : NacreI18n.tr("ui.maintenance-page.text.no-resource-sample-recorded", "No resource sample recorded")
            font.pointSize: NacreTokens.textSize.caption
            color: NacreTokens.mutedInk
            wrapMode: Text.Wrap
        }
        NacreText {
            width: parent.width
            text: NacreI18n.tr("ui.maintenance-page.text.session-value0-update-cache-value1", "Session: {value0} · Update cache: {value1}", {
                value0: (root.state.session?.event || NacreI18n.tr("ui.maintenance-page.text.not-recorded", "Not recorded")),
                value1: (root.state.updatesAgeSeconds === null || root.state.updatesAgeSeconds === undefined ? NacreI18n.tr("ui.maintenance-page.text.not-recorded-ea80f8", "not recorded") : NacreI18n.tr("maintenance.cache.age", {
                        one: "{count} minutes old",
                        other: "{count} minutes old"
                    }, {
                        count: Math.floor(root.state.updatesAgeSeconds / 60)
                    }))
            })
            font.pointSize: NacreTokens.textSize.caption
            color: NacreTokens.mutedInk
            wrapMode: Text.Wrap
        }
        Flow {
            width: parent.width
            spacing: NacreTokens.space.sm
            ActionButton {
                text: NacreI18n.tr("ui.maintenance-page.text.refresh-status", "Refresh status")
                enabled: !Maintenance.busy
                onClicked: Maintenance.refresh()
            }
            ActionButton {
                text: NacreI18n.tr("ui.maintenance-page.text.sample-resource-use", "Sample resource use")
                enabled: !Maintenance.busy
                onClicked: root.act("profile")
            }
            ActionButton {
                text: NacreI18n.tr("ui.maintenance-page.text.check-session", "Check session")
                enabled: !Maintenance.busy
                onClicked: root.act("session")
            }
            ActionButton {
                text: NacreI18n.tr("ui.maintenance-page.text.retry-knowledge-sync", "Retry knowledge sync")
                enabled: !Maintenance.busy
                onClicked: root.act("sync-now")
            }
            ActionButton {
                text: NacreI18n.tr("ui.maintenance-page.text.repair-failed-portals", "Repair failed portals")
                enabled: !Maintenance.busy
                onClicked: root.act("repair-portals")
            }
            ActionButton {
                text: NacreI18n.tr("ui.maintenance-page.text.restart-desktop", "Restart desktop")
                enabled: !Maintenance.busy
                onClicked: root.pendingRecovery = "restart"
            }
            ActionButton {
                text: NacreI18n.tr("ui.maintenance-page.text.restore-previous-release", "Restore previous release")
                enabled: !Maintenance.busy
                onClicked: root.pendingRecovery = "rollback"
            }
        }
        NacreText {
            width: parent.width
            text: Maintenance.message || ""
            font.pointSize: NacreTokens.textSize.caption
            color: NacreTokens.accent
            wrapMode: Text.Wrap
        }
        NacreText {
            width: parent.width
            text: root.state.checked ? NacreI18n.tr("ui.maintenance-page.text.checked-value0", "Checked {value0}", {
                value0: new Date(root.state.checked).toLocaleString()
            }) : ""
            font.pointSize: NacreTokens.textSize.body
            color: NacreTokens.mutedInk
            wrapMode: Text.Wrap
        }
    }
}
