from qml_source import install_i18n

"""Native Colors studio controls, sizing, previews and explicit Apply dispatch."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from qml_source import install_foundation_interaction

ROOT = Path(__file__).parents[1]
SHELL = ROOT.parent / "shell"


class ColorsUITests(unittest.TestCase):
    def test_native_view_routes_and_bounds(self):
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
                "NacreTextField",
                "NacreAdjustSlider",
                "NacreScrollBar",
                "NacreImage",
            ):
                s = (
                    (SHELL / "widgets" / f"{name}.qml")
                    .read_text()
                    .replace("import qs.services", 'import "."')
                )
                (fixtures / f"{name}.qml").write_text(s)
            for name in (
                "NacreColorsView",
                "NacreThemePreview",
                "NacreColorSample",
                "NacreAccentTarget",
            ):
                s = (
                    (SHELL / "modules/colors" / f"{name}.qml")
                    .read_text()
                    .replace("import qs.widgets", 'import "fixtures"')
                    .replace("import qs.services", 'import "fixtures"')
                    .replace("import Quickshell\n", "")
                )
                (target / f"{name}.qml").write_text(s)
            colors = json.loads(
                (ROOT / "tests/orient-gallery/palettes.json").read_text()
            )["night-city"]["dark-natural"]["colours"]
            body = """property bool visible:true;property var options:({mode:"dark",personality:"natural",backgroundFromWallpaper:false,overrides:{},brightness:0,saturation:1,hour:12,workspaceColors:false});property var preview:({id:"preview",name:"Night city",thumbnail:"",palette:{colours:COLORS,source:{candidates:[]},roleSources:{},accessibility:{textPasses:true,minimumTextContrast:4.52,warnings:[],text:[]}},comparisons:[]});property var library:[];property var favorites:[];property var history:[];property bool ready:true;property bool previewBusy:false;property bool actionBusy:false;property bool applied:false;property string error:"";property string status:"";property string exportDirectory:"";property var actions:[];function change(v){options=Object.assign({},options,v)}function choose(path){}function useFavorite(id){}function useHistory(item){}function setRole(role,value){let n=Object.assign({},options.overrides);if(value)n[role]=value;else delete n[role];change({overrides:n})}function action(name){if(ready && !previewBusy && !actionBusy)actions=[...actions,name]}function close(){actions=[...actions,"close"]}""".replace(
                "COLORS", json.dumps(colors)
            )
            (fixtures / "NacreColorsApp.qml").write_text(
                "pragma Singleton\nimport QtQuick\nQtObject{" + body + "}"
            )
            with (fixtures / "qmldir").open("a") as stream:
                stream.write(
                    "\nsingleton NacreColorsApp 1.0 NacreColorsApp.qml\nNacreIcon 1.0 NacreIcon.qml\nNacreAdjustSlider 1.0 NacreAdjustSlider.qml\nNacreScrollBar 1.0 NacreScrollBar.qml\nNacreImage 1.0 NacreImage.qml\n"
                )
            (fixtures / "BrandLogo.qml").write_text(
                "import QtQuick\nItem{property color primary;property color secondary;property color tertiary;property color highlight;property color background;property color foreground;property bool motionEnabled:false}"
            )
            (target / "tst_colors.qml").write_text("""import QtQuick
import QtTest
import "fixtures"
TestCase {id:test;name:"ColorsStudio";width:1300;height:1000;visible:true;when:windowShown
 Component{id:mini;NacreThemePreview{width:400;height:280;palette:NacreColorsApp.preview.palette.colours}}
 function test_mini_desktop_children_stay_inside(){const preview=createTemporaryObject(mini,test);for(const h of [200,260,280,350]){preview.height=h;wait(10);verify(findChild(preview,"previewTerminal").visible);verify(findChild(preview,"previewNotification").visible);for(const child of preview.children){if(child.visible && child.width>0 && child.height>0)verify(child.y+child.height<=h+.1)}}}
 Component{id:studio;NacreColorsView{width:1180;height:780}}
 function test_state_preserves_unsaved_view_choices_without_apply(){
  const view=createTemporaryObject(studio,test);wait(30);
  view.query="demo";view.page="accessibility";view.vision="deutan";view.fineTune=true;view.inspected={id:"inspection"};
  const state=view.captureState();const initial=NacreColorsApp.actions.length;
  const restored=createTemporaryObject(studio,test);restored.restoreState(state);wait(20);
  compare(restored.query,"demo");compare(restored.page,"accessibility");compare(restored.vision,"deutan");verify(restored.fineTune);compare(restored.inspected.id,"inspection");compare(NacreColorsApp.actions.length,initial);
 }
 function test_resize_and_preview_controls_are_read_only(){
  const view=createTemporaryObject(studio,test);wait(30);
  const initial=NacreColorsApp.actions.length;
  const snap=Qt.application.arguments.find(a=>a.startsWith("colors-preview="));
  if(snap){let done=false;verify(view.grabToImage(result=>done=result.saveToFile(snap.slice(15)+"-wide.png")));tryVerify(()=>done,2000)}
  compare(findChild(view,"sourcePersonality"),null);
  verify(findChild(view,"pigmentPersonality")!==null);
  verify(findChild(view,"colorStyleControls")!==null);
  compare(findChild(view,"appearanceControls"),null);
  view.fineTune=true;wait(10);
  const background=findChild(view,"wallpaperBackgroundSwitch");
  const scroll=findChild(view,"colorsCanvas");scroll.contentY=Math.max(0,background.mapToItem(scroll.contentItem,0,0).y-scroll.height/2);wait(20);
  background.clicked();
  compare(NacreColorsApp.options.backgroundFromWallpaper,true);
  compare(NacreColorsApp.options.personality,"natural");compare(NacreColorsApp.actions.length,initial);
  findChild(view,"popPersonality").clicked();compare(NacreColorsApp.options.personality,"pop");compare(NacreColorsApp.actions.length,initial);
  for(const w of [740,960,1180]){view.width=w;wait(10);const canvas=findChild(view,"colorsCanvas");verify(canvas.width>400);verify(canvas.x+canvas.width<=view.width+.1);}
 }
 function test_explicit_actions_only_and_busy_guard(){
  const view=createTemporaryObject(studio,test);wait(30);const initial=NacreColorsApp.actions.length;
  NacreColorsApp.previewBusy=true;NacreColorsApp.action("apply");compare(NacreColorsApp.actions.length,initial);
  NacreColorsApp.previewBusy=false;findChild(view,"exportThemes").clicked();compare(NacreColorsApp.actions[initial],"export");NacreColorsApp.actions=NacreColorsApp.actions.slice(0,initial);NacreColorsApp.action("apply");compare(NacreColorsApp.actions[initial],"apply");
  view.page="compare";wait(10);view.page="accessibility";wait(10);compare(NacreColorsApp.actions.length,initial+1);
 }
}
""")
            if os.environ.get("NACRE_COLORS_PREVIEW"):
                test = target / "tst_colors.qml"
                test.write_text(
                    test.read_text().replace(
                        'Qt.application.arguments.find(a=>a.startsWith("colors-preview="))',
                        json.dumps(
                            "colors-preview=" + os.environ["NACRE_COLORS_PREVIEW"]
                        ),
                    )
                )
            install_i18n(fixtures)
            result = subprocess.run(
                [
                    str(runner),
                    "-input",
                    str(target),
                    "-o",
                    "-,txt",
                ],
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                capture_output=True,
                text=True,
                timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
