import QtQuick
import QtQuick.Controls.Basic
import qs.widgets
import qs.services

Flickable {
    id: root
    // Settings selects pages, extras, weather, media, home-time or calendar-focus
    // and suppresses its enclosing heading; all uses the attached customizer.
    property string section: "all"
    property bool showHeading: true
    property bool showDescription: true
    readonly property var sections: section === "all" ? ["pages", "extras", "weather", "media", "home-time", "calendar-focus"] : [section]
    contentWidth: width
    contentHeight: content.height
    implicitHeight: contentHeight
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: NacreScrollBar {}
    FastScroll {
        view: root
    }
    Column {
        id: content
        width: root.width
        spacing: NacreTokens.space.xl
        NacreText {
            visible: root.section === "all" && root.showHeading
            text: NacreI18n.tr("dashboard.dashboard-customize.text.make-this-dashboard-yours", "Make this dashboard yours")
            font.pointSize: 19
            color: NacreTokens.accent
        }
        Repeater {
            model: root.sections
            NacreDashboardCustomizeSection {
                required property string modelData
                objectName: "dashboardCustomize-" + modelData
                width: content.width
                section: modelData
                showHeading: root.showHeading
                showDescription: root.showDescription
            }
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
