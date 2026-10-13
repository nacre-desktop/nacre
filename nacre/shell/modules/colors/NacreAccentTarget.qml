import QtQuick
import qs.widgets
import qs.services

NacreSurface {
    id: root
    required property string role
    required property string value
    property bool pinned: false
    signal assign(string value)
    signal clear
    width: 180
    height: 54
    radius: 12
    color: NacreTokens.raised
    border.width: drop.containsDrag ? 2 : 1
    border.color: drop.containsDrag ? NacreTokens.accent : NacreTokens.outline
    Rectangle {
        x: 12
        y: 12
        width: 30
        height: 30
        radius: 8
        color: "#" + root.value
    }
    NacreText {
        x: 52
        y: 18
        text: ({
                primary: NacreI18n.tr("colors.accent-target.text.main", "Main"),
                secondary: NacreI18n.tr("colors.accent-target.text.second", "Second"),
                tertiary: NacreI18n.tr("colors.accent-target.text.third", "Third")
            })[root.role]
        font.pointSize: 10
    }
    NacreText {
        x: 114
        y: 18
        text: "#" + root.value
        font.family: NacreTokens.monoFamily
        font.pointSize: 9
        color: NacreTokens.mutedInk
    }
    ActionButton {
        x: parent.width - width - 10
        y: 10
        text: ""
        accessibleLabel: root.pinned ? NacreI18n.tr("ui.accent-target.accessibleLabel.unlock-value0", "Unlock {value0}", {
            value0: root.role
        }) : NacreI18n.tr("ui.accent-target.accessibleLabel.lock-value0", "Lock {value0}", {
            value0: root.role
        })
        icon: root.pinned ? "lock" : "lock_open"
        onClicked: root.pinned ? root.clear() : root.assign(root.value)
    }
    DropArea {
        id: drop
        anchors.fill: parent
        keys: ["orient-color"]
        onDropped: drop => {
            const value = drop.source?.colorHex;
            if (typeof value === "string" && /^[0-9a-f]{6}$/.test(value)) {
                root.assign(value);
                drop.accept();
            }
        }
    }
}
