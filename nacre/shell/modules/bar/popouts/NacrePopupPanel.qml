import QtQuick
import qs.widgets

Item {
    id: root
    required property var screen
    property alias currentName: content.currentName
    property alias currentCenter: content.currentCenter
    property alias hasCurrent: content.hasCurrent
    property bool headerHovered: false
    property bool pinned: false
    readonly property real targetWidth: content.targetWidth
    readonly property real targetHeight: content.targetHeight
    readonly property var currentItem: content.currentItem
    property real presentedHeight: 0
    implicitWidth: targetWidth
    implicitHeight: Math.max(0, presentedHeight)
    visible: height > 0
    enabled: hasCurrent
    clip: true
    function focusPinned() {
        if (pinned && hasCurrent && visible)
            forceActiveFocus(Qt.OtherFocusReason);
    }
    function syncSize() {
        presentedHeight = hasCurrent ? targetHeight : 0;
    }
    onTargetHeightChanged: Qt.callLater(syncSize)
    onVisibleChanged: if (visible && pinned)
        Qt.callLater(focusPinned)
    Component.onCompleted: Qt.callLater(syncSize)
    Connections {
        target: NacreTokens
        function onMotionEnabledChanged() {
            if (!NacreTokens.motionEnabled) {
                sizeMotion.stop();
                root.syncSize();
            }
        }
    }
    onPinnedChanged: if (pinned)
        Qt.callLater(focusPinned)
    onHasCurrentChanged: {
        if (!hasCurrent) {
            pinned = false;
            presentedHeight = 0;
        } else if (!content.validRoute)
            hasCurrent = false;
        else
            Qt.callLater(syncSize);
    }
    onCurrentNameChanged: if (hasCurrent && !content.validRoute)
        hasCurrent = false
    NacrePopupContent {
        id: content
        screen: root.screen
        retaining: root.presentedHeight > 0
    }
    Behavior on presentedHeight {
        enabled: NacreTokens.motionEnabled
        NumberAnimation {
            id: sizeMotion
            duration: NacreTokens.motionEnabled ? 230 : 0
            easing.type: Easing.InOutCubic
        }
    }
}
