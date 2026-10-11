import QtQuick
import qs.widgets
import qs.services

NacreSettingsPage {
    id: root
    property string page: "lock"
    function saveWeather(value) {
        DesktopSettings.set("weatherLocation", value.trim());
    }
    NacreSettingsSection {
        title: "Lock screen"
        Flow {
            width: parent.width
            spacing: 8
            ActionButton {
                text: "Preview layout"
                onClicked: AppLaunch.run(["nacre-shell", "lock-preview"])
            }
            ActionButton {
                text: "Lock now"
                onClicked: DesktopActions.execute("lock")
            }
        }
    }
    NacreSettingsSection {
        title: "Widgets and privacy"
        NacreSettingToggle {
            label: "Show media controls"
            setting: "lockMedia"
        }
        NacreSettingToggle {
            label: "Show weather"
            setting: "lockWeather"
        }
        NacreSettingToggle {
            label: "Show notifications"
            setting: "lockNotifications"
        }
        NacreSettingToggle {
            label: "Show notification message contents"
            setting: "lockNotificationContents"
        }
    }
    NacreSettingsSection {
        title: "Weather"
        description: "Leave the city empty to use automatic weather location."
        NacreTextField {
            id: city
            objectName: "weatherCity"
            width: parent.width
            text: DesktopSettings.data.weatherLocation || ""
            placeholderText: "City or city, country"
            onAccepted: root.saveWeather(text)
        }
        Flow {
            width: parent.width
            spacing: 8
            ActionButton {
                text: "Save city"
                onClicked: root.saveWeather(city.text)
            }
            ActionButton {
                text: "Refresh weather"
                onClicked: NacreWeather.reload()
            }
        }
        NacreSettingToggle {
            label: "Use Fahrenheit"
            setting: "weatherFahrenheit"
        }
        NacreText {
            width: parent.width
            text: [NacreWeather.location, NacreWeather.displayTemperature, NacreWeather.description].filter(value => !!value).join(" · ")
            wrapMode: Text.Wrap
        }
        NacreText {
            width: parent.width
            text: NacreWeather.error || DesktopSettings.message
            wrapMode: Text.Wrap
            color: NacreTokens.mutedInk
        }
    }
}
