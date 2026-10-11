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
    readonly property date displayed: new Date(year, month, 1, 12)
    readonly property var days: Calendar.calendar(year, month, firstWeekday)
    readonly property string todayKey: Calendar.dayKey(NacreTime.date)
    implicitWidth: 280
    implicitHeight: 282
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
        text: Qt.formatDate(root.displayed, "MMMM yyyy")
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }
    MonthButton {
        x: 0
        label: "Previous month"
        icon: "chevron_left"
        onActivated: root.changeMonth(-1)
    }
    MonthButton {
        x: root.width - width
        label: "Next month"
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
                text: Qt.locale().dayName(((root.firstWeekday + index) % 7) || 7, Locale.ShortFormat)
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
                font.pointSize: 10
                color: NacreColours.palette.m3onSurfaceVariant
            }
        }
        Repeater {
            model: root.days
            delegate: Item {
                required property var modelData
                width: root.width / 7
                height: 34
                Rectangle {
                    anchors.centerIn: parent
                    width: 30
                    height: 30
                    radius: 15
                    visible: parent.modelData.key === root.todayKey
                    color: NacreColours.palette.m3primary
                }
                NacreText {
                    anchors.fill: parent
                    text: parent.modelData.day
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                    font.pointSize: 11
                    color: parent.modelData.key === root.todayKey ? NacreColours.palette.m3onPrimary : parent.modelData.inMonth ? NacreColours.palette.m3onSurface : NacreColours.palette.m3onSurfaceVariant
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
