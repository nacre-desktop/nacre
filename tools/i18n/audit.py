"""Render owned UI routes in English/pseudo with synthetic native Qt providers."""

from pathlib import Path
import sys
import subprocess
import os
import json
import re
import importlib
import argparse
import tempfile
from unittest.mock import patch

root = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(root / "nacre/shell-tools/tests"))
sys.path.insert(0, str(root / "nacre/shell-tools"))
original = subprocess.run
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--output", type=Path, default=Path("/tmp/nacre-translation-audit"))
parser.add_argument("surfaces", nargs="*")
args = parser.parse_args()
output = args.output.resolve()
output.mkdir(parents=True, exist_ok=True)
english = json.loads((root / "nacre/shell/i18n/en.json").read_text())
palette = json.loads(
    (root / "nacre/shell-tools/tests/orient-gallery/palettes.json").read_text()
)["night-city"]["dark-natural"]["colours"]
suites = {
    "bar": (
        "test_bar_controls",
        "BarControlsTests",
        "test_reactive_bar_titles_status_power_profiles_and_calendar",
        """for(const component of [battery,calendar,title]){const panel=createTemporaryObject(component,this);wait(100);capture(panel,component===battery?"bar-battery":component===calendar?"bar-calendar":"bar-title")}""",
        "",
    ),
    "notices": (
        "test_notifications_ui",
        "NotificationUITests",
        "test_cards_and_popup_stack",
        """const panel=makeCard();panel.modelData.appIcon="nacre-notifications";panel.modelData.appName="Nacre Battery";panel.modelData.summary=NacreI18n.tr("battery.low.title","Battery Low");panel.modelData.body=NacreI18n.tr("battery.low.body","Remaining: {percent}%",{percent:15});panel.modelData.timeStr="12:34";wait(100);capture(panel,"notification-battery");panel.modelData.summary=NacreI18n.tr("focus.complete.title","Focus complete");panel.modelData.body=NacreI18n.tr("focus.complete.body","Take a moment to rest.");wait(100);capture(panel,"notification-focus");""",
        "",
    ),
    "settings": (
        "test_settings_ui",
        "SettingsUITests",
        "test_pages_and_shared_scrolling",
        """const panel=createTemporaryObject(auditScene,this);const view=findChild(panel,"auditView");for(const page of ["appearance","desktop","displays","sound","network","bluetooth","notifications","workflows","lock","time","ai","maintenance"]){view.open(page);wait(100);capture(panel,"settings-"+page)}panel.width=720;view.open("appearance");wait(100);capture(panel,"settings-narrow")""",
        """Component {id:auditScene;NacreSurface {width:1120;height:800;color:NacreTokens.body;NacreSettings {objectName:"auditView";anchors.fill:parent;active:true}}}""",
    ),
    "colors": (
        "test_colors_ui",
        "ColorsUITests",
        "test_native_view_routes_and_bounds",
        """const panel=createTemporaryObject(studio,this);for(const page of ["studio","compare","accessibility"]){panel.page=page;wait(100);capture(panel,"colors-"+page)}panel.page="studio";panel.fineTune=true;wait(100);capture(panel,"colors-fine");panel.width=740;wait(100);capture(panel,"colors-narrow");""",
        "",
    ),
    "welcome": (
        "test_welcome_ui",
        "WelcomeUITests",
        "test_native_routes_startup_and_responsive_footer",
        """const panel=createTemporaryObject(studio,this);for(const page of ["welcome","colors","shortcuts","apps","ready","help"]){NacreWelcomeApp.page=page;wait(100);capture(panel,"welcome-"+page)}panel.width=672;panel.height=440;NacreWelcomeApp.page="apps";wait(100);capture(panel,"welcome-narrow")""",
        "",
    ),
    "controls": (
        "test_quick_controls",
        "QuickControlsTests",
        "test_popups_and_settings_navigation_in_native_qt",
        """const panel=createTemporaryObject(center,this);for(const section of ["home","network","bluetooth","audio","battery","history"]){panel.selectedSection=section;wait(100);capture(panel,"controls-"+section)}""",
        "",
    ),
    "launcher": (
        "test_universal_menu_ui",
        "UniversalMenuUITests",
        "test_routes_search_keyboard_favorites_and_guards",
        """const panel=createTemporaryObject(scene,this);const ids=new Set(["root","apps","actions","hidden"]);for(const row of NacreMenu.catalog.rows||[])if(row.id)ids.add(row.id);for(const page of ids){panel.navigate(page);wait(60);capture(panel,"launcher-"+page)}for(const form of ["webapp","reminder"]){panel.run({operation:"form",value:form});wait(100);capture(panel,"launcher-form-"+form);panel.cancelForm()}for(const query of [">", "= 2+2", "# Demo", ": Demo", "? Demo"]){findChild(panel,"launcherSearch").text=query;wait(100);capture(panel,"launcher-query-"+query.charCodeAt(0))}findChild(panel,"launcherSearch").text="";panel.width=680;panel.navigate("apps");wait(100);capture(panel,"launcher-narrow")""",
        "",
    ),
    "chat": (
        "test_chatpane_ui",
        "ChatPaneUITests",
        "test_scrolling_and_sending_during_generation",
        """const panel=createTemporaryObject(pane,this);wait(100);capture(panel,"chat-empty");SidebarChat.busy=true;wait(50);capture(panel,"chat-busy");""",
        "",
    ),
    "summary": (
        "test_dashboard_summary",
        "DashboardSummaryTests",
        "test_actual_cards_alignment_narrow_layout_and_spectrum_states",
        """const panel=createTemporaryObject(summary,this);wait(100);capture(panel,"dashboard-summary");""",
        "",
    ),
    "media": (
        "test_media_performance_ui",
        "MediaUITests",
        "test_control_transactions_resource_data_and_layout",
        """const panel=createTemporaryObject(page,this);wait(100);capture(panel,"media-empty");const song=createTemporaryObject(player,this);NacrePlayers.active=song;NacrePlayers.list=[song];wait(100);capture(panel,"media-active");""",
        "",
    ),
    "wallpaper": (
        "test_wallpaper_picker_ui",
        "WallpaperPickerUITests",
        "test_filter_search_layouts_and_hex_hit_testing",
        """const panel=createTemporaryObject(picker,this);for(const layout of ["carousel","grid","hex"]){NacreWallpapers.preference({layout:layout});wait(100);capture(panel,"wallpaper-"+layout)}""",
        "",
    ),
    "login": (
        "test_login_ui",
        "LoginUITests",
        "test_native_submission_guards_and_failure_recovery",
        """theme.configuration=Object.assign({},theme.configuration,{language:language});wait(100);capture(theme,"login");proxy.loginFailed();wait(100);capture(theme,"login-error");""",
        "",
    ),
}
common = r"""
 function scan(item,rows){
  if(!item.visible||item.opacity===0)return;
  if(item.contentWidth!==undefined&&typeof item.text==="string"&&item.text.trim()&&item.font?.family!=="Material Symbols Rounded"){
   rows.push({text:item.text,object:item.objectName,width:item.width,height:item.height,contentWidth:item.contentWidth,contentHeight:item.contentHeight,truncated:item.truncated||false,elide:item.elide,wrapMode:item.wrapMode});
  }
  for(const child of item.children||[])scan(child,rows);
 }
 function capture(panel,label){
  const rows=[];scan(panel,rows);console.log("AUDIT "+language+" "+label+" "+JSON.stringify(rows));
  let saved=false;verify(panel.grabToImage(result=>saved=result.saveToFile(OUTPUT+"/"+language+"-"+label+".png")));tryVerify(()=>saved,3000);
 }
 property string language:"en"
""".replace("OUTPUT", json.dumps(str(output)))


