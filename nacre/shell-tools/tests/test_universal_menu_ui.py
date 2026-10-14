from qml_fixtures import stage_qml, run_qml
from qml_fixtures import install_i18n

"""Real universal menu and model, with only external owners replaced by fixtures."""

import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest

from qml_fixtures import install_foundation_interaction

TOOLS = Path(__file__).resolve().parents[1]
SHELL = TOOLS.parent / "shell"


def stage(target):
    fixtures = target / "fixtures"
    shutil.copytree(TOOLS / "tests/qml/fixtures", fixtures)
    install_foundation_interaction(fixtures, SHELL / "widgets")
    for name in (
        "NacreAppBrowser",
        "NacreMenuIcon",
        "NacreMenuForm",
        "NacreMenuButton",
        "NacreCalculatorPad",
    ):
        stage_qml(
            SHELL / "modules/launcher" / (name + ".qml"), target / (name + ".qml")
        )
    for name in ("menu-model.js", "app-browser.js", "calculator.js"):
        shutil.copy2(SHELL / "modules/launcher" / name, target / name)
    stage_qml(SHELL / "services/NacreMenu.qml", fixtures / "NacreMenu.qml")
    stage_qml(
        SHELL / "services/NacreMenuContext.qml", fixtures / "NacreMenuContext.qml"
    )
    menu_data = json.dumps((TOOLS / "launcher-menu.json").read_text())
    (fixtures / "FileView.qml").write_text(
        'import QtQuick\nQtObject {property string path: "";property bool preload:false;property bool printErrors:false;property bool watchChanges:false;signal loaded();signal fileChanged();function reload(){loaded()}function text(){return path.includes("/shell/tools/launcher-menu.json") ? '
        + menu_data
        + ' : ""}Component.onCompleted:Qt.callLater(reload)}'
    )
    with (fixtures / "qmldir").open("a") as manifest:
        manifest.write("\nFileView 1.0 FileView.qml\n")
    definitions = {
        "NacreApps": """property var all:[{id:"editor",name:"Editor",icon:"",comment:"Code"},{id:"google-chrome",name:"Google Chrome",icon:"",comment:"Web browser"},{id:"nacre-ai",name:"Nacre AI",icon:"",comment:"Assistant"},{id:"music",name:"Music",icon:"",comment:"Player"}];readonly property var list:all.filter(row=>!LauncherPreferences.hidden.includes(row.id));property string launched:"";function launch(entry){launched=entry.id;return true}""",
        "LauncherPreferences": """property var favorites:[];property var hidden:[];property var recent:[];property var usage:({});property string error:"";property bool ready:true;function update(action,id,enabled){const key=action==="favorite"?"favorites":action==="hide"?"hidden":"recent";let result=this[key].filter(value=>value!==id);if(enabled)result.push(id);this[key]=result;}""",
        "DesktopActions": """property var list:[{name:"Open terminal",action:"terminal",icon:"terminal",description:"Terminal"},{name:"Disable blur",action:"toggle-setting",value:"blur",icon:"blur_on",launcher:false}];property var calls:[];function execute(action,value){calls=[...calls,[action,value]];}""",
        "AppLaunch": """property var commands:[];function run(command){commands=[...commands,command];}""",
        "DesktopSettings": """property var data:({animations:true,blur:true,shadow:true,dnd:false,interfaceFont:""});property var changes:[];function set(key,value){data=Object.assign({},data,{[key]:value});changes=[...changes,[key,value]];}""",
        "NacreWallpapers": """property var list:[{path:"/fixture/blue.png",name:"Blue garden",dynamic:false,thumbnail:""}];property string current:"/fixture/blue.png";property var appearancePreferences:({palettePersonality:"natural",paletteMode:"dark",paletteBackgroundFromWallpaper:false});property var paletteOptions:[{name:"Blue",accent:"blue",colors:[]}];property string selectedAccent:"blue";property var preferences:[];function preference(value){preferences=[...preferences,value];appearancePreferences=Object.assign({},appearancePreferences,value);}function setWallpaper(value){current=value;}""",
        "NacreControlTools": """property bool busy:false;property var data:({nightLightSupported:true,nightLightEnabled:false,nightLightExternal:false});property int refreshes:0;function refresh(){refreshes++}function toggleNightLight(){data=Object.assign({},data,{nightLightEnabled:!data.nightLightEnabled});}""",
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
    colors = fixtures / "NacreColours.qml"
    colors.write_text(
        colors.read_text().replace(
            '"m3onPrimary": "black",',
            '"m3onPrimary": "black", "m3primaryContainer":"#334455", "m3onPrimaryContainer":"white",',
        )
    )
    with (fixtures / "qmldir").open("a") as manifest:
        for name, body in definitions.items():
            (fixtures / (name + ".qml")).write_text(
                "pragma Singleton\nimport QtQuick\nQtObject{" + body + "}\n"
            )
            if name not in (fixtures / "qmldir").read_text():
                manifest.write("singleton " + name + " 1.0 " + name + ".qml\n")
    stage_qml(SHELL / "widgets/ActionButton.qml", fixtures / "ActionButton.qml")
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
    def test_app_icon_fallback_never_renders_desktop_image_identifiers(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder)
            stage(target)
            (target / "tst_menu.qml").unlink()
            # Exercise the production symbol font component, too.
            stage_qml(
                SHELL / "widgets/NacreIcon.qml", target / "fixtures/NacreIcon.qml"
            )
            stage_qml(
                SHELL / "widgets/NacreText.qml", target / "fixtures/NacreText.qml"
            )
            (target / "icon.svg").write_text(
                '<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32"><rect width="32" height="32" fill="blue"/></svg>'
            )
            (target / "tst_icon.qml").write_text("""import QtQuick
import QtTest
TestCase {
 name:"LauncherIconFallback"
 width:100;height:100;visible:true;when:windowShown
 Component {id:icon;NacreMenuIcon {width:44;height:44;size:44;entry:({kind:"app",icon:"io.missioncenter.MissionCenter"})}}
 function test_pending_ready_and_missing_app_icon(){
  const item=createTemporaryObject(icon,this);
  const image=item.children.find(child=>child.source!==undefined);
  const symbol=item.children.find(child=>child.fill!==undefined);
  verify(symbol.visible);compare(symbol.text,"apps");
  compare(symbol.font.pointSize,22);
  image.source=Qt.resolvedUrl("icon.svg");
  // Inspect immediately, before the asynchronous image can replace fallback.
  compare(symbol.text,"apps");
  tryCompare(image,"status",Image.Ready);
  tryCompare(symbol,"visible",false);
  item.entry=({kind:"app",icon:"discord"});
  image.source="";
  tryCompare(symbol,"visible",true);compare(symbol.text,"apps");
  item.entry=({kind:"action",icon:"download"});
  compare(symbol.text,"download");
 }
}""")
            install_i18n(target / "fixtures")
            result = run_qml(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                capture_output=True,
                text=True,
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)

    def test_routes_search_keyboard_favorites_and_guards(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder)
            stage(target)
            install_i18n(target / "fixtures")
            result = run_qml(
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
            (target / "tst_menu.qml").unlink()
            (target / "tst_context.qml").write_text("""import QtQuick
import QtTest
import "fixtures"
TestCase {
 name:"IndependentLauncherContexts"
 NacreMenuContext {id:first;shared:NacreMenu}
 NacreMenuContext {id:second;shared:NacreMenu}
 function test_open_refresh_is_deferred_and_coalesced(){
  const before = NacreControlTools.refreshes;
  first.prepare("home","",true);first.prepare("home","",true);
  compare(NacreControlTools.refreshes,before);
  tryCompare(NacreControlTools,"refreshes",before+1);
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
            result = run_qml(
                [str(runner), "-input", str(target)],
                capture_output=True,
                text=True,
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                timeout=20,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
