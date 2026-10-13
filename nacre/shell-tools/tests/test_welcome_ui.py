from qml_source import install_i18n

"""Native Welcome navigation, controls, bounded layout and keyboard dismissal."""

import os
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from qml_source import install_foundation_interaction

ROOT = Path(__file__).parents[1]
SHELL = ROOT.parent / "shell"


class WelcomeUITests(unittest.TestCase):
    def test_native_routes_startup_and_responsive_footer(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            fixtures = target / "fixtures"
            shutil.copytree(ROOT / "tests/qml/fixtures", fixtures)
            install_foundation_interaction(fixtures, SHELL / "widgets")
            for name in (
                "NacreText",
                "NacreIcon",
                "NacreSurface",
                "ActionButton",
                "FastScroll",
                "NacreScrollBar",
                "NacreImage",
            ):
                text = (
                    (SHELL / "widgets" / (name + ".qml"))
                    .read_text()
                    .replace("import qs.services", 'import "."')
                )
                (fixtures / (name + ".qml")).write_text(text)
                if name in ("NacreIcon", "NacreScrollBar", "NacreImage"):
                    with (fixtures / "qmldir").open("a") as stream:
                        stream.write(f"\n{name} 1.0 {name}.qml\n")
            clip = (
                (SHELL / "widgets/NacreClip.qml")
                .read_text()
                .replace("import Quickshell.Widgets", "")
                .replace("ClippingRectangle {", "Rectangle {\nclip: true")
            )
            (fixtures / "NacreClip.qml").write_text(clip)
            with (fixtures / "qmldir").open("a") as stream:
                stream.write("\nNacreClip 1.0 NacreClip.qml\n")
            samples = []
            for index, color in enumerate(["#6686bd", "#ab516d", "#bb8e3d", "#519c87"]):
                image = target / ("sample" + str(index) + ".svg")
                image.write_text(
                    '<svg xmlns="http://www.w3.org/2000/svg" width="640" height="360"><rect width="640" height="360" fill="'
                    + color
                    + '"/><path d="M0 330 Q320 90 640 260 V360 H0Z" fill="#131922"/></svg>'
                )
                samples.append(
                    {
                        "artist": "Test artist",
                        "source": "https://example.test/source",
                        "licenseUrl": "https://creativecommons.org/licenses/by/4.0/",
                        "license": "CC-BY-4.0",
                        "path": str(image),
                        "preview": str(image),
                        "poster": str(image),
                        "name": [
                            "Blue horizon",
                            "Rose mist",
                            "Gold dusk",
                            "Green valley",
                        ][index],
                    }
                )
            (fixtures / "NacreWallpapers.qml").write_text(
                "pragma Singleton\nimport QtQuick\nQtObject {property string displayPath:"
                + json.dumps(samples[0]["path"])
                + ';property string selectedPath:"";function pickFiles(){}}'
            )
            with (fixtures / "qmldir").open("a") as stream:
                stream.write("\nsingleton NacreWallpapers 1.0 NacreWallpapers.qml\n")
            (target / "branding").mkdir()
            shutil.copyfile(
                SHELL / "branding/LogoData.js", target / "branding/LogoData.js"
            )
            (fixtures / "BrandLogo.qml").write_text(
                (SHELL / "widgets/BrandLogo.qml")
                .read_text()
                .replace("import qs.services", 'import "."')
            )
            palette = json.loads(
                (ROOT / "tests/orient-gallery/palettes.json").read_text()
            )["night-city"][
                "light-natural"
                if os.environ.get("NACRE_WELCOME_LIGHT")
                else "dark-natural"
            ]["colours"]
            roles = {"m3" + key: "#" + value for key, value in palette.items()}
            (fixtures / "NacreColours.qml").write_text(
                "pragma Singleton\nimport QtQuick\nQtObject{property bool light:"
                + ("true" if os.environ.get("NACRE_WELCOME_LIGHT") else "false")
                + ";property var palette:"
                + json.dumps(roles)
                + "}"
            )
            (fixtures / "NacreWelcomeApp.qml").write_text("""pragma Singleton
import QtQuick
QtObject { property bool visible:true;property string page:"welcome";property var steps:["welcome","colors","shortcuts","apps","ready"];readonly property int step:Math.max(0,steps.indexOf(page));property string error:"";property bool busy:false;
 property var data:({preferences:{showAtLogin:true},available:{ai:false,brain:false},shortcuts:SHORTCUTS});
 property string requestedMode:"";property string requestedPersonality:"";property var actions:[];property string optionalApp:"";property var demoEntries:SAMPLES;property bool demoBusy:false;property string demoError:"";property bool demoStarting:false;property bool demoChanged:false;property bool demoRestored:false;
 property var appearance:({palettePreset:"wallpaper",paletteMode:"CURRENT_MODE",palettePersonality:"natural"});
 function refresh(){}function setup(name){optionalApp=name}function selectDemo(path){demoChanged=true;actions=[...actions,"scene:"+path]}
 function restoreDemo(){actions=[...actions,"restore-demo"];demoChanged=false;demoRestored=true}
 function themeDemo(mode,name){if(mode)requestedMode=mode;if(name)requestedPersonality=name;actions=[...actions,"theme:"+mode+":"+name]}
 function route(name){actions=[...actions,name]} function link(name){actions=[...actions,"link:"+name]}
 function setStartup(value){data=Object.assign({},data,{preferences:{showAtLogin:value}})}
 function openCredit(url){actions=[...actions,"credit:"+url]}
 function jump(name){page=name}function advance(){if(step<4)page=steps[step+1];else finish()}function back(){if(page==="help")page="ready";else if(step>0)page=steps[step-1];else close()}function finish(){actions=[...actions,"finish"]}
 function close(){actions=[...actions,"close"]}
}""")
            (fixtures / "NacreCompositor.qml").write_text(
                "pragma Singleton\nimport QtQuick\nQtObject {property var capabilities:({})}"
            )
            with (fixtures / "qmldir").open("a") as stream:
                stream.write(
                    "\nBrandLogo 1.0 BrandLogo.qml\nsingleton NacreWelcomeApp 1.0 NacreWelcomeApp.qml\nsingleton NacreCompositor 1.0 NacreCompositor.qml\n"
                )
            stub = fixtures / "NacreWelcomeApp.qml"
            shortcuts = [
                {
                    "id": [
                        "launcher",
                        "terminal",
                        "put-away",
                        "workspace",
                        "lock",
                        "settings",
                        "files",
                        "wallpaper",
                        "ai",
                        "focus",
                        "move",
                        "capture",
                    ][i],
                    "key": "Super + S\nSuper + Shift + O\nSuper + Alt + K"
                    if i == 5
                    else "Super + " + str(i),
                    "titleId": "welcome.shortcut." + str(i) + ".title",
                    "title": "Current action " + str(i),
                    "detail": "A described binding from the current compositor.",
                }
                for i in range(12)
            ]
            stub.write_text(
                stub.read_text()
                .replace("SAMPLES", json.dumps(samples))
                .replace("SHORTCUTS", json.dumps(shortcuts))
                .replace(
                    "CURRENT_MODE",
                    "light" if os.environ.get("NACRE_WELCOME_LIGHT") else "dark",
                )
            )
            for name in ("NacreWelcomeView", "NacreWelcomeRow", "NacreWelcomeDemo"):
                text = (
                    (SHELL / "modules/welcome" / (name + ".qml"))
                    .read_text()
                    .replace("import qs.services", 'import "fixtures"')
                    .replace("import qs.widgets", 'import "fixtures"')
                    .replace('import "../colors"', 'import "fixtures"')
                )
                (target / (name + ".qml")).write_text(text)
            previewSource = (
                (SHELL / "modules/colors/NacreThemePreview.qml")
                .read_text()
                .replace("import qs.services", 'import "."')
                .replace("import qs.widgets", 'import "."')
            )
            (fixtures / "NacreThemePreview.qml").write_text(previewSource)
            (fixtures / "NacrePresentation.qml").write_text(
                "pragma Singleton\nimport QtQuick\nQtObject {property var active:({colours:"
                + json.dumps(palette)
                + "})}"
            )
            with (fixtures / "qmldir").open("a") as stream:
                stream.write(
                    "\nNacreThemePreview 1.0 NacreThemePreview.qml\nsingleton NacrePresentation 1.0 NacrePresentation.qml\n"
                )
            shutil.copyfile(
                SHELL / "modules/welcome/welcome-catalog.js",
                target / "welcome-catalog.js",
            )
            (target / "tst_welcome.qml").write_text("""import QtQuick
import QtTest
import "fixtures"
TestCase {id:test;name:"Welcome";width:1200;height:900;visible:true;when:windowShown
 Component{id:studio;NacreSurface{width:1000;height:680;color:NacreTokens.body;NacreWelcomeView{id:main;anchors.fill:parent;objectName:"welcomeView"}}}
 function test_view_state_keeps_expansion_but_obeys_current_tour_page(){
  const preview=createTemporaryObject(studio,test);const view=findChild(preview,"welcomeView");view.allShortcuts=true;const state=view.captureState();
  const other=createTemporaryObject(studio,test);const restored=findChild(other,"welcomeView");restored.restoreState(state);verify(restored.allShortcuts);
 }
 function test_routes_keyboard_and_responsive_footer(){
  const preview=createTemporaryObject(studio,test);const view=findChild(preview,"welcomeView");wait(200);
  const canvas=findChild(view,"welcomeCanvas"); const footer=findChild(view,"welcomeFooter");
  verify(canvas.y+canvas.height< footer.y);verify(footer.y+footer.height<=view.height);
  const prefix=Qt.application.arguments.find(a=>a.startsWith("welcome-preview="));
  if(prefix){let saved=false;verify(preview.grabToImage(result=>{saved=result.saveToFile(prefix.slice(16)+"-wide.png")}));tryVerify(()=>saved,3000)}
  compare(NacreWelcomeApp.actions.length,0);
  compare(NacreWelcomeApp.page,"welcome");
  const hero=findChild(view,"welcomeHeroLogo");const initialHero=hero.width;preview.height=1000;wait(30);verify(hero.width>initialHero);verify(hero.width<=320);preview.height=680;wait(30);
  findChild(view,"welcomeDone").clicked();compare(NacreWelcomeApp.page,"colors");wait(200);
  compare(findChild(view,"welcomeScenes").children.length>0,true);
  const mini=findChild(view,"welcomeDesktopPreview");const roles=findChild(view,"welcomePaletteRoles");compare(mini.width,mini.parent.width);verify(roles.y>=mini.y+mini.height);
  const styles=["natural","pigment","harmony","pop","mist","vivid","pearl","tide"].map(style=>findChild(view,"welcomePersonality_"+style));
  for(const button of styles){verify(button!==null);verify(button.x+button.width<=button.parent.width)}
  for(let i=1;i<styles.length;i++){verify(styles[i].y>styles[i-1].y || styles[i].x>=styles[i-1].x+styles[i-1].width)}
  const scene=findChild(view,"welcomeScene_1");verify(scene!==null);
  const hit=findChild(scene,"nacreInteractionFeedback").parent;mouseClick(hit,20,20);
  compare(NacreWelcomeApp.actions[0],"scene:"+NacreWelcomeApp.demoEntries[1].path);
  findChild(view,"welcomeMode_light").clicked();compare(NacreWelcomeApp.actions[1],"theme:light:");
  findChild(view,"welcomePersonality_pop").clicked();compare(NacreWelcomeApp.actions[2],"theme::pop");verify(findChild(view,"welcomePersonality_pop").selected);
  findChild(view,"welcomePersonality_pearl").clicked();compare(NacreWelcomeApp.actions[3],"theme::pearl");
  view.forceActiveFocus();NacreWelcomeApp.demoBusy=true;verify(!findChild(view,"welcomeMode_dark").enabled);NacreWelcomeApp.demoBusy=false;
  compare(findChild(view,"welcomeRestoreDemo"),null);
  compare(findChild(view,"welcomeColors"),null);
  findChild(view,"welcomeStep_shortcuts").children[0];
  NacreWelcomeApp.jump("shortcuts");wait(180);compare(NacreWelcomeApp.page,"shortcuts");
  const wheel=findChild(view,"welcomeWheel");compare(wheel.pixelMultiplier,3);compare(wheel.step,900);compare(wheel.kinetic,false);
  wheel.scrollBy(100000,true);wait(180);verify(Math.abs(canvas.contentY-wheel.maximum())<1);
  const bottom=canvas.contentY;for(let i=0;i<20;i++)wheel.scrollBy(200,true);wait(140);compare(canvas.contentY,bottom);
  wheel.scrollBy(-100000,true);wait(150);verify(canvas.contentY<1);
  const settingsRow=findChild(view,"welcomeShortcut_settings");verify(settingsRow.height>findChild(view,"welcomeShortcut_launcher").height);
  for(const child of settingsRow.children){verify(child.y+child.height<=settingsRow.height)}
  verify(canvas.contentHeight>canvas.height);
  NacreWelcomeApp.jump("apps");wait(180);
  findChild(view,"welcomeAppRow_ai").clicked();compare(NacreWelcomeApp.optionalApp,"ai");
  compare(findChild(view,"welcomeAppRow_ai").icon,"chat_bubble");
  compare(findChild(view,"welcomeAppRow_brain").icon,"neurology");
  preview.width=480;wait(80);
  for(const app of ["settings","colors","ai","brain"]){const row=findChild(view,"welcomeAppRow_"+app);verify(row.stacked);for(const child of row.children){if(child.visible && child.implicitHeight!==undefined)verify(child.y+child.height<=row.height+1)}}
  NacreWelcomeApp.jump("ready");wait(180);
  preview.width=672;preview.height=440;wait(30);verify(canvas.height>100);
  verify(footer.y+footer.height<=view.height);verify(findChild(view,"welcomeDone").x>=0);
  if(prefix){NacreWelcomeApp.page="colors";wait(100);let saved=false;verify(preview.grabToImage(result=>{saved=result.saveToFile(prefix.slice(16)+"-narrow.png")}));tryVerify(()=>saved,3000)}
  NacreWelcomeApp.page="ready";wait(200);
  compare(findChild(view,"welcomeStartupToggle"),null);
  view.forceActiveFocus();keyClick(Qt.Key_Escape);compare(NacreWelcomeApp.actions[NacreWelcomeApp.actions.length-1],"close");
 }
}""")
            test_file = target / "tst_welcome.qml"
            prefix = (
                "welcome-preview=" + os.environ["NACRE_WELCOME_PREVIEW"]
                if os.environ.get("NACRE_WELCOME_PREVIEW")
                else ""
            )
            test_file.write_text(
                test_file.read_text().replace(
                    'const prefix=Qt.application.arguments.find(a=>a.startsWith("welcome-preview="));',
                    "const prefix=" + json.dumps(prefix) + ";",
                )
            )
            install_i18n(fixtures)
            result = subprocess.run(
                [
                    str(runner),
                    "-input",
                    str(target / "tst_welcome.qml"),
                ],
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                capture_output=True,
                text=True,
                timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
