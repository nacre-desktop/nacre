from qml_source import install_i18n, install_paths

"""Real universal menu and model, with only external owners replaced by fixtures."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

from qml_source import install_foundation_interaction, remove_objects

TOOLS = Path(__file__).resolve().parents[1]
SHELL = TOOLS.parent / "shell"


def adapt(source):
    source = source.replace("import qs.utils", 'import "."')
    for name in ("Quickshell.Services.UPower", "Quickshell.Io", "Quickshell"):
        source = source.replace("import " + name + "\n", "")
    for name in ("qs.widgets", "qs.services"):
        source = source.replace("import " + name, 'import "fixtures"')
    source = source.replace("Quickshell.screens[0].width", "1920").replace(
        "Quickshell.screens[0].height", "1200"
    )
    source = source.replace(
        'Quickshell.iconPath(root.entry.icon || "application-x-executable", true)', '""'
    )
    source = remove_objects(source, r"\bIpcHandler\s*\{")
    return source


def stage(target):
    fixtures = target / "fixtures"
    shutil.copytree(TOOLS / "tests/qml/fixtures", fixtures)
    install_foundation_interaction(fixtures, SHELL / "widgets")
    for name in (
        "NacreAppBrowser",
        "NacreMenuIcon",
        "NacreMenuForm",
        "NacreMenuButton",
    ):
        (target / (name + ".qml")).write_text(
            adapt((SHELL / "modules/launcher" / (name + ".qml")).read_text())
        )
    for name in ("menu-model.js", "app-browser.js", "calculator.js"):
        shutil.copy2(SHELL / "modules/launcher" / name, target / name)
    source = adapt((SHELL / "services/NacreMenuContext.qml").read_text()).replace(
        "Scope {", "QtObject {", 1
    )
    source = source.replace(
        '"../modules/launcher/menu-model.js"', '"../menu-model.js"'
    ).replace('"../modules/launcher/app-browser.js"', '"../app-browser.js"')
    for kind in ("Process", "NacreHelperTask", "Timer", "FileView"):
        source = remove_objects(source, r"\b" + kind + r"\s*\{")
    source = source.replace(
        'property string defaultText: ""',
        "property string defaultText: "
        + json.dumps((TOOLS / "launcher-menu.json").read_text()),
    )
    source = source.replace(
        "Component.onCompleted: if (!shared)\n        refresh()",
        "Component.onCompleted: parse()",
    )
    source = (
        source.replace("PowerProfile.PowerSaver", "0")
        .replace("PowerProfile.Balanced", "1")
        .replace("PowerProfile.Performance", "2")
    )
    source = source.replace('Quickshell.env("HOME")', '"/fixture"')
    source = source.replace(
        "id: root",
        """id: root
    property QtObject brainDelay: QtObject {function restart(){}}
    property QtObject stateWorker: QtObject { property bool running:false;property bool force:false }
    property QtObject actionWorker: QtObject { property bool running:false;property string action:"";property string value:"";property var payload:({}) }
    property QtObject ownershipWorker: QtObject { property bool running:false;property string ident:"" }
    property QtObject packageDelay: QtObject { function restart(){} }
    property QtObject fileDelay: QtObject { function restart(){} }
    property QtObject conversionDelay: QtObject { function restart(){} }
    property QtObject captureDelay: QtObject { property string value:"";function restart(){} }
