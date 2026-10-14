import QtQuick
import qs.widgets
import qs.services

NacreSettingsPage {
    id: root
    property string page: "time"
    function change(kind, value) {
        if (!TimezoneSettings.busy && ["install", "update", "automatic", "manual", "confirm"].includes(kind))
            TimezoneSettings.change(kind, value || "");
    }
    NacreSettingsSection {
        title: TimezoneSettings.status.localTime || NacreI18n.tr("ui.time-page.title.local-time", "Local time")
        description: TimezoneSettings.status.timezone || NacreI18n.tr("ui.time-page.description.reading-timezone", "Reading timezone…")
        Flow {
            width: parent.width
            spacing: NacreTokens.space.sm
            ActionButton {
                text: NacreI18n.tr("ui.time-page.text.refresh", "Refresh")
                enabled: !TimezoneSettings.busy
                onClicked: TimezoneSettings.refresh()
            }
            ActionButton {
                text: TimezoneSettings.status.installed ? NacreI18n.tr("ui.time-page.text.update-location-now", "Update location now") : NacreI18n.tr("ui.time-page.text.set-up-automatic-timezone", "Set up automatic timezone")
                enabled: !TimezoneSettings.busy
                onClicked: root.change(TimezoneSettings.status.installed ? "update" : "install")
            }
        }
    }
    NacreSettingsSection {
        title: NacreI18n.tr("ui.time-page.title.automatic-timezone", "Automatic timezone")
        description: NacreI18n.tr("ui.time-page.description.uses-device-location-when-available-with-your-confirmed", "Uses device location when available, with your confirmed timezone as a fallback. Network location alone never changes the clock.")
        Flow {
            width: parent.width
            spacing: NacreTokens.space.sm
            ActionButton {
                text: NacreI18n.tr("ui.time-page.text.on", "On")
                selected: TimezoneSettings.status.automatic === true
                enabled: !TimezoneSettings.busy && TimezoneSettings.status.installed === true
                onClicked: root.change("automatic", "on")
            }
            ActionButton {
                text: NacreI18n.tr("ui.time-page.text.off", "Off")
                selected: TimezoneSettings.status.automatic !== true
                enabled: !TimezoneSettings.busy && TimezoneSettings.status.installed === true
                onClicked: root.change("automatic", "off")
            }
        }
        NacreText {
            width: parent.width
            text: NacreI18n.tr("ui.time-page.text.confirmed-timezone-value0", "Confirmed timezone: {value0}", {
                value0: (TimezoneSettings.status.confirmedTimezone || NacreI18n.tr("ui.time-page.text.not-set", "Not set"))
            })
            wrapMode: Text.Wrap
        }
        NacreText {
            width: parent.width
            text: [TimezoneSettings.status.city, TimezoneSettings.status.source].filter(value => !!value).join(" · ")
            wrapMode: Text.Wrap
            color: NacreTokens.mutedInk
        }
    }
    NacreSettingsSection {
        title: NacreI18n.tr("ui.time-page.title.choose-a-timezone", "Choose a timezone")
        description: NacreI18n.tr("ui.time-page.description.for-example-europe-oslo-or-america-chicago-confirming-k", "For example, Europe/Oslo or America/Chicago. Confirming keeps automatic mode; setting manually turns it off.")
        NacreSelect {
            id: zone
            objectName: "timezoneEntry"
            property string selectedZone: TimezoneSettings.status.timezone || ""
            width: Math.min(parent.width, NacreTokens.space.huge * 12)
            model: TimezoneSettings.zones || [selectedZone]
            searchable: true
            currentIndex: model.indexOf(selectedZone)
            onActivated: index => selectedZone = model[index]
        }
        Flow {
            width: parent.width
            spacing: NacreTokens.space.sm
            ActionButton {
                text: NacreI18n.tr("ui.time-page.text.confirm-timezone", "Confirm timezone")
                enabled: !TimezoneSettings.busy && !!zone.selectedZone
                onClicked: root.change("confirm", zone.selectedZone)
            }
            ActionButton {
                text: NacreI18n.tr("ui.time-page.text.set-manually", "Set manually")
                enabled: !TimezoneSettings.busy && !!zone.selectedZone
                onClicked: root.change("manual", zone.selectedZone)
            }
        }
    }
    NacreText {
        width: parent.width
        text: TimezoneSettings.message || TimezoneSettings.status.error || ""
        wrapMode: Text.Wrap
        color: NacreTokens.mutedInk
    }
}
