pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Window
import qs.services
import QtQuick.Controls.Basic

ComboBox {
    id: root
    property bool dimWhenDisabled: true
    opacity: enabled || !dimWhenDisabled ? 1 : .45
    property bool searchable: false
    property bool fontChoices: false
    property bool multiple: false
    property var checkedIndices: []
    property int maximumChecked: 0
    property var alwaysAvailableIndices: []
    signal optionToggled(int index)
    function canChoose(index) {
        return !multiple || !maximumChecked || checkedIndices.includes(index) || alwaysAvailableIndices.includes(index) || checkedIndices.length < maximumChecked;
    }
    function choose(index) {
        if (!canChoose(index))
            return;
        if (multiple)
            optionToggled(index);
        else {
            currentIndex = index;
            activated(index);
            popup.close();
        }
    }
    implicitWidth: NacreTokens.space.huge * 8
    implicitHeight: NacreTokens.space.huge + NacreTokens.space.sm
    font.family: NacreTokens.textFamily
    font.pointSize: NacreTokens.textSize.body
    onEnabledChanged: if (!enabled)
        popup.close()
    wheelEnabled: false
    leftPadding: NacreTokens.space.md
    rightPadding: NacreTokens.space.huge
    background: Rectangle {
        radius: NacreTokens.radii.small
        color: NacreTokens.raised
        border.width: 1
        border.color: root.visualFocus || root.popup.visible ? NacreTokens.accent : NacreTokens.outline
    }
    contentItem: NacreText {
        text: root.displayText
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }
    indicator: NacreIcon {
        x: root.width - width - NacreTokens.space.sm
        anchors.verticalCenter: parent.verticalCenter
        text: "expand_more"
    }
    popup: Popup {
        y: root.height + NacreTokens.space.xs
        width: root.searchable ? Math.max(root.width, Math.min(NacreTokens.space.huge * 11, root.Window.window?.width || root.width)) : root.width
        padding: NacreTokens.space.sm
        implicitHeight: Math.min(column.implicitHeight + padding * 2, NacreTokens.space.huge * 12)
        onOpened: {
            filter.text = "";
            choices.currentIndex = Math.max(0, root.currentIndex);
            choices.positionViewAtIndex(Math.max(0, root.currentIndex), ListView.Contain);
            if (root.searchable)
                filter.forceActiveFocus();
        }
        background: Rectangle {
            color: NacreTokens.raised
            radius: NacreTokens.radii.medium
            border.width: 1
            border.color: NacreTokens.outline
        }
        contentItem: Column {
            id: column
            spacing: NacreTokens.space.sm
            NacreTextField {
                id: filter
                objectName: "choiceFilter"
                visible: root.searchable
                width: parent.width
                placeholderText: NacreI18n.tr("settings.registry.ui.search-choices", "Search choices")
                onTextChanged: choices.currentIndex = text ? 0 : Math.max(0, root.currentIndex)
                Keys.onDownPressed: choices.forceActiveFocus()
                Keys.onReturnPressed: choices.chooseCurrent()
            }
            ListView {
                id: choices
                objectName: "choiceList"
                width: parent.width
                height: Math.min(contentHeight, NacreTokens.space.huge * 9)
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                model: (root.model || []).map((label, index) => ({
                            label: String(label),
                            index
                        })).filter(item => item.label.toLowerCase().includes(filter.text.toLowerCase()))
                currentIndex: root.currentIndex
                function chooseCurrent() {
                    const item = model[currentIndex];
                    if (item)
                        root.choose(item.index);
                }
                delegate: ItemDelegate {
                    required property var modelData
                    width: choices.width
                    height: NacreTokens.space.huge + NacreTokens.space.sm
                    highlighted: root.multiple ? root.checkedIndices.includes(modelData.index) : modelData.index === root.currentIndex
                    enabled: root.canChoose(modelData.index)
                    opacity: enabled ? 1 : .45
                    contentItem: NacreText {
                        text: (root.multiple && root.checkedIndices.includes(modelData.index) ? "✓ " : "") + modelData.label
                        font.family: root.fontChoices && modelData.index > 0 ? modelData.label : NacreTokens.textFamily
                        elide: Text.ElideRight
                        verticalAlignment: Text.AlignVCenter
                        color: parent.highlighted ? NacreTokens.focusInk(NacreTokens.accent) : NacreTokens.ink
                    }
                    background: Rectangle {
                        radius: NacreTokens.radii.small
                        color: parent.highlighted ? NacreTokens.accent : parent.hovered ? Qt.alpha(NacreTokens.accent, .15) : "transparent"
                    }
                    onClicked: root.choose(modelData.index)
                }
                ScrollBar.vertical: NacreScrollBar {}
                FastScroll {
                    view: choices
                    step: NacreTokens.space.huge * 5
                    pixelMultiplier: 2
                }
                Keys.onReturnPressed: chooseCurrent()
            }
        }
    }
}
