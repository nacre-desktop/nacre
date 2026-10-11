import QtQuick
import qs.widgets
import qs.services

NacreOverviewCard {
    id: root
    readonly property bool available: Number.isFinite(NacreWeather.temperature)
    Accessible.role: Accessible.StaticText
    Accessible.name: NacreWeather.location + " " + NacreWeather.displayTemperature + " " + NacreWeather.description
    Row {
        id: reading
        width: parent.width - 24
        anchors.horizontalCenter: parent.horizontalCenter
        y: 28
        spacing: 12
        NacreIcon {
            width: root.width < 180 ? 40 : 64
            height: 64
            verticalAlignment: Text.AlignVCenter
            text: NacreWeather.icon || "cloud_off"
            font.pointSize: root.width < 180 ? 28 : 44
            color: NacreColours.palette.m3secondary
        }
        NacreText {
            objectName: "weatherTemperature"
            width: Math.max(0, reading.width - reading.children[0].width - reading.spacing)
            height: 64
            verticalAlignment: Text.AlignVCenter
            fontSizeMode: Text.Fit
            minimumPointSize: 16
            text: root.available ? NacreWeather.displayTemperature : "—"
            font.pointSize: root.width < 180 ? 22 : 30
            font.weight: Font.DemiBold
            color: NacreColours.palette.m3primary
        }
    }
    NacreText {
        objectName: "weatherDescription"
        x: 12
        y: 90
        width: parent.width - 24
        text: NacreWeather.description || (root.available ? "Weather" : "Weather unavailable")
        font.pointSize: 12
        horizontalAlignment: Text.AlignHCenter
        elide: Text.ElideRight
    }
    NacreText {
        x: 12
        y: 121
        width: parent.width - 24
        text: NacreWeather.stale ? "Cached weather" : NacreWeather.location || ""
        font.pointSize: 9
        color: NacreColours.palette.m3onSurfaceVariant
        horizontalAlignment: Text.AlignHCenter
        elide: Text.ElideRight
    }
}
