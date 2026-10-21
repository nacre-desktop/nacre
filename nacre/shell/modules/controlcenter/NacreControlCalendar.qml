import QtQuick
import QtQuick.Layouts
import qs.widgets
import qs.services
import qs.modules.bar.popouts as Details
import "../dashboard/overview/overview.js" as Calendar

Column {
    id: root
    spacing: 16
    readonly property var events: NacreDashboardState.calendar.events || []
    readonly property var selectedEvents: events.filter(event => Calendar.dayKey(new Date(event.start * 1000)) === month.selectedDay)
    Details.NacreCalendarPopup {
        id: month
        objectName: "controlCalendarMonth"
        width: root.width
        interactive: true
        events: root.events
    }
    RowLayout {
        width: parent.width
        NacreText {
            Layout.fillWidth: true
            text: NacreI18n.date(month.dateForDay(month.selectedDay), "dddd, d MMMM")
            font.pointSize: 12
            wrapMode: Text.Wrap
        }
        ActionButton {
            objectName: "controlCalendarToday"
            compact: true
            text: NacreI18n.tr("controlcenter.calendar.today", "Today")
            onClicked: {
                month.month = NacreTime.date.getMonth();
                month.year = NacreTime.date.getFullYear();
                month.selectedDay = month.todayKey;
            }
        }
    }
    Repeater {
        model: root.selectedEvents
        delegate: NacreSurface {
            required property var modelData
            width: root.width
            implicitHeight: eventText.implicitHeight + 24
            radius: 16
            color: NacreTokens.raised
            RowLayout {
                id: eventText
                x: 12
                y: 12
                width: parent.width - 24
                spacing: 12
                NacreIcon {
                    text: "event"
                    color: NacreTokens.accent
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    NacreText {
                        Layout.fillWidth: true
                        text: modelData.title
                        wrapMode: Text.Wrap
                    }
                    NacreText {
                        objectName: "controlCalendarEventTime"
                        text: modelData.time.indexOf(":") < 0 ? NacreI18n.tr("controlcenter.calendar.all-day", "All day") : NacreI18n.time(new Date(modelData.start * 1000), "HH:mm")
                        font.pointSize: 10
                        color: NacreTokens.mutedInk
                    }
                }
            }
        }
    }
    NacreText {
        width: parent.width
        visible: !root.selectedEvents.length
        text: NacreI18n.tr("controlcenter.calendar.empty", "No events for this day")
        color: NacreTokens.mutedInk
        wrapMode: Text.Wrap
    }
    ActionButton {
        objectName: "controlCalendarSettings"
        icon: "calendar_month"
        text: NacreI18n.tr("controlcenter.calendar.settings", "Calendar settings")
        onClicked: NacrePanelState.openSettings("panels/dashboard#calendarFile")
    }
}