def runner(command, **kwargs):
    if not command or Path(command[0]).name != "qmltestrunner":
        return original(command, **kwargs)
    target = Path(command[command.index("-input") + 1])
    target = target.parent if target.is_file() else target
    fixtures = target / "fixtures"
    if fixtures.exists():
        i18n = fixtures / "NacreI18n.qml"
        i18n.write_text(
            i18n.read_text().replace(
                "property var messages: ({})",
                "property var messages: ("
                + json.dumps(english, ensure_ascii=False)
                + ")",
            )
        )
        wallpaper = fixtures / "NacreWallpapers.qml"
        if wallpaper.exists():
            wallpaper.write_text(
                wallpaper.read_text().replace(
                    'property string rotationStatus:"Next wallpaper at 12:30"',
                    'property string rotationStatus:NacreI18n.tr("wallpaper.rotation.next","Next wallpaper at {time}",{time:"12:30"})',
                )
            )
        (fixtures / "NacreText.qml").write_text(
            (root / "nacre/shell/widgets/NacreText.qml").read_text()
        )
        field = fixtures / "NacreTextField.qml"
        if field.exists():
            field.write_text(
                (root / "nacre/shell/widgets/NacreTextField.qml").read_text()
            )
        (fixtures / "ActionButton.qml").write_text(
            (root / "nacre/shell/widgets/ActionButton.qml")
            .read_text()
            .replace("import qs.services", 'import "."')
        )
        icon = fixtures / "NacreIcon.qml"
        if icon.exists():
            icon.write_text((root / "nacre/shell/widgets/NacreIcon.qml").read_text())
        colors = fixtures / "NacreColours.qml"
        colors.write_text(
            "pragma Singleton\nimport QtQuick\nQtObject{property bool light:false;property var palette:"
            + json.dumps({"m3" + k: "#" + v for k, v in palette.items()})
            + "}"
        )
    codefile = next(target.glob("tst_*.qml"))
    code = codefile.read_text()
    code = re.sub(r"\bfunction test_", "function disabled_", code)
    body = suites[current][3]
    extra = suites[current][4]
    setup = "" if current == "login" else "NacreI18n.language=language;"
    code = (
        code.rstrip()[:-1]
        + common
        + extra
        + '\nfunction test_audit(){for(const lang of ["en","pseudo"]){language=lang;'
        + setup
        + body
        + "}}\n}\n"
    )
    codefile.write_text(code)
    result = original(command, **kwargs)
    (output / (current + ".log")).write_text(result.stdout + result.stderr)
    if result.returncode:
        print(
            current
            + " FAILED\n"
            + "\n".join(
                line
                for line in (result.stdout + result.stderr).splitlines()
                if "FAIL" in line or "QWARN" in line
            )
        )
    return result


