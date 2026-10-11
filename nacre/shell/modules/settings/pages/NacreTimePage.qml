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
        title: TimezoneSettings.status.localTime || "Local time"
        description: TimezoneSettings.status.timezone || "Reading timezone…"
        Flow {
            width: parent.width
            spacing: 8
            ActionButton {
                text: "Refresh"
                enabled: !TimezoneSettings.busy
                onClicked: TimezoneSettings.refresh()
            }
            ActionButton {
                text: TimezoneSettings.status.installed ? "Update location now" : "Set up automatic timezone"
                enabled: !TimezoneSettings.busy
                onClicked: root.change(TimezoneSettings.status.installed ? "update" : "install")
            }
        }
    }
    NacreSettingsSection {
        title: "Automatic timezone"
        description: "Uses device location when available, with your confirmed timezone as a fallback. Network location alone never changes the clock."
        Flow {
            width: parent.width
            spacing: 8
            ActionButton {
                text: "On"
                selected: TimezoneSettings.status.automatic === true
                enabled: !TimezoneSettings.busy && TimezoneSettings.status.installed === true
                onClicked: root.change("automatic", "on")
            }
            ActionButton {
                text: "Off"
                selected: TimezoneSettings.status.automatic !== true
                enabled: !TimezoneSettings.busy && TimezoneSettings.status.installed === true
                onClicked: root.change("automatic", "off")
            }
        }
        NacreText {
            width: parent.width
            text: "Confirmed timezone: " + (TimezoneSettings.status.confirmedTimezone || "Not set")
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
        title: "Choose a timezone"
        description: "For example, Europe/Oslo or America/Chicago. Confirming keeps automatic mode; setting manually turns it off."
        NacreTextField {
            id: zone
            objectName: "timezoneEntry"
            width: parent.width
            text: TimezoneSettings.status.timezone || ""
            placeholderText: "Area/City"
        }
        Flow {
            width: parent.width
            spacing: 8
            ActionButton {
                text: "Confirm timezone"
                enabled: !TimezoneSettings.busy && !!zone.text.trim()
                onClicked: root.change("confirm", zone.text.trim())
            }
            ActionButton {
                text: "Set manually"
                enabled: !TimezoneSettings.busy && !!zone.text.trim()
                onClicked: root.change("manual", zone.text.trim())
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
