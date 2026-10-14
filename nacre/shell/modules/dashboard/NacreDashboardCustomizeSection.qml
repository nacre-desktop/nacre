import QtQuick
import qs.widgets
import qs.services

Column {
    id: part
    property string section: "pages"
    property bool showHeading: true
    property bool showDescription: true
    readonly property string title: {
        switch (section) {
        case "pages":
            return NacreI18n.tr("dashboard.settings.pages", "Dashboard pages");
        case "extras":
            return NacreI18n.tr("dashboard.customize.extras", "Optional pages");
        case "weather":
        case "media":
            return NacreI18n.option("dashboard-tab", section);
        case "home-time":
            return NacreI18n.tr("dashboard.dashboard-customize.text.home-time-while-traveling", "Home time while traveling");
        case "calendar-focus":
            return NacreI18n.tr("dashboard.customize.calendar-focus", "Calendar and focus");
        default:
            return "";
        }
    }
    spacing: NacreTokens.space.md
    NacreText {
        objectName: "dashboardCustomizeSectionHeading"
        visible: part.showHeading && part.title.length > 0
        text: part.title
        width: parent.width
        wrapMode: Text.Wrap
        font.pointSize: NacreTokens.textSize.title
    }
    Loader {
        objectName: "dashboardCustomizeSectionBody"
        width: part.width
        active: part.visible
        sourceComponent: ({
                pages: pages,
                extras: extras,
                weather: weather,
                media: media,
                "home-time": home,
                "calendar-focus": calendarFocus
            })[part.section] || null
    }
    Component {
        id: pages
        Column {
            spacing: NacreTokens.space.md
            NacreText {
                visible: part.showDescription
                width: parent.width
                text: NacreI18n.tr("dashboard.dashboard-customize.text.show-the-pages-you-use-and-arrange-them-in-your-preferr", "Show the pages you use, and arrange them in your preferred order. Number keys follow that order.")
                wrapMode: Text.Wrap
                color: NacreTokens.mutedInk
                font.pointSize: NacreTokens.textSize.caption
            }
            NacreDashboardExtrasOptions {
                width: parent.width
                includeCore: true
                includeExtras: false
            }
        }
    }
    Component {
        id: extras
        NacreDashboardExtrasOptions {
            includeCore: false
            includeExtras: true
            showPreview: false
        }
    }
    Component {
        id: weather
        NacreDashboardToggleRow {
            objectName: "dashboardWeatherPhotos"
            label: NacreI18n.tr("dashboard.customize.city-photos", "City photos in Weather")
            description: part.showDescription ? NacreI18n.tr("dashboard.customize.city-photos-detail", "Optional Wikipedia/Wikimedia photos, with credit. Uses the city name; cached locally.") : ""
            checked: NacreDashboardState.weatherPhotos === true
            enabled: NacreDashboardState.extrasReady
            onToggled: value => {
                NacreDashboardState.weatherPhotos = value;
                NacreDashboardState.saveExtras();
            }
        }
    }
    Component {
        id: media
        NacreDashboardToggleRow {
            objectName: "dashboardLyrics"
            label: NacreI18n.tr("dashboard.dashboard-customize.text.synced-lyrics", "Lyrics")
            checked: NacreDashboardState.preferences.lyrics !== false
            onToggled: value => NacreDashboardState.save("lyrics", value)
        }
    }
    Component {
        id: home
        Column {
            spacing: NacreTokens.space.sm
            NacreTextField {
                id: zone
                objectName: "dashboardHomeZone"
                width: parent.width
                placeholderText: NacreI18n.tr("dashboard.dashboard-customize.placeholderText.home-timezone-e-g-europe-oslo", "Home timezone, e.g. Europe/Oslo")
                text: NacreDashboardState.preferences.homeZone || ""
            }
            NacreTextField {
                id: city
                objectName: "dashboardHomeCity"
                width: parent.width
                placeholderText: NacreI18n.tr("dashboard.dashboard-customize.placeholderText.home-city-label-optional", "Home city label (optional)")
                text: NacreDashboardState.preferences.homeCity || ""
            }
            ActionButton {
                objectName: "dashboardSaveHome"
                text: NacreI18n.tr("dashboard.dashboard-customize.text.save-home-time", "Save home time")
                compact: true
                onClicked: NacreDashboardState.saveMany({
                    homeZone: zone.text.trim(),
                    homeCity: city.text.trim()
                })
            }
        }
    }
    Component {
        id: calendarFocus
        Column {
            spacing: NacreTokens.space.sm
            NacreText {
                text: NacreI18n.tr("dashboard.dashboard-customize.text.local-calendar", "Local calendar")
                font.pointSize: NacreTokens.textSize.body
            }
            NacreTextField {
                id: calendar
                objectName: "dashboardCalendarFile"
                width: parent.width
                placeholderText: NacreI18n.tr("dashboard.dashboard-customize.placeholderText.absolute-path-to-an-ics-export", "Absolute path to an .ics export")
                text: NacreDashboardState.preferences.calendarFile || ""
            }
            NacreText {
                visible: part.showDescription
                width: parent.width
                text: NacreI18n.tr("dashboard.dashboard-customize.text.reads-local-events-only-export-expanded-recurring-event", "Reads local events only. Export expanded recurring events from your calendar app; account sync remains with that app.")
                font.pointSize: NacreTokens.textSize.caption
                color: NacreTokens.mutedInk
                wrapMode: Text.Wrap
            }
            ActionButton {
                objectName: "dashboardSaveCalendar"
                text: NacreI18n.tr("dashboard.dashboard-customize.text.save-calendar", "Save calendar")
                compact: true
                onClicked: NacreDashboardState.save("calendarFile", calendar.text.trim())
            }
            NacreText {
                text: NacreI18n.tr("dashboard.customize.focus-duration", "Default focus duration")
                font.pointSize: NacreTokens.textSize.body
            }
            Flow {
                width: parent.width
                spacing: NacreTokens.space.sm
                Repeater {
                    model: [15, 25, 50]
                    ActionButton {
                        required property int modelData
                        objectName: "dashboardFocusMinutes-" + modelData
                        text: NacreI18n.tr("ui.focus-page.text.value0-min", "{value0} min", {
                            value0: modelData
                        })
                        compact: true
                        selected: (NacreDashboardState.preferences.focusMinutes || 25) === modelData
                        onClicked: NacreDashboardState.save("focusMinutes", modelData)
                    }
                }
            }
        }
    }
}