def audit_dashboard():
    from test_media_performance_ui import prepare_pages
    from qml_source import install_i18n

    with tempfile.TemporaryDirectory() as folder:
        target = Path(folder)
        prepare_pages(target)
        fixtures = target / "fixtures"
        for name in [
            "NacreWeatherPage",
            "NacreSystemPage",
            "NacreFocusPage",
            "NacreTodayPage",
            "NacreDashboardHero",
            "NacreWeatherSky",
            "NacreChamberGauge",
            "NacreDashboardSummary",
            "NacreCompactMetric",
        ]:
            source = (
                (root / "nacre/shell/modules/dashboard" / f"{name}.qml")
                .read_text()
                .replace("import qs.widgets", 'import "fixtures"')
                .replace("import qs.services", 'import "fixtures"')
                .replace("import Quickshell\n", "")
                .replace("Quickshell.", "Environment.")
            )
            (target / f"{name}.qml").write_text(source)
        for name in ["NacreSessionControls"]:
            (target / f"{name}.qml").write_text(
                (root / "nacre/shell/modules/session" / f"{name}.qml")
                .read_text()
                .replace("import qs.widgets", 'import "fixtures"')
                .replace("import qs.services", 'import "fixtures"')
            )
        definitions = {
            "NacreDashboardState": 'property var home:({events:[]});property var focusState:({status:"idle"});property bool running:false;property real progress:.3;property string timerText:"24:12";property string message:"";property var brain:({available:false,nodes:[],links:[]});property var preferences:({albumPalette:true});function save(k,v){}function start(value){}function end(value){}function request(value){}function openNote(value){}',
            "NacreDashboardSystem": 'property var disks:[];property var processes:[];property var network:[];property string message:"";function end(pid,start){}',
            "NacreWeather": 'property var reading:({hours:[],days:[],code:0});property real temperature:22;property string displayTemperature:"22°C";property string description:NacreI18n.option("weather-description","Clear");property string icon:"sunny";property string location:"Fixture city";property bool stale:false;property bool busy:false;property string detail:"";property string range:"";property string error:"";function degrees(value){return value===undefined?"—":value+"°C"}function reload(){}',
            "NacreWallpapers": 'property string poster:"";property string displayPreview:"";property string displayPath:"";property bool displayDynamic:false;property bool displayAnimated:false;property bool rotationReady:true;property bool themeBusy:false;property var preferences:({palettePersonality:"natural"});function previousWallpaper(){}function advanceRotation(value){}function toggleMotion(){}',
            "WallpaperPlayback": "property bool paused:false;property bool sleeping:false;property bool locked:false;property bool batteryPaused:false",
            "NacreColorsApp": "function open(){}",
            "NacrePanelState": "function openMode(value){}",
            "Updates": "property int count:8",
            "Environment": 'function env(key){return "fixture-user"}function execDetached(args){}',
            "AppLaunch": "function run(args){}",
        }
        for name, body in definitions.items():
            (fixtures / f"{name}.qml").write_text(
                "pragma Singleton\nimport QtQuick\nQtObject{" + body + "}"
            )
            manifest = fixtures / "qmldir"
            text = manifest.read_text()
            if not re.search(r"\b" + name + r" 1.0", text):
                manifest.write_text(
                    text + "\nsingleton " + name + " 1.0 " + name + ".qml\n"
                )
        install_i18n(fixtures)
        code = (
            """import QtQuick
 import QtTest
 import "fixtures"
 TestCase {id:test;name:"DashboardTranslationAudit";width:1200;height:1000;visible:true;when:windowShown
  Component{id:weather;NacreWeatherPage{width:940;height:800;shouldUpdate:true}}
  Component{id:system;NacreSystemPage{width:940;height:800;shouldUpdate:true}}
  Component{id:focus;NacreFocusPage{width:940;height:800;shouldUpdate:true}}
  Component{id:today;NacreTodayPage{width:940;height:800;shouldUpdate:true;visibilities:({})}}
  Component{id:session;NacreSessionControls{width:150;height:380;visibilities:({session:true})}}
 """
            + common
            + """
  function test_audit(){for(const lang of ["en","pseudo"]){language=lang;NacreI18n.language=lang;for(const pair of [[weather,"dashboard-weather"],[system,"dashboard-system"],[focus,"dashboard-focus"],[today,"dashboard-today"],[session,"session"]]){const panel=createTemporaryObject(pair[0],this);wait(100);capture(panel,pair[1]);if(pair[0]===focus){for(const status of ["running","paused"]){NacreDashboardState.focusState={status:status};NacreDashboardState.running=status==="running";wait(60);capture(panel,"dashboard-focus-"+status)}}}}}
 }"""
        )
        (target / "tst_pages.qml").write_text(code)

        (fixtures / "NacreText.qml").write_text(
            (root / "nacre/shell/widgets/NacreText.qml").read_text()
        )
        (fixtures / "NacreIcon.qml").write_text(
            (root / "nacre/shell/widgets/NacreIcon.qml").read_text()
        )
        (fixtures / "NacreColours.qml").write_text(
            "pragma Singleton\nimport QtQuick\nQtObject{property bool light:false;property var palette:"
            + json.dumps({"m3" + k: "#" + v for k, v in palette.items()})
            + "}"
        )
        result = original(
            ["/usr/lib/qt6/bin/qmltestrunner", "-input", str(target), "-o", "-,txt"],
            env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
            capture_output=True,
            text=True,
            timeout=30,
        )
        (output / "dashboard-pages.log").write_text(result.stdout + result.stderr)
        if result.returncode or "QWARN" in result.stdout + result.stderr:
            raise RuntimeError(
                "Dashboard audit failed; see " + str(output / "dashboard-pages.log")
            )


