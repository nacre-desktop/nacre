import QtQuick
import qs.utils
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
        AppLaunch.run([NacrePaths.shellBin + "/nacre_shell", "scheme", "set", flag, value]);
    }
    FileView {
        path: NacrePaths.state + "/scheme.json"
        watchChanges: true
        printErrors: false
        onFileChanged: reload()
        onLoaded: {
            try {
                root.scheme = JSON.parse(text());
            } catch (error) {
                console.warn("NacreAppearancePage: Cannot decode appearance scheme:", error);
            }
        }
    }
    readonly property var prefs: NacreWallpapers.appearancePreferences ?? NacreWallpapers.preferences
    NacreSettingsSection {
        title: NacreI18n.tr("settings.language.title", "Language")
        Row {
            spacing: NacreTokens.space.sm
            Repeater {
                model: NacreI18n.developerMode ? ["system", "en", "pseudo"] : ["system", "en"]
                ActionButton {
                    required property string modelData
                    text: modelData === "system" ? NacreI18n.tr("settings.language.system", "System default") : modelData === "en" ? NacreI18n.tr("settings.language.english", "English") : NacreI18n.tr("settings.language.pseudo", "Pseudo")
                    selected: (DesktopSettings.data.language || "system") === modelData
                    onClicked: DesktopSettings.set("language", modelData)
                }
            }
        }
    }
    NacreSettingsSection {
        title: NacreI18n.tr("ui.appearance-page.title.wallpaper", "Wallpaper")
        description: NacreI18n.tr("ui.appearance-page.description.choose-a-scene-from-your-personal-collection", "Choose a scene from your personal collection.")
        Row {
            width: parent.width
            spacing: NacreTokens.space.lg
            Image {
                width: Math.min(300, parent.width * .36)
                height: width * .5
                source: NacreWallpapers.preview ? "file://" + NacreWallpapers.preview : ""
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                cache: true
                retainWhileLoading: true
                sourceSize.width: NacreTokens.space.huge * 20
                sourceSize.height: NacreTokens.space.huge * 11 + NacreTokens.space.sm
            }
            Column {
                width: Math.max(0, parent.width - parent.children[0].width - parent.spacing)
                spacing: NacreTokens.space.lg
                NacreText {
                    width: parent.width
                    text: NacreWallpapers.list.find(w => w.path === NacreWallpapers.current)?.name || NacreWallpapers.current.split("/").pop() || NacreI18n.tr("ui.appearance-page.text.choose-a-wallpaper", "Choose a wallpaper")
                    font.pointSize: NacreTokens.textSize.body
                    elide: Text.ElideRight
                }
                ActionButton {
                    text: NacreI18n.tr("ui.appearance-page.text.wallpaper-picker", "Wallpaper picker")
                    icon: "wallpaper"
                    onClicked: AppLaunch.run([NacrePaths.userBin + "/nacre-shell", "wallpaper"])
                }
            }
        }
    }
    NacreSettingsSection {
        title: NacreI18n.tr("ui.appearance-page.title.wallpaper-motion", "Wallpaper motion")
        description: NacreI18n.tr("ui.appearance-page.description.keep-the-scene-and-colors-while-choosing-how-its-backgr", "Keep the scene and colors while choosing how its background moves.")
        Flow {
            width: parent.width
            spacing: NacreTokens.space.sm
            Repeater {
                model: [
                    {
                        id: "full",
                        nameId: "ui.nacreappearancepage.name.full-motion",
                        name: "Full motion"
                    },
                    {
                        id: "battery",
                        nameId: "ui.nacreappearancepage.name.still-on-battery",
                        name: "Still on battery"
                    },
                    {
                        id: "still",
                        nameId: "ui.nacreappearancepage.name.always-still",
                        name: "Always still"
                    }
                ]
                ActionButton {
                    required property var modelData
                    text: NacreI18n.modelLabel(modelData, "name")
                    selected: (root.prefs.motionMode || "full") === modelData.id
                    onClicked: NacreWallpapers.preference({
                        motionMode: modelData.id
                    })
                }
            }
        }
    }
    NacreSettingsSection {
        title: NacreI18n.tr("ui.appearance-page.title.wallpaper-rotation", "Wallpaper rotation")
        description: NacreI18n.tr("ui.appearance-page.description.automatically-change-scenes-while-you-use-the-desktop", "Automatically change scenes while you use the desktop.")
        Flow {
            width: parent.width
            spacing: NacreTokens.space.sm
            ActionButton {
                text: NacreI18n.tr("ui.appearance-page.text.manual", "Manual")
                selected: !root.prefs.rotationEnabled
                onClicked: NacreWallpapers.preference({
                    rotationEnabled: false
                })
            }
            ActionButton {
                text: NacreI18n.tr("ui.appearance-page.text.rotate-automatically", "Rotate automatically")
                selected: root.prefs.rotationEnabled === true
                onClicked: NacreWallpapers.preference({
                    rotationEnabled: true
                })
            }
            ActionButton {
                text: NacreI18n.tr("ui.appearance-page.text.next-now", "Next now")
                icon: "skip_next"
                enabled: NacreWallpapers.rotationReady
                onClicked: NacreWallpapers.advanceRotation(true)
            }
        }
        Flow {
            width: parent.width
            spacing: NacreTokens.space.sm
            Repeater {
                model: [15, 30, 60, 120]
                ActionButton {
                    required property int modelData
                    text: NacreI18n.tr("ui.appearance-page.text.value0-min-value1", "{value0} min{value1}", {
                        value0: modelData,
                        value1: (modelData === 30 ? NacreI18n.tr("ui.appearance-page.text.recommended", " · recommended") : "")
                    })
                    selected: (root.prefs.rotationMinutes || 30) === modelData
                    onClicked: NacreWallpapers.preference({
                        rotationMinutes: modelData
                    })
                }
            }
        }
        Row {
            width: parent.width
            spacing: NacreTokens.space.md
            NacreText {
                text: NacreI18n.tr("ui.appearance-page.text.custom-interval-minutes", "Custom interval (minutes)")
                font.pointSize: NacreTokens.textSize.caption
                anchors.verticalCenter: parent.verticalCenter
            }
            NacreNumberField {
                objectName: "wallpaperInterval"
                label: NacreI18n.tr("ui.appearance-page.label.wallpaper-interval-in-minutes", "Wallpaper interval in minutes")
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
            text: NacreI18n.tr("ui.appearance-page.text.30-minutes-balances-variety-with-fewer-interruptions-lo", "30 minutes balances variety with fewer interruptions. Longer intervals change scenes less often.")
            font.pointSize: NacreTokens.textSize.caption
            color: NacreTokens.mutedInk
            wrapMode: Text.Wrap
        }
        Flow {
            width: parent.width
            spacing: NacreTokens.space.sm
            Repeater {
                model: [
                    {
                        id: "all",
                        nameId: "ui.nacreappearancepage.name.all-wallpapers",
                        name: "All wallpapers"
                    },
                    {
                        id: "static",
                        nameId: "ui.nacreappearancepage.name.static-only",
                        name: "Static only"
                    },
                    {
                        id: "dynamic",
                        nameId: "ui.nacreappearancepage.name.dynamic-only",
                        name: "Dynamic only"
                    }
                ]
                ActionButton {
                    required property var modelData
                    text: NacreI18n.modelLabel(modelData, "name")
                    selected: (root.prefs.rotationKind || "all") === modelData.id
                    onClicked: NacreWallpapers.preference({
                        rotationKind: modelData.id
                    })
                }
            }
            ActionButton {
                text: NacreI18n.tr("ui.appearance-page.text.shuffle", "Shuffle")
                selected: root.prefs.rotationShuffle !== false
                onClicked: NacreWallpapers.preference({
                    rotationShuffle: true
                })
            }
            ActionButton {
                text: NacreI18n.tr("ui.appearance-page.text.in-order", "In order")
                selected: root.prefs.rotationShuffle === false
                onClicked: NacreWallpapers.preference({
                    rotationShuffle: false
                })
            }
        }
        NacreText {
            width: parent.width
            text: NacreWallpapers.rotationStatus || ""
            font.pointSize: NacreTokens.textSize.caption
            color: NacreTokens.mutedInk
            wrapMode: Text.Wrap
        }
    }
    NacreSettingsSection {
        title: NacreI18n.tr("ui.appearance-page.title.desktop-colors", "Desktop colors")
        description: NacreI18n.tr("ui.appearance-page.description.match-the-wallpaper-or-keep-a-fixed-palette-across-scen", "Match the wallpaper or keep a fixed palette across scenes.")
        Flow {
            width: parent.width
            spacing: NacreTokens.space.sm
            ActionButton {
                text: NacreI18n.tr("ui.appearance-page.text.match-wallpaper", "Match wallpaper")
                selected: (root.prefs.palettePreset || "wallpaper") === "wallpaper"
                enabled: !NacreWallpapers.themeBusy
                onClicked: NacreWallpapers.preference({
                    paletteAccent: "auto"
                })
            }
            ActionButton {
                text: NacreI18n.tr("ui.appearance-page.text.dark", "Dark")
                selected: (root.prefs.paletteMode || "dark") === "dark"
                enabled: !NacreWallpapers.themeBusy
                onClicked: NacreWallpapers.preference({
                    paletteMode: "dark"
                })
            }
            ActionButton {
                text: NacreI18n.tr("ui.appearance-page.text.light", "Light")
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
                const rawName = fixed ? NacreWallpapers.palettePresets.find(p => p.id === root.prefs.palettePreset)?.name : NacreWallpapers.paletteOptions.find(p => p.accent === NacreWallpapers.selectedAccent)?.name;
                const name = rawName ? NacreI18n.option("palette-name", rawName) : fixed ? NacreI18n.tr("ui.appearance-page.text.fixed-palette", "Fixed palette") : NacreI18n.tr("ui.appearance-page.text.wallpaper-colors", "Wallpaper colors");
                const mode = NacreI18n.option("mode", root.prefs.paletteMode === "light" ? "light" : "dark");
                return fixed ? NacreI18n.tr("palette.summary.fixed", "In use: {name} · {mode}", {
                    name: name,
                    mode: mode
                }) : NacreI18n.tr("palette.summary.wallpaper", "In use: {name} · {mode} · {style}", {
                    name: name,
                    mode: mode,
                    style: NacreI18n.option("personality", root.prefs.palettePersonality || (root.prefs.paletteHarmony ? "harmony" : "natural"))
                });
            }
            wrapMode: Text.Wrap
            font.pointSize: NacreTokens.textSize.body
        }
        Flow {
            id: wallColors
            objectName: "wallpaperPaletteOptions"
            width: parent.width
            spacing: NacreTokens.space.compact
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
            spacing: NacreTokens.space.sm
            Repeater {
                model: ["natural", "pigment", "harmony", "pop", "mist", "vivid", "pearl", "tide"]
                ActionButton {
                    required property string modelData
                    objectName: modelData + "PaletteButton"
                    text: NacreI18n.option("personality", modelData)
                    selected: (root.prefs.palettePersonality || "natural") === modelData
                    enabled: (root.prefs.palettePreset || "wallpaper") === "wallpaper" && !NacreWallpapers.themeBusy
                    onClicked: NacreWallpapers.preference({
                        palettePersonality: modelData
                    })
                }
            }
        }
        ActionButton {
            text: NacreI18n.tr("ui.appearance-page.text.explore-in-nacre-colors", "Explore in Nacre Colors")
            icon: "palette"
            onClicked: AppLaunch.run([NacrePaths.userBin + "/nacre-colors"])
        }
        NacreText {
            width: parent.width
            text: NacreI18n.tr("ui.appearance-page.text.natural-restores-the-original-nacre-color-balance-pigme", "Natural restores the original Nacre color balance. Pigment uses the newer subject-weighted color grouping. All styles are also available in Nacre Colors.")
            font.pointSize: NacreTokens.textSize.caption
            color: NacreTokens.mutedInk
            wrapMode: Text.Wrap
        }
        Flow {
            width: parent.width
            spacing: NacreTokens.space.sm
            ActionButton {
                text: NacreI18n.tr("ui.appearance-page.text.vivid-palettes", "Vivid palettes")
                selected: root.paletteGroup === "vivid"
                onClicked: root.paletteGroup = "vivid"
            }
            ActionButton {
                text: NacreI18n.tr("ui.appearance-page.text.soft-palettes", "Soft palettes")
                selected: root.paletteGroup === "soft"
                onClicked: root.paletteGroup = "soft"
            }
        }
        Flow {
            id: presets
            objectName: "paletteOptions"
            width: parent.width
            spacing: NacreTokens.space.compact
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
            font.pointSize: NacreTokens.textSize.caption
        }
    }
    NacreSettingsSection {
        title: NacreI18n.tr("ui.appearance-page.title.palette-style", "Palette style")
        description: NacreI18n.tr("ui.appearance-page.description.optional-style-choices-from-the-same-orient-engine", "Optional style choices from the same Orient engine.")
        Flow {
            width: parent.width
            spacing: NacreTokens.space.sm
            Repeater {
                model: [
                    {
                        id: "tonalspot",
                        nameId: "ui.nacreappearancepage.name.natural",
                        name: "Natural"
                    },
                    {
                        id: "vibrant",
                        nameId: "ui.nacreappearancepage.name.vibrant",
                        name: "Vibrant"
                    },
                    {
                        id: "expressive",
                        nameId: "ui.nacreappearancepage.name.expressive",
                        name: "Expressive"
                    },
                    {
                        id: "fidelity",
                        nameId: "ui.nacreappearancepage.name.faithful",
                        name: "Faithful"
                    },
                    {
                        id: "fruitsalad",
                        nameId: "ui.nacreappearancepage.name.fruit-salad",
                        name: "Fruit salad"
                    },
                    {
                        id: "monochrome",
                        nameId: "ui.nacreappearancepage.name.monochrome",
                        name: "Monochrome"
                    },
                    {
                        id: "neutral",
                        nameId: "ui.nacreappearancepage.name.neutral",
                        name: "Neutral"
                    },
                    {
                        id: "rainbow",
                        nameId: "ui.nacreappearancepage.name.rainbow",
                        name: "Rainbow"
                    },
                    {
                        id: "content",
                        nameId: "ui.nacreappearancepage.name.image-colors",
                        name: "Image colors"
                    }
                ]
                ActionButton {
                    required property var modelData
                    text: NacreI18n.modelLabel(modelData, "name")
                    selected: root.scheme.variant === modelData.id
                    enabled: !NacreWallpapers.themeBusy
                    onClicked: root.changeScheme("-v", modelData.id)
                }
            }
        }
        Flow {
            width: parent.width
            spacing: NacreTokens.space.sm
            ActionButton {
                text: NacreI18n.tr("ui.appearance-page.text.standard-contrast", "Standard contrast")
                selected: root.scheme.flavour !== "hard"
                onClicked: root.changeScheme("-f", "default")
            }
            ActionButton {
                text: NacreI18n.tr("ui.appearance-page.text.high-contrast", "High contrast")
                selected: root.scheme.flavour === "hard"
                onClicked: root.changeScheme("-f", "hard")
            }
        }
    }
    NacreSettingsSection {
        title: NacreI18n.tr("ui.appearance-page.title.workspace-bar", "Workspace bar")
        description: NacreI18n.tr("ui.appearance-page.description.window-symbols-beside-the-active-pearl-turn-them-off-fo", "Window symbols beside the active pearl. Turn them off for a numbered pearls-only bar.")
        NacreSettingToggle {
            label: NacreI18n.tr("ui.appearance-page.label.show-window-symbols", "Show window symbols")
            setting: "workspaceWindowHints"
            defaultChecked: true
        }
    }
    NacreSettingsSection {
        title: NacreI18n.tr("ui.appearance-page.title.desktop-frame", "Desktop frame")
        NacreSettingToggle {
            label: NacreI18n.tr("ui.appearance-page.label.frame-sheen", "Frame sheen")
            setting: "frameSheen"
        }
        NacreText {
            width: parent.width
            text: NacreI18n.tr("ui.appearance-page.text.a-soft-wallpaper-color-rim-around-the-screen-and-open-p", "A soft wallpaper-color rim around the screen and open panels.")
            wrapMode: Text.WordWrap
            color: NacreTokens.mutedInk
            font.pointSize: NacreTokens.textSize.caption
        }
        Row {
            spacing: NacreTokens.space.md
            width: parent.width
            NacreText {
                text: NacreI18n.tr("ui.appearance-page.text.frame-width", "Frame width")
                anchors.verticalCenter: parent.verticalCenter
                font.pointSize: NacreTokens.textSize.body
            }
            NacreNumberField {
                label: NacreI18n.tr("ui.appearance-page.label.frame-width", "Frame width")
                from: 0
                to: 30
                value: DesktopSettings.data.frameWidth ?? 10
                onAdjusted: number => DesktopSettings.set("frameWidth", number)
            }
        }
        Row {
            spacing: NacreTokens.space.md
            width: parent.width
            NacreText {
                text: NacreI18n.tr("ui.appearance-page.text.frame-corners", "Frame corners")
                anchors.verticalCenter: parent.verticalCenter
                font.pointSize: NacreTokens.textSize.body
            }
            NacreNumberField {
                label: NacreI18n.tr("ui.appearance-page.label.frame-corner-radius", "Frame corner radius")
                from: 0
                to: 40
                value: DesktopSettings.data.frameRounding ?? 25
                onAdjusted: number => DesktopSettings.set("frameRounding", number)
            }
        }
        Repeater {
            model: [
                {
                    key: "topBarEnabled",
                    label: NacreI18n.tr("ui.appearance-page.label.top-menu", "Top menu")
                },
                {
                    key: "frameLeftEnabled",
                    label: NacreI18n.tr("ui.appearance-page.label.left-edge", "Left edge")
                },
                {
                    key: "frameRightEnabled",
                    label: NacreI18n.tr("ui.appearance-page.label.right-edge", "Right edge")
                },
                {
                    key: "frameBottomEnabled",
                    label: NacreI18n.tr("ui.appearance-page.label.bottom-edge", "Bottom edge")
                },
                {
                    key: "aiSidebarEnabled",
                    label: NacreI18n.tr("ui.appearance-page.label.ai-sidebar", "AI sidebar")
                },
                {
                    key: "livePreviews",
                    label: NacreI18n.tr("ui.appearance-page.label.live-previews", "Live previews")
                },
                {
                    key: "nativePalette",
                    label: NacreI18n.tr("ui.appearance-page.label.command-palette", "Command palette")
                },
                {
                    key: "nativeOverview",
                    label: NacreI18n.tr("ui.appearance-page.label.window-overview", "Window overview")
                },
                {
                    key: "nativeClipboard",
                    label: NacreI18n.tr("ui.appearance-page.label.clipboard-picker", "Clipboard picker")
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
