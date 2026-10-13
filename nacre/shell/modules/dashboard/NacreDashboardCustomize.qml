import QtQuick
import QtQuick.Controls.Basic
import qs.widgets
import qs.services

Flickable {
    id: root
    contentWidth: width
    contentHeight: content.height
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: NacreScrollBar {}
    FastScroll {
        view: root
    }
    Column {
        id: content
        width: root.width
        spacing: 18
        NacreText {
            text: NacreI18n.tr("dashboard.dashboard-customize.text.make-this-dashboard-yours", "Make this dashboard yours")
            font.pointSize: 19
            color: NacreTokens.accent
        }
        NacreText {
            width: parent.width
            text: NacreI18n.tr("dashboard.dashboard-customize.text.show-the-pages-you-use-and-arrange-them-in-your-preferr", "Show the pages you use, and arrange them in your preferred order. Number keys follow that order.")
            font.pointSize: 11
            wrapMode: Text.Wrap
            color: NacreTokens.mutedInk
        }
        Repeater {
            model: ["today", "weather", "media", "system", "focus"]
            Row {
                required property string modelData
                width: content.width
                spacing: 12
                ActionButton {
                    width: 140
                    text: parent.modelData === "today" ? NacreI18n.tr("dashboard.dashboard-customize.text.dashboard", "Dashboard") : NacreI18n.option("dashboard-tab", parent.modelData)
                    selected: NacreDashboardState.tabs.includes(parent.modelData)
                    onClicked: NacreDashboardState.toggleTab(parent.modelData)
                }
                ActionButton {
                    text: NacreI18n.tr("dashboard.dashboard-customize.text.earlier", "Earlier")
                    compact: true
                    enabled: NacreDashboardState.tabs.indexOf(parent.modelData) > 0
                    onClicked: NacreDashboardState.reorder(parent.modelData, -1)
                }
                ActionButton {
                    text: NacreI18n.tr("dashboard.dashboard-customize.text.later", "Later")
                    compact: true
                    enabled: NacreDashboardState.tabs.includes(parent.modelData) && NacreDashboardState.tabs.indexOf(parent.modelData) < NacreDashboardState.tabs.length - 1
                    onClicked: NacreDashboardState.reorder(parent.modelData, 1)
                }
            }
        }
        Row {
            spacing: 10
            ActionButton {
                text: NacreI18n.tr("dashboard.dashboard-customize.text.synced-lyrics", "Synced lyrics")
                selected: NacreDashboardState.preferences.lyrics !== false
                onClicked: NacreDashboardState.save("lyrics", !NacreDashboardState.preferences.lyrics)
            }
            ActionButton {
                text: NacreI18n.tr("dashboard.dashboard-customize.text.album-colors", "Album colors")
                selected: NacreDashboardState.preferences.albumPalette !== false
                onClicked: NacreDashboardState.save("albumPalette", !NacreDashboardState.preferences.albumPalette)
            }
        }
        NacreText {
            text: NacreI18n.tr("dashboard.dashboard-customize.text.home-time-while-traveling", "Home time while traveling")
            font.pointSize: 14
        }
        NacreTextField {
            id: zone
            width: parent.width
            placeholderText: NacreI18n.tr("dashboard.dashboard-customize.placeholderText.home-timezone-e-g-europe-oslo", "Home timezone, e.g. Europe/Oslo")
            text: NacreDashboardState.preferences.homeZone || ""
        }
        NacreTextField {
            id: city
            width: parent.width
            placeholderText: NacreI18n.tr("dashboard.dashboard-customize.placeholderText.home-city-label-optional", "Home city label (optional)")
            text: NacreDashboardState.preferences.homeCity || ""
        }
        ActionButton {
            text: NacreI18n.tr("dashboard.dashboard-customize.text.save-home-time", "Save home time")
            compact: true
            onClicked: {
                NacreDashboardState.saveMany({
                    homeZone: zone.text.trim(),
                    homeCity: city.text.trim()
                });
            }
        }
        NacreText {
            text: NacreI18n.tr("dashboard.dashboard-customize.text.local-calendar", "Local calendar")
            font.pointSize: 14
        }
        NacreTextField {
            id: calendar
            width: parent.width
            placeholderText: NacreI18n.tr("dashboard.dashboard-customize.placeholderText.absolute-path-to-an-ics-export", "Absolute path to an .ics export")
            text: NacreDashboardState.preferences.calendarFile || ""
        }
        NacreText {
            width: parent.width
            text: NacreI18n.tr("dashboard.dashboard-customize.text.reads-local-events-only-export-expanded-recurring-event", "Reads local events only. Export expanded recurring events from your calendar app; account sync remains with that app.")
            font.pointSize: 10
            color: NacreTokens.mutedInk
            wrapMode: Text.Wrap
        }
        ActionButton {
            text: NacreI18n.tr("dashboard.dashboard-customize.text.save-calendar", "Save calendar")
            compact: true
            onClicked: NacreDashboardState.save("calendarFile", calendar.text.trim())
        }
        NacreText {
            width: parent.width
            visible: NacreDashboardState.message !== ""
            text: NacreDashboardState.message
            wrapMode: Text.Wrap
            color: NacreTokens.mutedInk
        }
    }
}
