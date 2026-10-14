"""Stage unchanged Settings/panel UI with synthetic, isolated service adapters."""

import json
from pathlib import Path
import shutil
import subprocess
import hashlib
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
SHELL = ROOT / "nacre/shell"


def singleton(folder, name, body, imports=""):
    (folder / (name + ".qml")).write_text(
        "pragma Singleton\nimport QtQuick\nimport Quickshell\n"
        "import qs.modules.settings.prototype\n"
        + imports
        + "\nSingleton {\n"
        + body
        + "\n}\n"
    )


def stage(target, *, icons=False):
    target = Path(target)
    target.mkdir(parents=True, exist_ok=True)
    for relative in ["modules/settings", "i18n"]:
        shutil.copytree(SHELL / relative, target / relative, dirs_exist_ok=True)
    for relative, names in {
        "widgets": [
            "NacreTokens",
            "NacreClip",
            "NacreSurface",
            "NacreText",
            "NacreIcon",
            "NacreInteraction",
            "NacreTextField",
            "NacreScrollBar",
            "FastScroll",
            "ActionButton",
        ],
        "modules/dashboard": [
            "NacreDashboardCustomize",
            "NacreDashboardCustomizeSection",
            "NacreDashboardToggleRow",
            "NacreDashboardExtrasOptions",
            "NacreDashboardNavigation",
        ],
        "modules/controlcenter": [
            "NacreControlEdit",
            "NacreControlCatalog",
            "NacreControlTile",
            "NacreControlButton",
        ],
    }.items():
        folder = target / relative
        folder.mkdir(parents=True, exist_ok=True)
        for name in names:
            shutil.copy2(SHELL / relative / (name + ".qml"), folder)
    shutil.copy2(SHELL / "widgets/size-scale.json", target / "widgets/size-scale.json")
    # Rendering uses a gallery-only copy of the pinned icon font. Qt tests do
    # not need the font and must work in CI before the font gallery step.
    if icons:
        asset = next(
            group
            for group in json.loads(
                (ROOT / "nacre/shell-tools/font-assets.json").read_text()
            )["groups"]
            if group["id"] == "material"
        )
        for row in asset["files"]:
            installed = subprocess.check_output(
                ["fc-match", "-f", "%{file}", "Material Symbols Rounded"], text=True
            )
            candidate = Path(installed)
            raw = candidate.read_bytes() if candidate.name == row["name"] else b""
            if (
                len(raw) != row["size"]
                or hashlib.sha256(raw).hexdigest() != row["sha256"]
            ):
                with urllib.request.urlopen(row["url"], timeout=30) as response:
                    raw = response.read(row["size"] + 1)
            if (
                len(raw) != row["size"]
                or hashlib.sha256(raw).hexdigest() != row["sha256"]
            ):
                raise RuntimeError(
                    "Pinned preview font checksum mismatch: " + row["name"]
                )
            (target / row["name"]).write_bytes(raw)
    (target / "utils").mkdir()
    shutil.copy2(SHELL / "utils/NacrePaths.qml", target / "utils/NacrePaths.qml")
    services = target / "services"
    services.mkdir()
    shutil.copy2(SHELL / "services/NacreDashboardRegistry.qml", services)
    singleton(
        services,
        "NacreI18n",
        """
    readonly property var messages: JSON.parse(file.text())
    readonly property var options: JSON.parse(optionFile.text())
    function tr(id, fallback, args) {
        let value = messages[id] || fallback || id;
        for (const key of Object.keys(args || {})) value = value.replace("{" + key + "}", args[key]);
        return value;
    }
    function option(domain, value) {
        const row = options[domain]?.[value];
        return row ? tr(row.id, row.text || value) : value;
    }
    FileView {id:file;path:Qt.resolvedUrl("../i18n/en.json");blockLoading:true}
    FileView {id:optionFile;path:Qt.resolvedUrl("../i18n/options.json");blockLoading:true}
    """,
        "import Quickshell.Io",
    )
    palette = {
        "m3surface": "#18232a",
        "m3surfaceContainer": "#223139",
        "m3surfaceContainerHigh": "#30434d",
        "m3frame": "#152128",
        "m3onSurface": "#e2ebef",
        "m3onSurfaceVariant": "#abbec7",
        "m3outline": "#6f8792",
        "m3outlineVariant": "#3e535e",
        "m3primary": "#99cbd9",
        "m3onPrimary": "#10343f",
        "m3primaryContainer": "#294e5b",
        "m3onPrimaryContainer": "#d3eff6",
        "m3secondary": "#b5caca",
        "m3secondaryContainer": "#354a4d",
        "m3onSecondaryContainer": "#e6eeee",
        "m3tertiary": "#cbbaca",
        "m3error": "#ffc4be",
        "m3inverseSurface": "#e2ebef",
        "m3inverseOnSurface": "#18232a",
    }
    singleton(
        services,
        "NacreColours",
        "property bool light:false;property bool publishing:false;property var palette:("
        + json.dumps(palette)
        + ")",
    )
    singleton(
        services,
        "DesktopSettings",
        """
    readonly property var data: Object.assign({interfaceFont:"DejaVu Sans"}, SettingsPreviewContext.store?.files["desktop.json"] || {})
    function set(key,value){SettingsPreviewContext.backend?.change("desktop.json",[key],value)}
    """,
    )
    singleton(
        services,
        "NacreDashboardState",
        """
    readonly property var preferences: Object.assign({tabs:["today","media","weather","system"],lyrics:true,homeZone:"",homeCity:"",calendarFile:"",focusMinutes:25},SettingsPreviewContext.store?.files["dashboard.json"] || {})
    readonly property var coreTabs: preferences.tabs
    readonly property var extraOrder: SettingsPreviewContext.store?.files["dashboard-extras.json"]?.order || ["focus","notifications","clipboard","devices","network","ai-usage","screenshots","brain"]
    readonly property var enabledExtras: SettingsPreviewContext.store?.files["dashboard-extras.json"]?.enabled || []
    readonly property var tabs: [...coreTabs,...extraOrder.filter(id=>enabledExtras.includes(id))]
    property bool extrasReady:true
    property bool weatherPhotos:true
    property string message:""
    function save(key,value){SettingsPreviewContext.backend?.change("dashboard.json",[key],value)}
    function saveMany(values){for(const key of Object.keys(values)) save(key,values[key])}
    function toggleTab(id){const next=coreTabs.includes(id)?coreTabs.filter(x=>x!==id):[...coreTabs,id];if(next.length)save("tabs",next)}
    function reorder(id,delta){const next=[...coreTabs],index=next.indexOf(id),after=Math.max(0,Math.min(next.length-1,index+delta));if(index>=0){next.splice(index,1);next.splice(after,0,id);save("tabs",next)}}
    function toggleExtra(id){SettingsPreviewContext.backend?.change("dashboard-extras.json",["enabled"],enabledExtras.includes(id)?enabledExtras.filter(x=>x!==id):[...enabledExtras,id])}
    function reorderExtra(id,delta){const next=[...extraOrder],index=next.indexOf(id),after=Math.max(0,Math.min(next.length-1,index+delta));next.splice(index,1);next.splice(after,0,id);SettingsPreviewContext.backend?.change("dashboard-extras.json",["order"],next)}
    function saveExtras(){SettingsPreviewContext.backend?.change("dashboard-extras.json",["weatherPhotos"],weatherPhotos)}
    """,
    )
    singleton(
        services,
        "NacreControlPreferences",
        """
    readonly property var defaults:["volume","brightness","network","bluetooth","battery","night","dnd","awake","appearance"]
    readonly property var tiles:SettingsPreviewContext.store?.files["control-center.json"]?.tiles || defaults
    readonly property var optionalTiles:tiles.filter(id=>!defaults.includes(id))
    property string error:""
    function save(values){SettingsPreviewContext.backend?.change("control-center.json",["tiles"],values)}
    function toggle(id){save(tiles.includes(id)?tiles.filter(x=>x!==id):[...tiles,id])}
    function move(id,delta,visibleIds){const next=[...tiles],visible=visibleIds || tiles,index=visible.indexOf(id),other=visible[index+delta];if(other){const first=next.indexOf(id),second=next.indexOf(other);[next[first],next[second]]=[next[second],next[first]];save(next)}}
    function reset(){save(defaults)}
    """,
    )
    simple = {
        "NacreNetwork": 'property bool wiredConnected:false;property bool wifiEnabled:true;property bool wifiAvailable:true;property var active:({ssid:"Demo network"})',
        "NacreBluetooth": "property bool powered:true;property bool available:true;property var devices:[]",
        "NacreControlTools": "property bool busy:false;property var data:({nightLightSupported:true,nightLightEnabled:false,powerProfilesSupported:true,screenshot:true,colorPicker:true})",
        "NacreControlActions": "property bool busy:false",
        "NacreNotifs": "property bool dnd:false;property var retained:[]",
        "NacreMenu": "property var snapshot:({stayAwake:false,capabilities:{record:true}})",
        "NacreVpn": "property var connected:[];property var profiles:[];property bool busy:false",
        "NacreAudio": "property bool micAvailable:true;property bool micMuted:false",
        "NacreBrightness": "function getMonitorForScreen(screen){return {available:true}}",
        "NacreKeyboardLight": "property bool available:true",
        "NacreProfile": "property bool ai:true;property bool brain:true",
    }
    for name, body in simple.items():
        singleton(services, name, body)
    (target / "shell.qml").write_text(
        "import Quickshell\nimport qs.modules.settings.prototype\nShellRoot {SettingsPreview {}}\n"
    )
    for folder in [target, *[p for p in target.rglob("*") if p.is_dir()]]:
        rows = []
        for path in folder.glob("*.qml"):
            if path.name == "shell.qml":
                continue
            prefix = (
                "singleton " if path.read_text().startswith("pragma Singleton") else ""
            )
            rows.append(f"{prefix}{path.stem} 1.0 {path.name}")
        if rows:
            manifest = folder / "qmldir"
            declared = manifest.read_text().splitlines()[0] if manifest.exists() else ""
            if declared.startswith("module "):
                rows.insert(0, declared)
            manifest.write_text("\n".join(rows) + "\n")
    return target


