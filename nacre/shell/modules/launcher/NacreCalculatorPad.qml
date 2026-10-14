import QtQuick
import QtQuick.Layouts
import qs.widgets
import qs.services

Flickable {
    id: root

    required property string expression
    required property var result
    property bool copied: false
    signal insertRequested(string value)
    signal actionRequested(string action)

    contentWidth: width
    contentHeight: Math.max(height, display.height + 8 + 6 * 42 + 5 * 7)
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    onExpressionChanged: copied = false
    onResultChanged: copied = false

    function focusFirst() {
        keys.itemAt(4).focusExplicit();
    }

    NacreSurface {
        id: display
        width: root.width
        // Reserve the status line so incomplete expressions never move the keys.
        height: 110
        radius: 12
        color: NacreColours.palette.m3surfaceContainerHigh
        NacreText {
            x: 14
            y: 10
            width: parent.width - 28
            text: NacreI18n.tr("launcher.calculator.result", "Result")
            font.pointSize: 9
            color: NacreColours.palette.m3onSurfaceVariant
        }
        NacreText {
            objectName: "calculatorResult"
            x: 14
            y: 29
            width: parent.width - copy.width - 40
            height: 43
            text: root.result.ok ? root.result.value : root.expression ? "—" : "0"
            font.family: NacreTokens.monoFamily
            font.pointSize: 25
            verticalAlignment: Text.AlignVCenter
            elide: Text.ElideMiddle
        }
        ActionButton {
            id: copy
            objectName: "calculatorCopy"
            x: parent.width - width - 12
            y: 34
            width: Math.min(root.width * .4, implicitWidth)
            compact: true
            text: root.copied ? NacreI18n.tr("launcher.calculator.copied", "Copied") : NacreI18n.tr("launcher.calculator.copy", "Copy")
            accessibleLabel: NacreI18n.tr("launcher.calculator.copy-result", "Copy result")
            icon: root.copied ? "check" : "content_copy"
            enabled: root.result.ok === true
            onClicked: {
                root.actionRequested("copy");
                root.copied = true;
            }
        }
        NacreText {
            id: error
            objectName: "calculatorError"
            x: 14
            y: 82
            width: parent.width - 28
            height: 20
            text: !root.result.ok && root.expression ? root.result.error || "" : ""
            visible: text.length > 0
            elide: Text.ElideRight
            font.pointSize: 9
            color: NacreColours.palette.m3onSurfaceVariant
        }
    }

    GridLayout {
        y: display.height + 8
        width: root.width
        height: root.contentHeight - y
        columns: 4
        columnSpacing: 7
        rowSpacing: 7
        Repeater {
            id: keys
            model: [
                {
                    id: "clear",
                    label: NacreI18n.tr("launcher.calculator.clear-key", "C"),
                    action: "clear",
                    utility: true,
                    caption: NacreI18n.tr("launcher.calculator.clear", "Clear calculation")
                },
                {
                    id: "backspace",
                    label: "",
                    icon: "backspace",
                    action: "backspace",
                    utility: true,
                    caption: NacreI18n.tr("launcher.calculator.backspace", "Delete previous character")
                },
                {
                    id: "left-parenthesis",
                    label: "(",
                    value: "(",
                    utility: true,
                    caption: NacreI18n.tr("launcher.calculator.left-parenthesis", "Open parenthesis")
                },
                {
                    id: "right-parenthesis",
                    label: ")",
                    value: ")",
                    utility: true,
                    caption: NacreI18n.tr("launcher.calculator.right-parenthesis", "Close parenthesis")
                },
                {
                    id: "7",
                    label: "7",
                    value: "7"
                },
                {
                    id: "8",
                    label: "8",
                    value: "8"
                },
                {
                    id: "9",
                    label: "9",
                    value: "9"
                },
                {
                    id: "divide",
                    label: "÷",
                    value: "/",
                    operator: true,
                    caption: NacreI18n.tr("launcher.calculator.divide", "Divide")
                },
                {
                    id: "4",
                    label: "4",
                    value: "4"
                },
                {
                    id: "5",
                    label: "5",
                    value: "5"
                },
                {
                    id: "6",
                    label: "6",
                    value: "6"
                },
                {
                    id: "multiply",
                    label: "×",
                    value: "*",
                    operator: true,
                    caption: NacreI18n.tr("launcher.calculator.multiply", "Multiply")
                },
                {
                    id: "1",
                    label: "1",
                    value: "1"
                },
                {
                    id: "2",
                    label: "2",
                    value: "2"
                },
                {
                    id: "3",
                    label: "3",
                    value: "3"
                },
                {
                    id: "subtract",
                    label: "−",
                    value: "-",
                    operator: true,
                    caption: NacreI18n.tr("launcher.calculator.subtract", "Subtract")
                },
                {
                    id: "sign",
                    label: "±",
                    action: "sign",
                    utility: true,
                    caption: NacreI18n.tr("launcher.calculator.sign", "Change sign")
                },
                {
                    id: "0",
                    label: "0",
                    value: "0"
                },
                {
                    id: "decimal",
                    label: ".",
                    value: ".",
                    caption: NacreI18n.tr("launcher.calculator.decimal", "Decimal point")
                },
                {
                    id: "add",
                    label: "+",
                    value: "+",
                    operator: true,
                    caption: NacreI18n.tr("launcher.calculator.add", "Add")
                },
                {
                    id: "square",
                    symbol: "x²",
                    action: "square",
                    utility: true,
                    caption: NacreI18n.tr("launcher.calculator.square", "Square")
                },
                {
                    id: "sqrt",
                    label: "√",
                    action: "sqrt",
                    utility: true,
                    caption: NacreI18n.tr("launcher.calculator.sqrt", "Square root")
                },
                {
                    id: "equals",
                    label: "=",
                    action: "equals",
                    caption: NacreI18n.tr("launcher.calculator.equals", "Calculate")
                }
            ]
            delegate: NacreSurface {
                id: key
                required property var modelData
                objectName: "calculatorKey_" + modelData.id
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.columnSpan: modelData.id === "equals" ? 2 : 1
                implicitWidth: 0
                implicitHeight: 42
                radius: 10
                enabled: modelData.id !== "equals" || root.result.ok === true
                opacity: enabled ? 1 : .4
                color: modelData.id === "equals" ? NacreColours.palette.m3primary : modelData.operator ? NacreColours.palette.m3primaryContainer : Qt.tint(NacreColours.palette.m3surface, Qt.alpha(NacreColours.palette.m3onSurface, modelData.utility ? .09 : .045))
                function focusExplicit() {
                    interaction.forceActiveFocus();
                }
                NacreText {
                    anchors.centerIn: parent
                    text: key.modelData.symbol || key.modelData.label
                    font.pointSize: 16
                    color: key.modelData.id === "equals" ? NacreColours.palette.m3onPrimary : key.modelData.operator ? NacreColours.palette.m3onPrimaryContainer : NacreColours.palette.m3onSurface
                }
                NacreIcon {
                    anchors.centerIn: parent
                    text: key.modelData.icon || ""
                    visible: !!key.modelData.icon
                    font.pointSize: 16
                }
                NacreInteraction {
                    id: interaction
                    accessibleName: key.modelData.caption || key.modelData.label
                    function onClicked() {
                        if (key.modelData.action)
                            root.actionRequested(key.modelData.action);
                        else
                            root.insertRequested(key.modelData.value);
                    }
                }
            }
        }
    }
}
