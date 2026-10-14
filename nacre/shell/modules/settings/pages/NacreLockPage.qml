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
        title: NacreI18n.tr("ui.lock-page.title.lock-screen", "Lock screen")
        Flow {
            width: parent.width
            spacing: NacreTokens.space.sm
            ActionButton {
                text: NacreI18n.tr("ui.lock-page.text.preview-layout", "Preview layout")
                onClicked: AppLaunch.run(["nacre-shell", "lock-preview"])
            }
            ActionButton {
                text: NacreI18n.tr("ui.lock-page.text.lock-now", "Lock now")
                onClicked: DesktopActions.execute("lock")
            }
        }
    }
    NacreSettingsSection {
        title: NacreI18n.tr("ui.lock-page.title.widgets-and-privacy", "Widgets and privacy")
        NacreSettingToggle {
            label: NacreI18n.tr("ui.lock-page.label.show-media-controls", "Show media controls")
            setting: "lockMedia"
        }
        NacreSettingToggle {
            label: NacreI18n.tr("ui.lock-page.label.show-weather", "Show weather")
            setting: "lockWeather"
        }
        NacreSettingToggle {
            label: NacreI18n.tr("ui.lock-page.label.show-notifications", "Show notifications")
            setting: "lockNotifications"
        }
        NacreSettingToggle {
            label: NacreI18n.tr("ui.lock-page.label.show-notification-message-contents", "Show notification message contents")
            setting: "lockNotificationContents"
        }
    }
    NacreSettingsSection {
        title: NacreI18n.tr("ui.lock-page.title.weather", "Weather")
        description: NacreI18n.tr("ui.lock-page.description.leave-the-city-empty-to-use-automatic-weather-location", "Leave the city empty to use automatic weather location.")
        NacreTextField {
            id: city
            objectName: "weatherCity"
            width: parent.width
            text: DesktopSettings.data.weatherLocation || ""
            placeholderText: NacreI18n.tr("ui.lock-page.placeholderText.city-or-city-country", "City or city, country")
            onAccepted: root.saveWeather(text)
        }
        Flow {
            width: parent.width
            spacing: NacreTokens.space.sm
            ActionButton {
                text: NacreI18n.tr("ui.lock-page.text.save-city", "Save city")
                onClicked: root.saveWeather(city.text)
            }
            ActionButton {
                text: NacreI18n.tr("ui.lock-page.text.refresh-weather", "Refresh weather")
                onClicked: NacreWeather.reload()
            }
        }
        NacreSettingToggle {
            label: NacreI18n.tr("ui.lock-page.label.use-fahrenheit", "Use Fahrenheit")
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
