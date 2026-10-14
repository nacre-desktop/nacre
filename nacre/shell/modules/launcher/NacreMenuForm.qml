import QtQuick
import QtQuick.Controls.Basic
import qs.widgets
import qs.services

NacreSurface {
    id: root
    required property var form
    signal submitted(var values)
    signal cancelled
    property string error: ""
    color: NacreColours.palette.m3surface
    function submit() {
        const values = {};
        for (let index = 0; index < fields.count; index++) {
            const field = fields.itemAt(index);
            values[field.modelData.id] = field.value;
        }
        if (form.kind === "webapp" && (!values.label?.trim() || !/^https?:\/\//.test(values.url || ""))) {
            error = NacreI18n.tr("launcher.menu-form.error.enter-an-app-name-and-an-http-or-https-website-url", "Enter an app name and an HTTP or HTTPS website URL");
            return;
        }
        if (form.kind === "reminder" && (!(Number(values.minutes) >= .1 && Number(values.minutes) <= 1440) || !values.message?.trim())) {
            error = NacreI18n.tr("launcher.menu-form.error.enter-a-duration-from-0-1-to-1440-minutes-and-a-message", "Enter a duration from 0.1 to 1440 minutes and a message");
            return;
        }
        root.submitted(values);
    }
    Flickable {
        id: formViewport
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: fixedFooter.top
        anchors.margins: 24
        anchors.bottomMargin: 12
        clip: true
        contentHeight: formColumn.implicitHeight
        contentWidth: width
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar {
            minimumSize: 0.06
            contentItem: NacreSurface {
                implicitWidth: 5
                radius: 3
                color: NacreColours.palette.m3outline
                opacity: 0.5
            }
            background: Item {}
        }
        Column {
            id: formColumn
            x: (parent.width - width) / 2
            width: Math.min(parent.width, 520)
            spacing: 16
            NacreText {
                width: parent.width
                text: root.form.title || NacreI18n.tr("launcher.menu-form.text.enter-a-value", "Enter a value")
                font.pointSize: 18
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
            }
            NacreText {
                width: parent.width
                text: root.form.description || ""
                visible: !!text
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                color: NacreColours.palette.m3onSurfaceVariant
            }
            Repeater {
                id: fields
                model: root.form.fields || []
                delegate: Column {
                    id: fieldGroup
                    required property var modelData
                    property string chosen: modelData.initial || ""
                    function focusInput() {
                        if (modelData.multiline)
                            multiline.forceActiveFocus();
                        else if (modelData.choices)
                            choices.itemAt(0)?.focusControl();
                        else
                            input.forceActiveFocus();
                    }
                    required property int index
                    readonly property string value: modelData.choices ? chosen : modelData.multiline ? multiline.text : input.text
                    width: parent.width
                    spacing: 7
                    NacreText {
                        text: parent.modelData.label
                        textFormat: Text.PlainText
                    }
                    NacreTextField {
                        id: input
                        objectName: "menuFormField_" + parent.modelData.id
                        visible: !parent.modelData.multiline && !parent.modelData.choices
                        width: parent.width
                        height: 44
                        text: parent.modelData.initial || ""
                        placeholderText: parent.modelData.placeholder || ""
                        Component.onCompleted: if (parent.index === 0 && visible)
                            Qt.callLater(() => forceActiveFocus())
                        onAccepted: {
                            const next = fields.itemAt(parent.index + 1);
                            next ? next.focusInput() : root.submit();
                        }
                        Keys.onEscapePressed: root.cancelled()
                    }
                    Repeater {
                        id: choices
                        model: fieldGroup.modelData.choices || []
                        delegate: NacreMenuButton {
                            required property var modelData
                            objectName: "menuFormChoice_" + modelData.value
                            width: fieldGroup.width
                            text: modelData.label
                            selected: fieldGroup.chosen === modelData.value
                            onClicked: fieldGroup.chosen = modelData.value
                            Component.onCompleted: if (fieldGroup.index === 0 && selected)
                                Qt.callLater(focusControl)
                        }
                    }
                    TextArea {
                        id: multiline
                        visible: parent.modelData.multiline === true
                        width: parent.width
                        height: 110
                        text: parent.modelData.initial || ""
                        wrapMode: TextEdit.Wrap
                        selectByMouse: true
                        color: NacreColours.palette.m3onSurface
                        selectionColor: NacreColours.palette.m3primary
                        font.family: NacreTokens.textFamily
                        font.pointSize: 11
                        background: NacreSurface {
                            color: NacreColours.palette.m3surfaceContainer
                            radius: 8
                            border.width: multiline.activeFocus ? 2 : 1
                            border.color: multiline.activeFocus ? NacreColours.palette.m3primary : NacreColours.palette.m3outlineVariant
                        }
                        Keys.onEscapePressed: root.cancelled()
                    }
                }
            }
        }
    }
    Column {
        id: fixedFooter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 24
        anchors.horizontalCenter: parent.horizontalCenter
        width: Math.min(parent.width - 48, 520)
        spacing: 10
        NacreText {
            width: parent.width
            text: root.error
            visible: !!text
            maximumLineCount: 3
            elide: Text.ElideRight
            wrapMode: Text.Wrap
            color: NacreColours.palette.m3error
        }
        Row {
            spacing: 10
            NacreMenuButton {
                text: NacreI18n.tr("launcher.menu-form.text.cancel", "Cancel")
                onClicked: root.cancelled()
            }
            NacreMenuButton {
                objectName: "menuFormSubmit"
                text: root.form.button || NacreI18n.tr("launcher.menu-form.text.continue", "Continue")
                selected: true
                onClicked: root.submit()
            }
        }
    }
    Keys.onPressed: event => {
        if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && event.modifiers & Qt.ControlModifier) {
            submit();
            event.accepted = true;
        }
    }
    Keys.onEscapePressed: cancelled()
}
