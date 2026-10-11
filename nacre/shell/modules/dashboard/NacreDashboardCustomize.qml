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
            text: "Make this dashboard yours"
            font.pointSize: 19
            color: NacreTokens.accent
        }
        NacreText {
            width: parent.width
            text: "Show the pages you use, and arrange them in your preferred order. Number keys follow that order."
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
                    text: parent.modelData === "today" ? "Dashboard" : parent.modelData[0].toUpperCase() + parent.modelData.slice(1)
                    selected: NacreDashboardState.tabs.includes(parent.modelData)
                    onClicked: NacreDashboardState.toggleTab(parent.modelData)
                }
                ActionButton {
                    text: "Earlier"
                    compact: true
                    enabled: NacreDashboardState.tabs.indexOf(parent.modelData) > 0
                    onClicked: NacreDashboardState.reorder(parent.modelData, -1)
                }
                ActionButton {
                    text: "Later"
                    compact: true
                    enabled: NacreDashboardState.tabs.includes(parent.modelData) && NacreDashboardState.tabs.indexOf(parent.modelData) < NacreDashboardState.tabs.length - 1
                    onClicked: NacreDashboardState.reorder(parent.modelData, 1)
                }
            }
        }
        Row {
            spacing: 10
            ActionButton {
                text: "Synced lyrics"
                selected: NacreDashboardState.preferences.lyrics !== false
                onClicked: NacreDashboardState.save("lyrics", !NacreDashboardState.preferences.lyrics)
            }
            ActionButton {
                text: "Album colors"
                selected: NacreDashboardState.preferences.albumPalette !== false
                onClicked: NacreDashboardState.save("albumPalette", !NacreDashboardState.preferences.albumPalette)
            }
        }
        NacreText {
            text: "Home time while traveling"
            font.pointSize: 14
        }
        NacreTextField {
            id: zone
            width: parent.width
            placeholderText: "Home timezone, e.g. Europe/Oslo"
            text: NacreDashboardState.preferences.homeZone || ""
        }
        NacreTextField {
            id: city
            width: parent.width
            placeholderText: "Home city label (optional)"
            text: NacreDashboardState.preferences.homeCity || ""
        }
        ActionButton {
            text: "Save home time"
            compact: true
            onClicked: {
                NacreDashboardState.saveMany({
                    homeZone: zone.text.trim(),
                    homeCity: city.text.trim()
                });
            }
        }
        NacreText {
            text: "Local calendar"
            font.pointSize: 14
        }
        NacreTextField {
            id: calendar
            width: parent.width
            placeholderText: "Absolute path to an .ics export"
            text: NacreDashboardState.preferences.calendarFile || ""
        }
        NacreText {
            width: parent.width
            text: "Reads local events only. Export expanded recurring events from your calendar app; account sync remains with that app."
            font.pointSize: 10
            color: NacreTokens.mutedInk
            wrapMode: Text.Wrap
        }
        ActionButton {
            text: "Save calendar"
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
