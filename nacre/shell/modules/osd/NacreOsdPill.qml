import QtQuick
import qs.widgets

Item {
    id: root
    property string title: ""
    property string detail: ""
    property string icon: "volume_up"
    property string valueLabel: ""
    property real value: 0
    property bool level: true
    property bool muted: false
    property bool animateLevel: true
    implicitWidth: 336
    implicitHeight: level ? 76 : 68
    Accessible.role: Accessible.StaticText
    Accessible.name: [title, valueLabel, detail].filter(part => part !== "").join(". ")
    Connections {
        target: NacreTokens
        function onMotionEnabledChanged() {
            if (!NacreTokens.motionEnabled) {
                fill.width = Qt.binding(() => track.width * Math.max(0, Math.min(1, root.value)));
            }
        }
    }
    NacreSurface {
        anchors.fill: parent
        radius: height / 2
        color: NacreTokens.body
        border.width: 1
        border.color: Qt.alpha(NacreTokens.outline, .28)
    }
    NacreIcon {
        objectName: "feedbackIcon"
        x: 19
        anchors.verticalCenter: parent.verticalCenter
        text: root.icon
        font.pointSize: 22
        color: root.muted ? NacreTokens.mutedInk : NacreTokens.accent
    }
    NacreText {
        id: heading
        objectName: "feedbackTitle"
        x: 60
        y: root.level ? 12 : 14
        width: parent.width - x - 25 - percentage.implicitWidth
        text: root.title
        font.pointSize: 11
        font.weight: Font.Medium
        elide: Text.ElideRight
    }
    NacreText {
        id: percentage
        objectName: "feedbackValue"
        anchors.right: parent.right
        anchors.rightMargin: 24
        y: heading.y
        text: root.valueLabel
        font.pointSize: 11
        color: NacreTokens.mutedInk
    }
    NacreText {
        objectName: "feedbackDetail"
        x: heading.x
        y: heading.y + heading.height + 2
        width: parent.width - x - 24
        text: root.detail
        font.pointSize: 9
        color: NacreTokens.mutedInk
        elide: Text.ElideRight
    }
    Rectangle {
        id: track
        objectName: "feedbackTrack"
        x: heading.x
        y: parent.height - 18
        width: parent.width - x - 24
        height: 5
        radius: height / 2
        visible: root.level
        color: NacreTokens.raised
        Rectangle {
            id: fill
            objectName: "feedbackFill"
            width: parent.width * Math.max(0, Math.min(1, root.value))
            height: parent.height
            radius: height / 2
            opacity: root.muted ? .35 : 1
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop {
                    position: 0
                    color: NacreTokens.accent
                }
                GradientStop {
                    position: 1
                    color: NacreTokens.gradientColour(NacreTokens.accent, NacreTokens.orientSecondary)
                }
            }
            Behavior on width {
                enabled: root.animateLevel && NacreTokens.motionEnabled
                NumberAnimation {
                    duration: NacreTokens.motionEnabled ? 130 : 0
                    easing.type: Easing.OutCubic
                }
            }
        }
    }
}
