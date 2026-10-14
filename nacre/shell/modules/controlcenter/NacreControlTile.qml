import QtQuick
import QtQuick.Controls.Basic
import qs.widgets
import qs.services

NacreClip {
    id: root
    property string label: ""
    property string detail: ""
    property string icon: ""
    property bool selected: false
    property bool available: true
    property bool hasDetails: false
    property bool navigation: false
    property bool contextDetails: false
    property string explanation: ""
    property bool elideDetail: false
    property bool held: false
    readonly property color tileInk: !available ? NacreTokens.mutedInk : selected ? NacreColours.palette.m3onPrimaryContainer : NacreTokens.ink
    signal clicked
    signal detailsRequested
    implicitHeight: Math.max(66, labels.implicitHeight + 16)
    implicitWidth: 184
    radius: 24
    color: !available ? Qt.alpha(NacreTokens.raised, .4) : selected ? NacreColours.palette.m3primaryContainer : NacreTokens.raised
    border.width: toggleInput.keyboardFocusVisible || detailInput.keyboardFocusVisible ? 2 : 1
    border.color: toggleInput.keyboardFocusVisible || detailInput.keyboardFocusVisible ? NacreTokens.focusInk(root.color) : Qt.alpha(NacreTokens.outline, available ? .12 : .06)
    Item {
        width: parent.width - (root.hasDetails ? 34 : 0)
        height: parent.height
        NacreIcon {
            x: 14
            anchors.verticalCenter: parent.verticalCenter
            text: root.icon
            font.pointSize: 17
            color: root.tileInk
            opacity: root.available ? 1 : .4
        }
        Column {
            id: labels
            x: 44
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - x - 8 - (root.navigation ? 24 : 0)
            spacing: 3
            NacreText {
                width: parent.width
                text: root.label
                color: root.tileInk
                opacity: root.available ? 1 : .55
                font.pointSize: 10
                font.weight: Font.Medium
                wrapMode: Text.Wrap
            }
            NacreText {
                width: parent.width
                text: root.detail
                visible: root.detail !== ""
                font.pointSize: 9
                color: root.selected && root.available ? root.tileInk : NacreTokens.mutedInk
                opacity: root.available ? 1 : .55
                wrapMode: Text.Wrap
                maximumLineCount: root.elideDetail ? 2 : 100
                elide: root.elideDetail ? Text.ElideRight : Text.ElideNone
            }
        }
        NacreInteraction {
            id: toggleInput
            objectName: root.objectName + "Toggle"
            radius: root.radius
            topRightRadius: root.hasDetails ? 0 : radius
            bottomRightRadius: root.hasDetails ? 0 : radius
            color: root.tileInk
            drawFocusOutline: false
            disabled: !root.available
            accessibleName: root.label + ": " + root.detail
            function onClicked() {
                if (!root.held)
                    root.clicked();
                root.held = false;
            }
            Keys.onPressed: event => {
                if (root.contextDetails && (event.key === Qt.Key_Menu || event.key === Qt.Key_F10 && event.modifiers & Qt.ShiftModifier)) {
                    root.detailsRequested();
                    event.accepted = true;
                }
            }
        }
    }
    NacreIcon {
        objectName: root.objectName + "NavigationGlyph"
        anchors.right: parent.right
        anchors.rightMargin: 12
        anchors.verticalCenter: parent.verticalCenter
        visible: root.navigation
        text: "chevron_right"
        color: root.tileInk
        font.pointSize: 17
    }
    NacreSurface {
        anchors.right: parent.right
        width: 34
        height: parent.height
        radius: 0
        color: "transparent"
        visible: root.hasDetails
        NacreIcon {
            anchors.centerIn: parent
            text: "chevron_right"
            font.pointSize: 17
            color: root.tileInk
        }
        NacreInteraction {
            id: detailInput
            color: root.tileInk
            drawFocusOutline: false
            objectName: root.objectName + "Details"
            accessibleName: NacreI18n.tr("controlcenter.tile.details", "Open {name} details", {
                name: root.label
            })
            function onClicked() {
                root.detailsRequested();
            }
        }
    }
    TapHandler {
        acceptedButtons: Qt.RightButton
        enabled: root.contextDetails
        onTapped: root.detailsRequested()
    }
    Connections {
        target: toggleInput
        function onPressedChanged() {
            if (root.contextDetails && toggleInput.pressed && !toggleInput.keyboardPressed) {
                root.held = false;
                hold.restart();
            } else
                hold.stop();
        }
    }
    Timer {
        id: hold
        interval: 600
        onTriggered: {
            root.held = true;
            root.detailsRequested();
        }
    }
    TapHandler {
        acceptedButtons: Qt.LeftButton
        enabled: root.contextDetails && !root.available
        longPressThreshold: .6
        onLongPressed: root.detailsRequested()
    }
    HoverHandler {
        id: explanationHover
    }
    ToolTip.visible: explanationHover.hovered && root.explanation !== ""
    ToolTip.delay: 400
    ToolTip.text: root.explanation
}
