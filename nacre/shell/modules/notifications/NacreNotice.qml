import QtQuick
import Quickshell
import qs.widgets
import qs.services
import qs.config

NacreSurface {
    id: root
    required property var modelData
    property bool history: false
    property bool expanded: false
    property real dragOffset: 0
    readonly property var availableActions: !history && modelData.notification ? [...modelData.actions].filter(action => action.identifier !== "default") : []
    readonly property bool hasDetails: !!modelData.body || !!modelData.image || availableActions.length > 0
    implicitWidth: NacreNotifications.sizes.width
    implicitHeight: contents.implicitHeight + 28
    x: dragOffset
    radius: 14
    color: NacreColours.palette.m3surfaceContainer
    border.width: 1
    border.color: modelData.urgency === 2 ? NacreColours.palette.m3error || NacreColours.palette.m3primary : Qt.alpha(NacreColours.palette.m3outlineVariant, 0.45)
    activeFocusOnTab: true
    clip: true
    Accessible.role: Accessible.AlertMessage
    Accessible.name: modelData.appName + ": " + modelData.summary
    function dismiss() {
        if (modelData) {
            modelData.hovered = false;
            NacreNotifs.dismiss(modelData);
        }
    }
    function openDetails() {
        if (NacreNotifications.actionOnClick && !history && modelData.notification) {
            const action = [...modelData.actions].find(action => action.identifier === "default");
            if (action) {
                action.invoke();
                return;
            }
        }
        if (hasDetails)
            expanded = !expanded;
    }
    function releaseHover() {
        if (modelData)
            modelData.hovered = false;
    }
    onVisibleChanged: if (!visible)
        releaseHover()
    Component.onDestruction: releaseHover()
    HoverHandler {
        onHoveredChanged: if (root.modelData)
            root.modelData.hovered = hovered && root.visible
    }
    Behavior on dragOffset {
        enabled: !pointer.pressed
        NumberAnimation {
            duration: NacreTokens.motionEnabled ? 160 : 0
            easing.type: Easing.OutCubic
        }
    }
    MouseArea {
        id: pointer
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        property real startX: 0
        property bool moved: false
        onPressed: event => {
            startX = mapToItem(null, event.x, event.y).x;
            moved = false;
            root.forceActiveFocus(Qt.MouseFocusReason);
        }
        onPositionChanged: event => {
            if (!pressed || pressedButtons !== Qt.LeftButton)
                return;
            const delta = mapToItem(null, event.x, event.y).x - startX;
            if (Math.abs(delta) > 8)
                moved = true;
            if (moved)
                root.dragOffset = delta;
        }
        onReleased: {
            if (moved && Math.abs(root.dragOffset) >= root.width * NacreNotifications.clearThreshold)
                root.dismiss();
            root.dragOffset = 0;
        }
        onCanceled: root.dragOffset = 0
        onClicked: event => {
            if (moved)
                return;
            if (event.button === Qt.RightButton)
                root.dismiss();
            else
                root.openDetails();
        }
    }
    Column {
        id: contents
        x: 14
        y: 14
        width: parent.width - 28
        spacing: 9
        Item {
            width: parent.width
            height: 30
            Image {
                id: appIcon
                width: 28
                height: 28
                anchors.verticalCenter: parent.verticalCenter
                source: root.modelData.appIcon.startsWith("/") ? "file://" + root.modelData.appIcon : Quickshell.iconPath(root.modelData.appIcon, true)
                asynchronous: true
                sourceSize.width: 28
                sourceSize.height: 28
                visible: status === Image.Ready
            }
            NacreIcon {
                anchors.centerIn: appIcon
                text: "notifications"
                visible: appIcon.status !== Image.Ready
                font.pointSize: 17
            }
            Column {
                x: 38
                width: parent.width - 114
                anchors.verticalCenter: parent.verticalCenter
                NacreText {
                    width: parent.width
                    text: root.modelData.appName || "Notification"
                    font.pointSize: 10
                    elide: Text.ElideRight
                }
                NacreText {
                    width: parent.width
                    text: root.modelData.timeStr || ""
                    color: NacreColours.palette.m3onSurfaceVariant
                    font.pointSize: 8
                    elide: Text.ElideRight
                }
            }
            CircleControl {
                objectName: "noticeExpand"
                x: parent.width - 66
                visible: root.hasDetails
                icon: root.expanded ? "expand_less" : "expand_more"
                label: root.expanded ? "Collapse notification" : "Expand notification"
                onActivated: root.expanded = !root.expanded
            }
            CircleControl {
                objectName: "noticeDismiss"
                x: parent.width - 30
                icon: "close"
                label: "Dismiss notification"
                onActivated: root.dismiss()
            }
        }
        NacreText {
            objectName: "noticeSummary"
            width: parent.width
            text: root.modelData.summary
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            maximumLineCount: root.expanded ? 6 : 2
            elide: Text.ElideRight
            font.pointSize: 12
            font.weight: Font.DemiBold
        }
        NacreText {
            objectName: "noticeBody"
            width: parent.width
            visible: text.length > 0
            text: root.modelData.body
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            maximumLineCount: root.expanded ? 12 : 2
            elide: Text.ElideRight
            color: NacreColours.palette.m3onSurfaceVariant
            font.pointSize: 10
        }
        Image {
            objectName: "noticeImage"
            width: parent.width
            height: visible ? 140 : 0
            visible: root.expanded && root.modelData.image.length > 0
            source: visible ? root.modelData.image : ""
            asynchronous: true
            sourceSize.width: 600
            sourceSize.height: 300
            fillMode: Image.PreserveAspectCrop
        }
        Flow {
            objectName: "noticeActions"
            width: parent.width
            visible: root.expanded && root.availableActions.length > 0
            spacing: 6
            Repeater {
                model: root.availableActions
                delegate: ActionButton {
                    required property var modelData
                    text: modelData.text || "Open"
                    compact: true
                    width: Math.min(implicitWidth, contents.width)
                    clip: true
                    onClicked: modelData.invoke()
                }
            }
        }
    }
    Keys.onPressed: event => {
        if (event.key === Qt.Key_Delete) {
            root.dismiss();
            event.accepted = true;
        } else if (event.key === Qt.Key_Space || event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
            root.openDetails();
            event.accepted = true;
        }
    }
    component CircleControl: NacreSurface {
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
            font.pointSize: 14
        }
        NacreInteraction {
            accessibleName: parent.label
            function onClicked() {
                parent.activated();
            }
        }
    }
}
