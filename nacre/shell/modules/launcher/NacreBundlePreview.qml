pragma ComponentBehavior: Bound
import QtQuick
import qs.widgets
import qs.services

Column {
    id: root
    required property var entry
    property string mode: "dark"
    property string scope: "both"
    property bool busy: false
    signal applyRequested(string mode, string scope)
    spacing: 12
    Grid {
        id: previews
        width: parent.width
        columns: root.width >= 620 ? 2 : 1
        spacing: 12
        Repeater {
            model: ["dark", "light"]
            Column {
                required property string modelData
                width: (root.width - (previews.columns - 1) * 12) / previews.columns
                spacing: 6
                NacreText {
                    width: parent.width
                    text: (root.entry.modeNames?.[modelData] || modelData) + " · " + (modelData === "dark" ? NacreI18n.tr("software.preview.dark", "Dark") : NacreI18n.tr("software.preview.light", "Light"))
                    wrapMode: Text.Wrap
                }
                Row {
                    width: parent.width
                    spacing: 8
                    Image {
                        width: (parent.width - 8) / 2
                        height: Math.max(110, width * 5 / 8)
                        source: root.entry.wallpapers?.find(item => item.mode === modelData)?.path ? "file://" + root.entry.wallpapers.find(item => item.mode === modelData).path : ""
                        sourceSize.width: 960
                        fillMode: Image.PreserveAspectFit
                        asynchronous: true
                    }
                    NacreThemePreview {
                        width: (parent.width - 8) / 2
                        height: Math.max(110, width * 5 / 8)
                        entry: root.entry
                        mode: modelData
                    }
                }
            }
        }
    }
    NacreText {
        width: parent.width
        text: NacreI18n.tr("software.bundle.credit", "{credit} · MIT palette; Nacre Orbits wallpapers · CC0. Orient adaptation; rendering differs from upstream.", {
            credit: root.entry.upstream?.credit || root.entry.upstreamCredit || ""
        })
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        font.pointSize: 9
        color: NacreTokens.mutedInk
    }
    TextMetrics {
        id: scopeMetrics
        font.family: NacreTokens.textFamily
        font.pointSize: NacreTokens.textSize.body
        text: NacreI18n.tr("software.bundle.both", "Palette and wallpaper")
    }
    Flow {
        width: parent.width
        spacing: 8
        Row {
            width: implicitWidth
            spacing: 8
            NacreMenuButton {
                compact: true
                selected: root.mode === "dark"
                text: NacreI18n.tr("software.preview.dark", "Dark")
                onClicked: root.mode = "dark"
            }
            NacreMenuButton {
                compact: true
                selected: root.mode === "light"
                text: NacreI18n.tr("software.preview.light", "Light")
                onClicked: root.mode = "light"
            }
        }
        NacreSelect {
            width: Math.min(root.width, Math.max(260, scopeMetrics.width + 65))
            model: [NacreI18n.tr("software.bundle.both", "Palette and wallpaper"), NacreI18n.tr("software.bundle.palette", "Palette only"), NacreI18n.tr("software.bundle.wallpaper", "Wallpaper only")]
            currentIndex: ["both", "palette", "wallpaper"].indexOf(root.scope)
            Accessible.name: NacreI18n.tr("software.bundle.apply-scope", "What to apply")
            onActivated: index => root.scope = ["both", "palette", "wallpaper"][index]
        }
        NacreMenuButton {
            objectName: "applyThemeBundle"
            width: Math.min(root.width, implicitWidth)
            enabled: !root.busy
            text: NacreI18n.tr("software.bundle.apply", "Apply selection")
            icon: "check"
            onClicked: root.applyRequested(root.mode, root.scope)
        }
    }
}
