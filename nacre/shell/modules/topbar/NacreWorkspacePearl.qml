import QtQuick
import qs.widgets
import qs.services

Item {
    id: root
    property color accent: NacreColours.palette.m3primary
    property bool occupied: false
    property bool current: false
    property bool urgent: false
    implicitWidth: current ? 20 : occupied ? 13 : 10
    implicitHeight: implicitWidth
    Rectangle {
        id: halo
        objectName: "workspaceAttentionRing"
        anchors.centerIn: parent
        width: parent.width + 5
        height: width
        radius: width / 2
        antialiasing: true
        color: "transparent"
        border.width: 1.5
        border.color: NacreColours.palette.m3tertiary
        opacity: .7
        visible: root.urgent
        SequentialAnimation on opacity {
            objectName: "workspaceAttentionPulse"
            running: halo.visible && root.visible && NacreTokens.motionEnabled
            loops: Animation.Infinite
            NumberAnimation {
                to: .25
                duration: 750
                easing.type: Easing.InOutSine
            }
            NumberAnimation {
                to: .8
                duration: 750
                easing.type: Easing.InOutSine
            }
            onRunningChanged: if (!running)
                halo.opacity = .7
        }
    }
    Rectangle {
        anchors.fill: parent
        radius: width / 2
        antialiasing: true
        color: NacreColours.palette.m3surfaceContainer
        border.width: root.current ? 1 : root.occupied ? 0 : 1.5
        border.color: root.current ? Qt.alpha(NacreColours.palette.m3onSurface, .5) : Qt.alpha(root.accent, .9)
        gradient: root.occupied || root.current ? enamel : null
        Gradient {
            id: enamel
            GradientStop {
                position: 0
                color: Qt.tint(root.accent, Qt.alpha("#fff9f1", root.current ? .88 : .48))
            }
            GradientStop {
                position: .45
                color: Qt.tint(root.accent, Qt.alpha("#fff9f1", root.current ? .40 : .10))
            }
            GradientStop {
                position: 1
                color: Qt.tint(root.accent, Qt.alpha(NacreColours.palette.m3secondary, .28))
            }
        }
        Rectangle {
            visible: root.current || root.occupied
            x: parent.width * .23
            y: parent.height * .14
            width: parent.width * .44
            height: parent.height * .23
            radius: height / 2
            antialiasing: true
            color: "#fff9f1"
            opacity: root.current ? .6 : .3
        }
    }
}