def state():
    return {
        "version": 1,
        "files": {
            "desktop.json": {
                "clickEdgeMenus": False,
                "detachablePanels": True,
                "dragPanels": True,
                "topEdge": True,
                "rightEdge": True,
                "leftDrawer": True,
            },
            "wallpaper-picker.json": {
                "palettePreset": "wallpaper",
                "paletteMode": "dark",
                "palettePersonality": "natural",
            },
            "state:scheme.json": {"flavour": "default"},
            "launcher.json": {
                "favorites": ["demo-browser.desktop"],
                "hidden": [],
                "recent": ["demo-editor.desktop"],
                "usage": {"demo-editor.desktop": {"count": 2}},
            },
            "dashboard.json": {
                "tabs": ["today", "media", "weather", "system"],
                "lyrics": True,
            },
            "dashboard-extras.json": {
                "version": 1,
                "enabled": [],
                "order": [
                    "focus",
                    "notifications",
                    "clipboard",
                    "devices",
                    "network",
                    "ai-usage",
                    "screenshots",
                    "brain",
                ],
                "weatherPhotos": True,
            },
            "control-center.json": {
                "version": 2,
                "tiles": [
                    "volume",
                    "brightness",
                    "network",
                    "bluetooth",
                    "battery",
                    "night",
                    "dnd",
                    "awake",
                    "appearance",
                ],
            },
        },
    }
