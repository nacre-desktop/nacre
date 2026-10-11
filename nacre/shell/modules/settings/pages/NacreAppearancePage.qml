import QtQuick
import Quickshell
import Quickshell.Io
import qs.widgets
import qs.services

NacreSettingsPage {
    id: root
    property string paletteGroup: "vivid"
    property var scheme: ({
            variant: "tonalspot",
            flavour: "default"
        })
    function changeScheme(flag, value) {
        AppLaunch.run([Quickshell.env("HOME") + "/.local/share/nacre/shell/bin/nacre_shell", "scheme", "set", flag, value]);
    }
    FileView {
        path: (Quickshell.env("XDG_STATE_HOME") || Quickshell.env("HOME") + "/.local/state") + "/nacre/scheme.json"
        watchChanges: true
        printErrors: false
        onFileChanged: reload()
        onLoaded: {
            try {
                root.scheme = JSON.parse(text());
            } catch (error) {}
        }
    }
    readonly property var prefs: NacreWallpapers.appearancePreferences ?? NacreWallpapers.preferences
    NacreSettingsSection {
        title: "Wallpaper"
        description: "Choose a scene from your personal collection."
        Row {
            width: parent.width
            spacing: 16
            Image {
                width: Math.min(300, parent.width * .36)
                height: width * .5
                source: NacreWallpapers.preview ? "file://" + NacreWallpapers.preview : ""
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                cache: true
                retainWhileLoading: true
                sourceSize.width: 640
                sourceSize.height: 360
            }
            Column {
                width: Math.max(0, parent.width - parent.children[0].width - parent.spacing)
                spacing: 14
                NacreText {
                    width: parent.width
                    text: NacreWallpapers.list.find(w => w.path === NacreWallpapers.current)?.name || NacreWallpapers.current.split("/").pop() || "Choose a wallpaper"
                    font.pointSize: 12
                    elide: Text.ElideRight
                }
                ActionButton {
                    text: "Wallpaper picker"
                    icon: "wallpaper"
                    onClicked: AppLaunch.run([Quickshell.env("HOME") + "/.local/bin/nacre-shell", "wallpaper"])
                }
            }
        }
    }
    NacreSettingsSection {
        title: "Wallpaper motion"
        description: "Keep the scene and colors while choosing how its background moves."
        Flow {
            width: parent.width
            spacing: 8
            Repeater {
                model: [
                    {
                        id: "full",
                        name: "Full motion"
                    },
                    {
                        id: "battery",
                        name: "Still on battery"
                    },
                    {
                        id: "still",
                        name: "Always still"
                    }
                ]
                ActionButton {
                    required property var modelData
                    text: modelData.name
                    selected: (root.prefs.motionMode || "full") === modelData.id
                    onClicked: NacreWallpapers.preference({
                        motionMode: modelData.id
                    })
                }
            }
        }
    }
    NacreSettingsSection {
        title: "Wallpaper rotation"
        description: "Automatically change scenes while you use the desktop."
        Flow {
            width: parent.width
            spacing: 8
            ActionButton {
                text: "Manual"
                selected: !root.prefs.rotationEnabled
                onClicked: NacreWallpapers.preference({
                    rotationEnabled: false
                })
            }
            ActionButton {
                text: "Rotate automatically"
                selected: root.prefs.rotationEnabled === true
                onClicked: NacreWallpapers.preference({
                    rotationEnabled: true
                })
            }
            ActionButton {
                text: "Next now"
                icon: "skip_next"
                enabled: NacreWallpapers.rotationReady
                onClicked: NacreWallpapers.advanceRotation(true)
            }
        }
        Flow {
            width: parent.width
            spacing: 8
            Repeater {
                model: [15, 30, 60, 120]
                ActionButton {
                    required property int modelData
                    text: modelData + " min" + (modelData === 30 ? " · recommended" : "")
                    selected: (root.prefs.rotationMinutes || 30) === modelData
                    onClicked: NacreWallpapers.preference({
                        rotationMinutes: modelData
                    })
                }
            }
        }
        Row {
            width: parent.width
            spacing: 12
            NacreText {
                text: "Custom interval (minutes)"
                font.pointSize: 10
                anchors.verticalCenter: parent.verticalCenter
            }
            NacreNumberField {
                objectName: "wallpaperInterval"
                label: "Wallpaper interval in minutes"
                from: 5
                to: 1440
                value: root.prefs.rotationMinutes || 30
                onAdjusted: number => NacreWallpapers.preference({
                        rotationMinutes: number
                    })
            }
        }
        NacreText {
            width: parent.width
            text: "30 minutes balances variety with fewer interruptions. Longer intervals change scenes less often."
            font.pointSize: 10
            color: NacreTokens.mutedInk
            wrapMode: Text.Wrap
        }
        Flow {
            width: parent.width
            spacing: 8
            Repeater {
                model: [
                    {
                        id: "all",
                        name: "All wallpapers"
                    },
                    {
                        id: "static",
                        name: "Static only"
                    },
                    {
                        id: "dynamic",
                        name: "Dynamic only"
                    }
                ]
                ActionButton {
                    required property var modelData
                    text: modelData.name
                    selected: (root.prefs.rotationKind || "all") === modelData.id
                    onClicked: NacreWallpapers.preference({
                        rotationKind: modelData.id
                    })
                }
            }
            ActionButton {
                text: "Shuffle"
                selected: root.prefs.rotationShuffle !== false
                onClicked: NacreWallpapers.preference({
                    rotationShuffle: true
                })
            }
            ActionButton {
                text: "In order"
                selected: root.prefs.rotationShuffle === false
                onClicked: NacreWallpapers.preference({
                    rotationShuffle: false
                })
            }
        }
        NacreText {
            width: parent.width
            text: NacreWallpapers.rotationStatus || ""
            font.pointSize: 10
            color: NacreTokens.mutedInk
            wrapMode: Text.Wrap
        }
    }
    NacreSettingsSection {
        title: "Desktop colors"
        description: "Match the wallpaper or keep a fixed palette across scenes."
        Flow {
            width: parent.width
            spacing: 8
            ActionButton {
                text: "Match wallpaper"
                selected: (root.prefs.palettePreset || "wallpaper") === "wallpaper"
                enabled: !NacreWallpapers.themeBusy
                onClicked: NacreWallpapers.preference({
                    paletteAccent: "auto"
                })
            }
            ActionButton {
                text: "Dark"
                selected: (root.prefs.paletteMode || "dark") === "dark"
                enabled: !NacreWallpapers.themeBusy
                onClicked: NacreWallpapers.preference({
                    paletteMode: "dark"
                })
            }
            ActionButton {
                text: "Light"
                selected: root.prefs.paletteMode === "light"
                enabled: !NacreWallpapers.themeBusy
                onClicked: NacreWallpapers.preference({
                    paletteMode: "light"
                })
            }
        }
        NacreText {
            objectName: "activePaletteSummary"
            width: parent.width
            text: {
                const fixed = root.prefs.palettePreset && root.prefs.palettePreset !== "wallpaper";
                const name = fixed ? NacreWallpapers.palettePresets.find(p => p.id === root.prefs.palettePreset)?.name : NacreWallpapers.paletteOptions.find(p => p.accent === NacreWallpapers.selectedAccent)?.name;
                return "In use: " + (name || (fixed ? "Fixed palette" : "Wallpaper colors")) + " · " + (root.prefs.paletteMode === "light" ? "Light" : "Dark") + (fixed ? "" : " · " + ((root.prefs.palettePersonality || (root.prefs.paletteHarmony ? "harmony" : "natural")).replace(/^./, s => s.toUpperCase())));
            }
            wrapMode: Text.Wrap
            font.pointSize: 11
        }
        Flow {
            id: wallColors
            objectName: "wallpaperPaletteOptions"
            width: parent.width
            spacing: 10
            readonly property int columns: Math.max(1, Math.min(4, Math.floor((width + 10) / 170)))
            Repeater {
                model: NacreWallpapers.paletteOptions
                NacrePaletteTile {
                    required property var modelData
                    objectName: "wallpaperPaletteTile"
                    entry: modelData
                    width: Math.floor((wallColors.width - (wallColors.columns - 1) * 10) / wallColors.columns)
                    selected: (root.prefs.palettePreset || "wallpaper") === "wallpaper" && NacreWallpapers.selectedAccent === modelData.accent
                    enabled: !NacreWallpapers.themeBusy
                    onChosen: NacreWallpapers.preference({
                        paletteAccent: modelData.accent
                    })
                }
            }
        }
        Flow {
            width: parent.width
            spacing: 8
            Repeater {
                model: ["natural", "pigment", "harmony", "pop", "mist", "vivid", "pearl", "tide"]
                ActionButton {
                    required property string modelData
                    objectName: modelData + "PaletteButton"
                    text: modelData.charAt(0).toUpperCase() + modelData.slice(1)
                    selected: (root.prefs.palettePersonality || "natural") === modelData
                    enabled: (root.prefs.palettePreset || "wallpaper") === "wallpaper" && !NacreWallpapers.themeBusy
                    onClicked: NacreWallpapers.preference({
                        palettePersonality: modelData
                    })
                }
            }
        }
        ActionButton {
            text: "Explore in Nacre Colors"
            icon: "palette"
            onClicked: AppLaunch.run([Quickshell.env("HOME") + "/.local/bin/nacre-colors"])
        }
        NacreText {
            width: parent.width
            text: "Natural restores the original Nacre color balance. Pigment uses the newer subject-weighted color grouping. All styles are also available in Nacre Colors."
            font.pointSize: 10
            color: NacreTokens.mutedInk
            wrapMode: Text.Wrap
        }
        Flow {
            width: parent.width
            spacing: 8
            ActionButton {
                text: "Vivid palettes"
                selected: root.paletteGroup === "vivid"
                onClicked: root.paletteGroup = "vivid"
            }
            ActionButton {
                text: "Soft palettes"
                selected: root.paletteGroup === "soft"
                onClicked: root.paletteGroup = "soft"
            }
        }
        Flow {
            id: presets
            objectName: "paletteOptions"
            width: parent.width
            spacing: 10
            readonly property int columns: Math.max(1, Math.min(4, Math.floor((width + 10) / 170)))
            Repeater {
                model: NacreWallpapers.palettePresets.filter(p => p.group === root.paletteGroup)
                NacrePaletteTile {
                    required property var modelData
                    objectName: "paletteColorTile"
                    entry: modelData
                    width: Math.floor((presets.width - (presets.columns - 1) * 10) / presets.columns)
                    selected: root.prefs.palettePreset === modelData.id
                    enabled: !NacreWallpapers.themeBusy
                    onChosen: NacreWallpapers.preference({
                        palettePreset: modelData.id
                    })
                }
            }
        }
        NacreText {
            width: parent.width
            text: NacreWallpapers.error || ""
            visible: text.length > 0
            color: NacreTokens.accent
            wrapMode: Text.Wrap
            font.pointSize: 10
        }
    }
    NacreSettingsSection {
        title: "Palette style"
        description: "Optional style choices from the same Orient engine."
        Flow {
            width: parent.width
            spacing: 8
            Repeater {
                model: [
                    {
                        id: "tonalspot",
                        name: "Natural"
                    },
                    {
                        id: "vibrant",
                        name: "Vibrant"
                    },
                    {
                        id: "expressive",
                        name: "Expressive"
                    },
                    {
                        id: "fidelity",
                        name: "Faithful"
                    },
                    {
                        id: "fruitsalad",
                        name: "Fruit salad"
                    },
                    {
                        id: "monochrome",
                        name: "Monochrome"
                    },
                    {
                        id: "neutral",
                        name: "Neutral"
                    },
                    {
                        id: "rainbow",
                        name: "Rainbow"
                    },
                    {
                        id: "content",
                        name: "Image colors"
                    }
                ]
                ActionButton {
                    required property var modelData
                    text: modelData.name
                    selected: root.scheme.variant === modelData.id
                    enabled: !NacreWallpapers.themeBusy
                    onClicked: root.changeScheme("-v", modelData.id)
                }
            }
        }
        Flow {
            width: parent.width
            spacing: 8
            ActionButton {
                text: "Standard contrast"
                selected: root.scheme.flavour !== "hard"
                onClicked: root.changeScheme("-f", "default")
            }
            ActionButton {
                text: "High contrast"
                selected: root.scheme.flavour === "hard"
                onClicked: root.changeScheme("-f", "hard")
            }
        }
    }
    NacreSettingsSection {
        title: "Desktop frame"
        NacreSettingToggle {
            label: "Frame sheen"
            setting: "frameSheen"
        }
        NacreText {
            width: parent.width
            text: "A soft wallpaper-color rim around the screen and open panels."
            wrapMode: Text.WordWrap
            color: NacreTokens.mutedInk
            font.pointSize: 10
        }
        Row {
            spacing: 12
            width: parent.width
            NacreText {
                text: "Frame width"
                anchors.verticalCenter: parent.verticalCenter
                font.pointSize: 11
            }
            NacreNumberField {
                label: "Frame width"
                from: 0
                to: 30
                value: DesktopSettings.data.frameWidth ?? 10
                onAdjusted: number => DesktopSettings.set("frameWidth", number)
            }
        }
        Row {
            spacing: 12
            width: parent.width
            NacreText {
                text: "Frame corners"
                anchors.verticalCenter: parent.verticalCenter
                font.pointSize: 11
            }
            NacreNumberField {
                label: "Frame corner radius"
                from: 0
                to: 40
                value: DesktopSettings.data.frameRounding ?? 25
                onAdjusted: number => DesktopSettings.set("frameRounding", number)
            }
        }
        Repeater {
            model: [
                {
                    key: "topEdge",
                    label: "Top menu"
                },
                {
                    key: "leftEdge",
                    label: "Left edge"
                },
                {
                    key: "rightEdge",
                    label: "Right edge"
                },
                {
                    key: "bottomEdge",
                    label: "Bottom edge"
                },
                {
                    key: "leftDrawer",
                    label: "AI sidebar"
                },
                {
                    key: "livePreviews",
                    label: "Live previews"
                },
                {
                    key: "nativePalette",
                    label: "Command palette"
                },
                {
                    key: "nativeOverview",
                    label: "Window overview"
                },
                {
                    key: "nativeClipboard",
                    label: "Clipboard picker"
                }
            ]
            NacreSettingToggle {
                required property var modelData
                label: modelData.label
                setting: modelData.key
            }
        }
    }
}
