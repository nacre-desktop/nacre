import QtQuick
import qs.widgets

Item {
    id: root
    required property var visibilities
    property real presentedWidth: 0
    implicitWidth: Math.max(0, presentedWidth)
    implicitHeight: 380
    visible: width > 0
    enabled: visibilities.session
    clip: true
    function focusOpened() {
        if (visibilities.session && visible)
            forceActiveFocus(Qt.OtherFocusReason);
    }
    onVisibleChanged: if (visible)
        Qt.callLater(focusOpened)
    function syncSize() {
        presentedWidth = visibilities.session ? 150 : 0;
    }
    Connections {
        target: root.visibilities
        function onSessionChanged() {
            root.syncSize();
            if (root.visibilities.session)
                Qt.callLater(root.focusOpened);
        }
    }
    Component.onCompleted: syncSize()
    Loader {
        active: root.visibilities.session || root.presentedWidth > 0
        width: 150
        height: 380
        sourceComponent: NacreSessionControls {
            visibilities: root.visibilities
        }
    }
    Behavior on presentedWidth {
        enabled: NacreTokens.motionEnabled
        NumberAnimation {
            id: motion
            duration: 180
            easing.type: Easing.OutCubic
        }
    }
    Connections {
        target: NacreTokens
        function onMotionEnabledChanged() {
            if (!NacreTokens.motionEnabled) {
                motion.stop();
                root.syncSize();
            }
        }
    }
}
