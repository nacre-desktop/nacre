import QtQuick
import qs.widgets
import qs.services
import qs.config

Item {
    id: root
    property bool suppressed: false
    readonly property real availableHeight: Math.max(0, (parent?.height ?? 800) - 24)
    implicitWidth: Math.min(NacreNotifications.sizes.width + 24, parent?.width ?? 424)
    readonly property real targetHeight: suppressed || !NacreNotifs.popups.length ? 0 : Math.min(availableHeight, stream.contentHeight + 24)
    property real presentedHeight: targetHeight
    implicitHeight: suppressed ? 0 : presentedHeight
    Behavior on presentedHeight {
        enabled: NacreTokens.motionEnabled && !root.suppressed
        NumberAnimation {
            id: revealMotion
            duration: 230
            easing.type: Easing.OutCubic
        }
    }
    Connections {
        target: NacreTokens
        function onMotionEnabledChanged() {
            if (!NacreTokens.motionEnabled) {
                revealMotion.stop();
                root.presentedHeight = Qt.binding(() => root.targetHeight);
            }
        }
    }
    visible: !suppressed && height > 0
    clip: true
    function releaseHover() {
        for (const entry of NacreNotifs.popups)
            entry.hovered = false;
    }
    onSuppressedChanged: if (suppressed)
        releaseHover()
    onVisibleChanged: if (!visible)
        releaseHover()
    Component.onDestruction: releaseHover()
    ListView {
        id: stream
        objectName: "notificationStream"
        anchors.fill: parent
        anchors.margins: 12
        model: NacreNotifs.popups
        spacing: 8
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        delegate: NacreNotice {
            width: stream.width
        }
        FastScroll {
            view: stream
        }
        add: Transition {
            NumberAnimation {
                property: "opacity"
                from: 0
                to: 1
                duration: NacreTokens.motionEnabled ? 160 : 0
            }
        }
    }
}
