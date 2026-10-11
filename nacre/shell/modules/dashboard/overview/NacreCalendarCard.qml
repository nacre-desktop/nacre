import QtQuick
import qs.widgets
import qs.services
import "overview.js" as Overview

NacreOverviewCard {
    id: root
    property int monthOffset: 0
    property int firstWeekday: Qt.locale().firstDayOfWeek % 7
    readonly property int todayYear: NacreTime.date.getFullYear()
    readonly property int todayMonth: NacreTime.date.getMonth()
    readonly property string todayKey: Overview.dayKey(NacreTime.date)
    readonly property date displayed: new Date(todayYear, todayMonth + monthOffset, 1, 12)
    readonly property var days: Overview.calendar(displayed.getFullYear(), displayed.getMonth(), firstWeekday)
    NacreText {
        x: 44
        y: 14
        width: parent.width - 88
        text: Qt.formatDate(root.displayed, "MMMM yyyy")
        font.pointSize: 11
        horizontalAlignment: Text.AlignHCenter
        elide: Text.ElideRight
    }
    MonthButton {
        x: 8
        y: 8
        icon: "chevron_left"
        label: "Previous month"
        onActivated: root.monthOffset--
    }
    MonthButton {
        x: parent.width - 38
        y: 8
        icon: "chevron_right"
        label: "Next month"
        onActivated: root.monthOffset++
    }
    Grid {
        x: 12
        y: 52
        width: parent.width - 24
        columns: 7
        Repeater {
            model: 7
            NacreText {
                required property int index
                width: parent.width / 7
                height: 28
                text: Qt.locale().dayName(((root.firstWeekday + index) % 7) || 7, Locale.ShortFormat)
                font.pointSize: 10
                horizontalAlignment: Text.AlignHCenter
                elide: Text.ElideRight
                color: NacreColours.palette.m3onSurfaceVariant
            }
        }
        Repeater {
            model: root.days
            delegate: Item {
                required property var modelData
                width: parent.width / 7
                height: Math.max(24, (root.height - 92) / 6)
                Rectangle {
                    anchors.centerIn: parent
                    width: Math.min(parent.width - 4, 30)
                    height: width
                    radius: width / 2
                    color: NacreColours.palette.m3primary
                    visible: parent.modelData.key === root.todayKey
                }
                NacreText {
                    anchors.fill: parent
                    text: parent.modelData.day
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                    color: parent.modelData.key === root.todayKey ? NacreColours.palette.m3onPrimary : parent.modelData.inMonth ? NacreColours.palette.m3onSurface : NacreColours.palette.m3onSurfaceVariant
                    opacity: 1
                    font.pointSize: 11
                }
            }
        }
    }
    component MonthButton: NacreSurface {
        required property string icon
        required property string label
        signal activated
        width: 30
        height: 30
        radius: 15
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
