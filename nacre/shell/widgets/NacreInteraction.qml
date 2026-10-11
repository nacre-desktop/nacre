import QtQuick

Item {
    id: root

    property color color: NacreTokens.ink
    property real radius: parent?.radius ?? NacreTokens.cornerRadius
    property real topLeftRadius: parent?.topLeftRadius >= 0 ? parent.topLeftRadius : radius
    property real topRightRadius: parent?.topRightRadius >= 0 ? parent.topRightRadius : radius
    property real bottomLeftRadius: parent?.bottomLeftRadius >= 0 ? parent.bottomLeftRadius : radius
    property real bottomRightRadius: parent?.bottomRightRadius >= 0 ? parent.bottomRightRadius : radius
    property string accessibleName: ""
    property bool disabled: false
    property bool focusByPointer: false
    property bool keyboardPressed: false
    readonly property bool hovered: hover.hovered && visible && enabled && !disabled
    readonly property bool pressed: (pointer.pressed && pointer.containsMouse || keyboardPressed) && visible && enabled && !disabled
    readonly property bool keyboardFocusVisible: activeFocus && !focusByPointer && visible && enabled && !disabled
    readonly property real stateOpacity: !enabled || disabled ? 0 : pressed ? 0.12 : hovered ? 0.08 : keyboardFocusVisible ? 0.06 : 0

    signal activated(var event)
    function onClicked(event) {
    }
    function activate(event) {
        if (enabled && !disabled && visible) {
            activated(event);
            onClicked(event);
        }
    }
    function clearKeys() {
        keyboardPressed = false;
    }
    function containsPoint(point): bool {
        const x = point.x, y = point.y, w = width, h = height;
        if (x < 0 || y < 0 || x >= w || y >= h)
            return false;
        const corners = [[0, 0, topLeftRadius], [w, 0, topRightRadius], [0, h, bottomLeftRadius], [w, h, bottomRightRadius]];
        for (const [cx, cy, requested] of corners) {
            const r = Math.max(0, Math.min(requested < 0 ? radius : requested, w / 2, h / 2));
            if (r === 0)
                continue;
            const dx = Math.abs(x - cx), dy = Math.abs(y - cy);
            if (dx < r && dy < r && (dx - r) * (dx - r) + (dy - r) * (dy - r) > r * r)
                return false;
        }
        return true;
    }

    anchors.fill: parent
    enabled: !disabled
    Accessible.role: Accessible.Button
    Accessible.name: accessibleName || parent?.text || parent?.modelData?.name || ""
    Accessible.onPressAction: activate(null)
    activeFocusOnTab: enabled && !disabled && visible
    containmentMask: QtObject {
        function contains(point: point): bool {
            return root.containsPoint(point);
        }
    }
    onDisabledChanged: if (disabled) {
        clearKeys();
        focus = false;
    }
    // Busy controls must release focus rather than restore a stale focus ring.
    onEnabledChanged: if (!enabled) {
        clearKeys();
        focus = false;
    }
    onVisibleChanged: if (!visible) {
        clearKeys();
        Qt.callLater(() => {
            if (stateTween)
                stateTween.complete();
        });
    }
    onActiveFocusChanged: if (!activeFocus) {
        clearKeys();
        focusByPointer = false;
    }

    Rectangle {
        id: feedback
        objectName: "nacreInteractionFeedback"
        anchors.fill: parent
        radius: root.radius
        topLeftRadius: root.topLeftRadius
        topRightRadius: root.topRightRadius
        bottomLeftRadius: root.bottomLeftRadius
        bottomRightRadius: root.bottomRightRadius
        color: root.color
        opacity: root.stateOpacity
        Behavior on opacity {
            enabled: root.visible
            NumberAnimation {
                id: stateTween
                duration: root.enabled && !root.disabled && NacreTokens.motionEnabled ? NacreTokens.stateDuration : 0
                easing.type: Easing.OutCubic
            }
        }
    }
    Rectangle {
        objectName: "nacreInteractionFocus"
        anchors.fill: parent
        color: "transparent"
        radius: root.radius
        topLeftRadius: root.topLeftRadius
        topRightRadius: root.topRightRadius
        bottomLeftRadius: root.bottomLeftRadius
        bottomRightRadius: root.bottomRightRadius
        border.width: 1
        border.color: NacreTokens.focusInk(root.parent?.color)
        visible: root.keyboardFocusVisible
    }
    HoverHandler {
        id: hover
        enabled: root.enabled && !root.disabled
    }
    MouseArea {
        id: pointer
        anchors.fill: parent
        containmentMask: root.containmentMask
        enabled: root.enabled && !root.disabled
        hoverEnabled: root.enabled && !root.disabled
        acceptedButtons: Qt.LeftButton
        cursorShape: Qt.PointingHandCursor
        onPressed: {
            root.focusByPointer = true;
            root.forceActiveFocus(Qt.MouseFocusReason);
        }
        onClicked: event => root.activate(event)
        onCanceled: root.clearKeys()
    }
    Keys.onPressed: event => {
        if (!root.enabled || root.disabled)
            return;
        if (event.key === Qt.Key_Space) {
            if (!event.isAutoRepeat)
                root.keyboardPressed = true;
            root.focusByPointer = false;
            event.accepted = true;
        } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
            root.focusByPointer = false;
            if (!event.isAutoRepeat)
                root.activate(null);
            event.accepted = true;
        } else
            event.accepted = false;
    }
    Keys.onReleased: event => {
        if (event.key !== Qt.Key_Space || event.isAutoRepeat)
            return;
        const activate = root.keyboardPressed;
        root.clearKeys();
        if (activate)
            root.activate(null);
        event.accepted = true;
    }
    Connections {
        target: NacreTokens
        function onMotionEnabledChanged() {
            if (!NacreTokens.motionEnabled)
                Qt.callLater(() => {
                    if (stateTween)
                        stateTween.complete();
                });
        }
    }
}
