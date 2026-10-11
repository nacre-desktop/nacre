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
            error = "Enter an app name and an HTTP or HTTPS website URL";
            return;
        }
        if (form.kind === "reminder" && (!(Number(values.minutes) >= .1 && Number(values.minutes) <= 1440) || !values.message?.trim())) {
            error = "Enter a duration from 0.1 to 1440 minutes and a message";
            return;
        }
        root.submitted(values);
    }
    Column {
        anchors.centerIn: parent
        width: Math.min(parent.width - 48, 520)
        spacing: 16
        NacreText {
            width: parent.width
            text: root.form.title || "Enter a value"
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
                required property var modelData
                required property int index
                property alias value: input.text
                width: parent.width
                spacing: 7
                NacreText {
                    text: parent.modelData.label
                    textFormat: Text.PlainText
                }
                NacreTextField {
                    id: input
                    objectName: "menuFormField_" + parent.modelData.id
                    width: parent.width
                    height: 44
                    text: parent.modelData.initial || ""
                    placeholderText: parent.modelData.placeholder || ""
                    Component.onCompleted: if (parent.index === 0)
                        Qt.callLater(() => forceActiveFocus())
                    onAccepted: root.submit()
                    Keys.onEscapePressed: root.cancelled()
                }
            }
        }
        NacreText {
            width: parent.width
            text: root.error
            visible: !!text
            wrapMode: Text.Wrap
            color: NacreColours.palette.m3error
        }
        Row {
            spacing: 10
            NacreMenuButton {
                text: "Cancel"
                onClicked: root.cancelled()
            }
            NacreMenuButton {
                text: root.form.button || "Continue"
                selected: true
                onClicked: root.submit()
            }
        }
    }
    Keys.onEscapePressed: cancelled()
}
