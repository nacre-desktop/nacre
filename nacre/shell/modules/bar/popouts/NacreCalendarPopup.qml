import QtQuick
import qs.widgets
import qs.services
import qs.config
import "../../dashboard/overview/overview.js" as Calendar

Item {
    id: root
    property int month: NacreTime.date.getMonth()
    property int year: NacreTime.date.getFullYear()
    property int firstWeekday: Qt.locale().firstDayOfWeek % 7
    property string selectedDay: todayKey
    property bool interactive: false
    property var events: []
    signal daySelected(string day)
    readonly property date displayed: new Date(year, month, 1, 12)
    readonly property var days: Calendar.calendar(year, month, firstWeekday)
    readonly property string todayKey: Calendar.dayKey(NacreTime.date)
    implicitWidth: 280
    implicitHeight: 282
    function dateForDay(day) {
        const parts = day.split("-").map(Number);
        return new Date(parts[0], parts[1] - 1, parts[2], 12);
    }
    function changeMonth(delta) {
        if (!Number.isInteger(delta) || Math.abs(delta) > 1200)
            return;
        const date = new Date(year, month + delta, 1, 12);
        if (!Number.isFinite(date.getTime()) || date.getFullYear() < 1 || date.getFullYear() > 9999)
            return;
        year = date.getFullYear();
        month = date.getMonth();
    }
    NacreText {
        objectName: "nacreCalendarHeading"
        x: 38
        width: root.width - 76
        height: 34
        text: NacreI18n.date(root.displayed, "MMMM yyyy")
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }
    MonthButton {
        x: 0
        label: NacreI18n.tr("popouts.calendar-popup.label.previous-month", "Previous month")
        icon: "chevron_left"
        onActivated: root.changeMonth(-1)
    }
    MonthButton {
        x: root.width - width
        label: NacreI18n.tr("popouts.calendar-popup.label.next-month", "Next month")
        icon: "chevron_right"
        onActivated: root.changeMonth(1)
    }
    Grid {
        y: 40
        width: root.width
        columns: 7
        Repeater {
            model: 7
            NacreText {
                required property int index
                width: root.width / 7
                height: 28
                text: NacreI18n.dayName(((root.firstWeekday + index) % 7) || 7, Locale.ShortFormat)
                fontSizeMode: Text.Fit
                minimumPointSize: 6
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
                font.pointSize: 10
                color: NacreColours.palette.m3onSurfaceVariant
            }
        }
        Repeater {
            model: root.days
            delegate: Item {
                id: dayCell
                required property var modelData
                readonly property bool hasEvents: root.events.some(event => Calendar.dayKey(new Date(event.start * 1000)) === modelData.key)
                width: root.width / 7
                height: 34
                Rectangle {
                    anchors.centerIn: parent
                    width: 30
                    height: 30
                    radius: 15
                    visible: dayCell.modelData.key === root.todayKey || root.interactive && dayCell.modelData.key === root.selectedDay
                    color: dayCell.modelData.key === root.todayKey ? NacreColours.palette.m3primary : NacreColours.palette.m3secondaryContainer
                }
                NacreText {
                    anchors.fill: parent
                    text: NacreI18n.number(dayCell.modelData.day)
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                    font.pointSize: 11
                    color: dayCell.modelData.key === root.todayKey ? NacreColours.palette.m3onPrimary : root.interactive && dayCell.modelData.key === root.selectedDay ? NacreColours.palette.m3onSecondaryContainer : dayCell.modelData.inMonth ? NacreColours.palette.m3onSurface : NacreColours.palette.m3onSurfaceVariant
                }
                Rectangle {
                    anchors.horizontalCenter: parent.horizontalCenter
                    y: parent.height - 5
                    width: 4
                    height: 4
                    radius: 2
                    visible: dayCell.hasEvents
                    color: dayCell.modelData.key === root.todayKey ? NacreColours.palette.m3onPrimary : NacreTokens.accent
                }
                NacreInteraction {
                    enabled: root.interactive
                    accessibleName: NacreI18n.date(root.dateForDay(dayCell.modelData.key), "dddd, d MMMM yyyy")
                    function onClicked() {
                        root.selectedDay = dayCell.modelData.key;
                        root.daySelected(root.selectedDay);
                    }
                }
            }
        }
    }
    component MonthButton: NacreSurface {
        required property string label
        required property string icon
        signal activated
        width: 34
        height: 34
        radius: 17
        color: "transparent"
        NacreIcon {
            anchors.centerIn: parent
            text: parent.icon
        }
        NacreInteraction {
            accessibleName: parent.label
            function onClicked() {
                parent.activated();
            }
        }
    }
}