""",
        1,
    )
    source = "pragma Singleton\n" + source.replace("    visible: false\n", "")
    source = source.replace("NacreAiViews.edge", "SidebarChat")
    (fixtures / "NacreMenu.qml").write_text(source)
    definitions = {
        "NacreApps": """property var all:[{id:"editor",name:"Editor",icon:"",comment:"Code"},{id:"google-chrome",name:"Google Chrome",icon:"",comment:"Web browser"},{id:"nacre-ai",name:"Nacre AI",icon:"",comment:"Assistant"},{id:"music",name:"Music",icon:"",comment:"Player"}];readonly property var list:all.filter(row=>!LauncherPreferences.hidden.includes(row.id));property string launched:"";function launch(entry){launched=entry.id;return true}""",
        "LauncherPreferences": """property var favorites:[];property var hidden:[];property var recent:[];property var usage:({});property string error:"";property bool ready:true;function update(action,id,enabled){const key=action==="favorite"?"favorites":action==="hide"?"hidden":"recent";let result=this[key].filter(value=>value!==id);if(enabled)result.push(id);this[key]=result;}""",
        "DesktopActions": """property var list:[{name:"Open terminal",action:"terminal",icon:"terminal",description:"Terminal"},{name:"Disable blur",action:"toggle-setting",value:"blur",icon:"blur_on",launcher:false}];property var calls:[];function execute(action,value){calls=[...calls,[action,value]];}""",
        "AppLaunch": """property var commands:[];function run(command){commands=[...commands,command];}""",
        "DesktopSettings": """property var data:({animations:true,blur:true,shadow:true,dnd:false,interfaceFont:""});property var changes:[];function set(key,value){data=Object.assign({},data,{[key]:value});changes=[...changes,[key,value]];}""",
        "NacreWallpapers": """property var list:[{path:"/fixture/blue.png",name:"Blue garden",dynamic:false,thumbnail:""}];property string current:"/fixture/blue.png";property var appearancePreferences:({palettePersonality:"natural",paletteMode:"dark",paletteBackgroundFromWallpaper:false});property var paletteOptions:[{name:"Blue",accent:"blue",colors:[]}];property string selectedAccent:"blue";property var preferences:[];function preference(value){preferences=[...preferences,value];appearancePreferences=Object.assign({},appearancePreferences,value);}function setWallpaper(value){current=value;}""",
        "NacreControlTools": """property bool busy:false;property var data:({nightLightSupported:true,nightLightEnabled:false,nightLightExternal:false});function refresh(){}function toggleNightLight(){data=Object.assign({},data,{nightLightEnabled:!data.nightLightEnabled});}""",
        "NacreAudio": """property var outputs:[];property var sink:null;function chooseOutput(id){return true;}property bool available:true;property bool micAvailable:true;property bool muted:false;property bool micMuted:false;function toggleMute(){muted=!muted;}function toggleMic(){micMuted=!micMuted;}""",
        "NacrePlayers": """property var active:({canTogglePlaying:true,canGoNext:true,canGoPrevious:true});property var actions:[];function control(value){actions=[...actions,value];return true;}""",
        "NacreNetwork": """property bool wifiAvailable:true;property bool wifiEnabled:true;property var visibleNetworks:[]""",
        "NacreBluetooth": """property bool available:true;property QtObject adapter:QtObject{property bool enabled:true};readonly property bool powered:adapter.enabled;property var devices:[]""",
        "NacreCompositor": """property var windows:[];property var calls:[];function focusWindow(id){calls=[...calls,"focus "+id]}function logout(){calls=[...calls,"logout"]}""",
        "DesktopExtras": """property var notes:[];property string notesQuery:"";property string brainQuery:"";property var clips:[];property var keys:[];property var requests:[];function request(action,payload){requests=[...requests,[action,payload]];}""",
        "Maintenance": """property var data:({services:{desktop:"active"}});function refresh(){}""",
        "Updates": """property int count:12""",
        "SidebarChat": """property bool busy:false;property bool inWorkspace:false;property string draft:"";property var attachments:[];property var commands:[];function command(value){commands=[...commands,value];}""",
        "NacreMenuPrompt": """property bool active:false;property var request:null;property var ownerView:null;property var result:[];function finish(value,cancelled){result=[value,cancelled];active=false;request=null;}""",
        "NacrePanelState": """property QtObject view:QtObject{property bool launcher:true;property bool left:false;property bool leftPinned:false};property var calls:[];function getForActive(){return view;}function openSettings(page){calls=[...calls,["settings",page]];}function openMode(mode){calls=[...calls,["mode",mode]];}""",
        "PowerProfiles": """property int profile:1;property bool hasPerformanceProfile:true""",
    }
    with (fixtures / "qmldir").open("a") as manifest:
        manifest.write("\nsingleton NacreMenu 1.0 NacreMenu.qml\n")
        for name, body in definitions.items():
            (fixtures / (name + ".qml")).write_text(
                "pragma Singleton\nimport QtQuick\nQtObject{" + body + "}\n"
            )
            if name not in (fixtures / "qmldir").read_text():
                manifest.write("singleton " + name + " 1.0 " + name + ".qml\n")
    button = fixtures / "ActionButton.qml"
    button.write_text(
        button.read_text().replace(
            "property bool selected:",
            "property bool compact:false;property bool selected:",
        )
    )
    # These are visual/external icon contracts, not the menu logic being tested.
    (fixtures / "NacreIcon.qml").write_text(
        "import QtQuick\nText {property real fill:0}\n"
    )
    (fixtures / "BrandLogo.qml").write_text(
        'import QtQuick\nRectangle {property bool ai:false;property bool brain:false;property bool settings:false;property bool colorsApp:false;color:"#85a9d1"}\n'
    )
    with (fixtures / "qmldir").open("a") as manifest:
        manifest.write("\nBrandLogo 1.0 BrandLogo.qml\n")
    shutil.copy2(SHELL / "widgets/FastScroll.qml", fixtures / "FastScroll.qml")
    (target / "tst_menu.qml").write_text(
        (TOOLS / "tests/launcher-qml/tst_universal.qml").read_text()
    )


class UniversalMenuUITests(unittest.TestCase):
    def test_routes_search_keyboard_favorites_and_guards(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder)
            stage(target)
            install_i18n(target / "fixtures")
            install_paths(target / "fixtures")
            result = subprocess.run(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                capture_output=True,
                text=True,
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                timeout=40,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)

    def test_two_real_contexts_keep_query_results_separate_and_share_catalogue(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder)
            stage(target)
            source = (target / "fixtures/NacreMenu.qml").read_text()
            source = source.replace("pragma Singleton", "").replace(
                'import "fixtures"', 'import "."'
            )
            source = source.replace(
                "id: root", "id: root\nproperty int openRefreshCount: 0", 1
            )
            source = source.replace(
                'function refreshOnOpen() {\n        refresh();\n        DesktopExtras.request("keys", {});\n        NacreControlTools.refresh();\n    }',
                "function refreshOnOpen() { root.openRefreshCount++; }",
            )
            (target / "fixtures/NacreMenuContext.qml").write_text(source)
            with (target / "fixtures/qmldir").open("a") as manifest:
                manifest.write("\nNacreMenuContext 1.0 NacreMenuContext.qml\n")
            (target / "tst_menu.qml").unlink()
            (target / "tst_context.qml").write_text("""import QtQuick
