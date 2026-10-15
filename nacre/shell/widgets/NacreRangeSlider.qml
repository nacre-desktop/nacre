import QtQuick
import QtQuick.Controls.Basic
import qs.services

Slider {
    id: root
    property bool dimWhenDisabled: true
    opacity: enabled || !dimWhenDisabled ? 1 : .45
    property string label: NacreI18n.tr("ui.range-slider.label.level", "Level")
    from: 0
    to: 1
    stepSize: 0.01
    live: true
    padding: NacreTokens.space.md
    implicitWidth: NacreTokens.space.huge * 8
    implicitHeight: NacreTokens.space.huge + NacreTokens.space.sm
    Accessible.name: label
    background: Rectangle {
        x: root.leftPadding
        y: (root.height - height) / 2
        width: root.availableWidth
        height: NacreTokens.space.md
        radius: NacreTokens.radii.full
        // Keep the palette visible on the unfilled rail, including at minimum.
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop {
                position: 0
                color: Qt.tint(NacreColours.palette.m3surfaceContainerHighest, Qt.alpha(NacreTokens.accent, .3))
            }
            GradientStop {
                position: 1
                color: Qt.tint(NacreColours.palette.m3surfaceContainerHighest, Qt.alpha(NacreTokens.gradientColour(NacreTokens.accent, NacreTokens.orientSecondary), .3))
            }
        }
        Rectangle {
            width: root.visualPosition * parent.width
            height: parent.height
            radius: NacreTokens.radii.full
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop {
                    position: 0
                    color: NacreTokens.accent
                }
                GradientStop {
                    position: 1
                    color: NacreTokens.gradientColour(NacreTokens.accent, NacreTokens.orientSecondary)
                }
            }
        }
    }
    handle: Rectangle {
        x: root.leftPadding + root.visualPosition * (root.availableWidth - width)
        y: (root.height - height) / 2
        width: NacreTokens.space.xxl
        height: NacreTokens.space.xxl
        radius: NacreTokens.radii.full
        gradient: Gradient {
            GradientStop {
                position: 0
                color: NacreTokens.gradientColour(NacreTokens.accent, NacreColours.palette.m3primaryFixed || NacreTokens.orientHighlight)
            }
            GradientStop {
                position: .45
                color: NacreTokens.accent
            }
            GradientStop {
                position: 1
                color: NacreTokens.gradientColour(NacreTokens.accent, NacreTokens.orientSecondary)
            }
        }
        border.width: root.visualFocus ? 2 : 1
        border.color: root.visualFocus ? NacreTokens.ink : NacreTokens.accent
    }
}
