import QtQuick
import qs.widgets
import qs.utils

Item {
    id: root
    required property var client
    property int size: 16
    implicitWidth: size
    implicitHeight: size
    readonly property string brand: {
        const klass = String(client?.wmClass || "").toLowerCase();
        const match = /^nacre-(ai|brain|colors|settings|welcome)(?:-|$)/.exec(klass);
        return match?.[1] || "";
    }
    BrandLogo {
        anchors.fill: parent
        visible: root.brand !== ""
        ai: root.brand === "ai"
        brain: root.brand === "brain"
        colorsApp: root.brand === "colors"
        settings: root.brand === "settings"
    }
    Image {
        id: image
        anchors.fill: parent
        visible: !root.brand && status === Image.Ready
        source: root.brand ? "" : NacreIcons.getAppIcon(root.client?.wmClass || "", "application-x-executable")
        sourceSize.width: root.size
        sourceSize.height: root.size
        fillMode: Image.PreserveAspectFit
        asynchronous: true
    }
    NacreIcon {
        anchors.centerIn: parent
        visible: !root.brand && !image.visible
        text: "apps"
        font.pointSize: root.size * .65
    }
    Accessible.ignored: true
}
