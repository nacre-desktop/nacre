import QtQuick
import QtQuick.Controls.Basic
import qs.services

Slider {
    id: root
    property string label: "Level"
    from: 0
    to: 1
    stepSize: 0.01
    live: true
    padding: 10
    implicitWidth: 280
    implicitHeight: 32
    Accessible.name: label
    background: Rectangle {
        x: root.leftPadding
        y: (root.height - height) / 2
        width: root.availableWidth
        height: 8
        radius: 4
        color: NacreTokens.raised
        Rectangle {
            width: root.visualPosition * parent.width
            height: parent.height
            radius: 4
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop {
                    position: 0
                    color: NacreTokens.accent
                }
                GradientStop {
                    position: 1
                    color: NacreColours.palette.m3secondary
                }
            }
            opacity: root.enabled ? 1 : .4
        }
    }
    handle: Rectangle {
        x: root.leftPadding + root.visualPosition * (root.availableWidth - width)
        y: (root.height - height) / 2
        width: 20
        height: 20
        radius: 10
        gradient: Gradient {
            GradientStop {
                position: 0
                color: "#faf7ff"
            }
            GradientStop {
                position: .45
                color: "#e9e4ed"
            }
            GradientStop {
                position: 1
                color: NacreColours.palette.m3secondary
            }
        }
        border.width: root.visualFocus ? 2 : 1
        border.color: root.visualFocus ? NacreTokens.accent : NacreTokens.outline
        opacity: root.enabled ? 1 : .4
    }
}