failures = []
logfiles = []
for current in args.surfaces or [*suites, "dashboard"]:
    try:
        if current == "dashboard":
            audit_dashboard()
            logfiles.append(output / "dashboard-pages.log")
            print("dashboard captured")
            continue
        module, cls, method, *_ = suites[current]
        mod = importlib.import_module(module)
        klass = getattr(mod, cls)
        with patch("subprocess.run", runner):
            klass(method).debug()
        logfiles.append(output / (current + ".log"))
        print(current + " captured")
    except (AssertionError, RuntimeError, OSError, subprocess.TimeoutExpired) as error:
        failures.append(current + ": " + str(error)[-500:])
report = []
for logfile in logfiles:
    for line in logfile.read_text().splitlines():
        match = re.search(r"AUDIT (en|pseudo) (\S+) (\[.*)", line)
        if match:
            report.append(
                dict(language=match[1], screen=match[2], rows=json.loads(match[3]))
            )
(output / "report.json").write_text(
    json.dumps(report, ensure_ascii=False, indent=2) + "\n"
)
# Literal labels must be localized; public sample names and device identities are external data.
known = {
    value.strip()
    for value in english.values()
    if isinstance(value, str) and "{" not in value
}
external = {
    "Nacre Battery",
    "Nacre Focus",
    "Speakers",
    "Headphones",
    "IBM Plex Sans",
    "Music",
    "Editor",
    "Google Chrome",
}
for state in report:
    if state["language"] == "pseudo":
        for row in state["rows"]:
            if row["text"] in known and row["text"] not in external:
                failures.append(state["screen"] + ": unlocalized " + row["text"])
            if row.get("truncated") and row["text"].startswith("["):
                failures.append(state["screen"] + ": truncated " + row["text"])
if not report:
    failures.append("No UI states were captured")
if failures:
    raise SystemExit("\n".join(failures))
print(
    "Native translation audit passed: "
    + str(sum(row["language"] == "pseudo" for row in report))
    + " states per language. Report/screenshots: "
    + str(output)
)