import QtTest
import "fixtures"
TestCase {
 name:"IndependentLauncherContexts"
 NacreMenuContext {id:first;shared:NacreMenu}
 NacreMenuContext {id:second;shared:NacreMenu}
 function test_open_refresh_is_deferred_and_coalesced(){
  const before = first.openRefreshCount;
  first.prepare("home","",true);first.prepare("home","",true);
  compare(first.openRefreshCount,before);
  tryCompare(first,"openRefreshCount",before+1,1000);
 }
 function test_queries_are_local_and_catalogue_is_shared(){
  first.fileQuery="one";first.fileRows=[{name:"First"}];
  second.fileQuery="two";second.fileRows=[{name:"Second"}];
  compare(first.fileQuery,"one");compare(second.fileQuery,"two");
  first.brainQuery="alpha";first.notes=[{title:"Alpha"}];
  second.brainQuery="beta";second.notes=[{title:"Beta"}];
  compare(first.brainRows("beta").length,0);
  compare(second.brainRows("alpha").length,0);
  compare(first.brainRows("alpha")[0].name,"Alpha");
  NacreMenu.catalog={rows:[{id:"shared",label:"Shared"}],software:[],error:""};
  compare(first.rows[0].id,"shared");compare(second.rows[0].id,"shared");
  NacreMenu.snapshot={packages:[{name:"installed"}]};
  verify(first.packageNames.has("installed"));verify(second.packageNames.has("installed"));
 }
}""")
            install_i18n(target / "fixtures")
            install_paths(target / "fixtures")
            result = subprocess.run(
                [str(runner), "-input", str(target)],
                capture_output=True,
                text=True,
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                timeout=20,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
