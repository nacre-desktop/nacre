from qml_source import install_i18n

"""Real hero buttons call the current wallpaper interface and open picker/colors."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from dashboard_fixtures import prepare_dashboard

ROOT = Path(__file__).resolve().parents[1]
SHELL = ROOT.parent / "shell"


class DashboardWallpaperActionsTests(unittest.TestCase):
    def test_current_manual_next_contract_and_picker_buttons(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            prepare_dashboard(target)
            fixtures = target / "fixtures"
            definitions = {
                "NacreWallpapers": 'property string poster:"";property string displayPreview:"";property string displayPath:"";property bool displayDynamic:false;property bool displayAnimated:false;property bool rotationReady:true;property bool themeBusy:false;property int previousCalls:0;property int pauseCalls:0;function previousWallpaper(){previousCalls++;}function toggleMotion(){pauseCalls++;WallpaperPlayback.paused=!WallpaperPlayback.paused;}property var preferences:({palettePersonality:"natural"});property int nextCalls:0;function advanceRotation(manual){if(manual)nextCalls++;}',
                "WallpaperPlayback": "property bool batteryPaused:false;property bool paused:false;property bool sleeping:false;property bool locked:false",
                "NacreDashboardState": "property var home:({})",
                "NacreColorsApp": "property int opens:0;function open(){opens++;}",
                "NacrePanelState": 'property string mode:"";function openMode(value){mode=value;}',
            }
            for name, body in definitions.items():
                (fixtures / (name + ".qml")).write_text(
                    "pragma Singleton\nimport QtQuick\nQtObject {" + body + "}"
                )
                with (fixtures / "qmldir").open("a") as manifest:
                    manifest.write(f"\nsingleton {name} 1.0 {name}.qml\n")
            (fixtures / "ActionButton.qml").write_text(
                (SHELL / "widgets/ActionButton.qml")
                .read_text()
                .replace("import qs.services", 'import "."')
            )
            (fixtures / "NacreScrollBar.qml").write_text(
                (SHELL / "widgets/NacreScrollBar.qml").read_text()
            )
            with (fixtures / "qmldir").open("a") as manifest:
                manifest.write(
                    "\nActionButton 1.0 ActionButton.qml\nNacreScrollBar 1.0 NacreScrollBar.qml\n"
                )
            for name in ("NacreTodayPage", "NacreDashboardHero", "NacreDashboardVideo"):
                source = (
                    (SHELL / "modules/dashboard" / (name + ".qml"))
                    .read_text()
                    .replace("import qs.widgets", 'import "fixtures"')
                    .replace("import qs.services", "")
                )
                (target / (name + ".qml")).write_text(source)
            (target / "NacreDashboardSummary.qml").write_text(
                "import QtQuick\nItem {property bool active:false;implicitHeight:300;signal systemRequested}"
            )
            ffmpeg = shutil.which("ffmpeg")
            if ffmpeg:
                for name, color in (("first", "red"), ("second", "blue")):
                    subprocess.run(
                        [
                            ffmpeg,
                            "-v",
                            "error",
                            "-f",
                            "lavfi",
                            "-i",
                            f"color=c={color}:s=64x64:r=10",
                            "-t",
                            "3",
                            "-c:v",
                            "mpeg4",
                            str(target / (name + ".mp4")),
                        ],
                        check=True,
                        timeout=15,
                    )
            (target / "tst_hero.qml").write_text(
                """import QtQuick
import QtTest
import QtMultimedia
import "fixtures"
TestCase {
 name:"DashboardWallpaperActions";width:1000;height:650;visible:true;when:windowShown
 Component {id:page;NacreTodayPage {width:940;height:562;shouldUpdate:true;visibilities:({})}}
 function test_maximized_page_allocates_space_to_wallpaper_not_stretched_cards(){
  const view=createTemporaryObject(page,this);view.width=1900;view.height=1100;wait(30);
  const hero=findChild(view,"dashboardWallpaperHero"),summary=findChild(view,"dashboardSummary");
  verify(hero.height>=450);verify(hero.height<=640);verify(summary.height<=420);
  verify(hero.height+summary.height+18<=view.height);
 }
 function test_manual_next_and_controls(){
  const view=createTemporaryObject(page,this);wait(40);
  const next=findChild(view,"dashboardNextWallpaper"),colors=findChild(view,"dashboardColors"),picker=findChild(view,"dashboardWallpapers");
  mouseClick(next,15,15);compare(NacreWallpapers.nextCalls,1);
  const feedback=findChild(next,"nacreInteractionFeedback");
  const interaction=feedback.parent;
  NacreWallpapers.rotationReady=false;mouseClick(next,15,15);compare(NacreWallpapers.nextCalls,1);
  NacreWallpapers.rotationReady=true;mouseMove(this,990,600);wait(200);
  verify(!interaction.pressed);verify(!interaction.keyboardFocusVisible);compare(interaction.stateOpacity,0);
  interaction.forceActiveFocus(Qt.TabFocusReason);verify(interaction.keyboardFocusVisible);
  keyClick(Qt.Key_Return);compare(NacreWallpapers.nextCalls,2);
  mouseClick(colors,15,15);compare(NacreColorsApp.opens,1);
  mouseClick(picker,15,15);compare(NacrePanelState.mode,"wallpaper");
  verify(picker.width>next.width);verify(picker.mapToItem(view,0,0).y+picker.height<next.mapToItem(view,0,0).y);
  findChild(view,"dashboardSummary").systemRequested();compare(view.visibilities.dashboardTab,3);
 }
 function test_transport_keeps_positions_and_shows_static_indicator(){
  const view=createTemporaryObject(page,this);wait(40);
  const previous=findChild(view,"dashboardPreviousWallpaper"),next=findChild(view,"dashboardNextWallpaper"),pause=findChild(view,"dashboardPauseWallpaper"),pill=findChild(view,"dashboardWallpaperTransport");
  const point=next.mapToItem(view,0,0);compare(pill.width,findChild(view,"dashboardWallpapers").parent.width);
  NacreWallpapers.displayDynamic=false;verify(pause.visible);verify(pause.informational);compare(pause.icon,"image");compare(pause.label,"Static wallpaper");
  const pauseCount=NacreWallpapers.pauseCalls;mouseClick(pause,22,17);compare(NacreWallpapers.pauseCalls,pauseCount);
  const picker=findChild(view,"dashboardWallpapers"),colors=findChild(view,"dashboardColors");
  compare(picker.width,colors.width);
  const count=NacreWallpapers.previousCalls;mouseClick(previous,22,17);compare(NacreWallpapers.previousCalls,count+1);
  NacreWallpapers.displayPath="";view.shouldUpdate=false;NacreWallpapers.displayDynamic=true;wait(20);
  verify(pause.visible);verify(!pause.informational);compare(next.mapToItem(view,0,0).x,point.x);compare(pill.width,findChild(view,"dashboardWallpapers").parent.width);
  const feedback=findChild(pause,"nacreInteractionFeedback"),badge=findChild(pause,"wallpaperCenterBadge"),interaction=feedback.parent;
  compare(feedback.width,badge.width);compare(feedback.height,badge.height);compare(feedback.mapToItem(pause,0,0).x,badge.x);
  compare(feedback.width,previous.width);compare(feedback.width,next.width);
  mouseMove(pause,2,16);tryCompare(interaction,"hovered",true);
  mouseMove(pause,pause.width/2,16);tryCompare(interaction,"hovered",true);
  const calls=NacreWallpapers.pauseCalls;mouseClick(pause,2,16);compare(NacreWallpapers.pauseCalls,calls+1);
  WallpaperPlayback.paused=false;mouseClick(pause,22,17);verify(WallpaperPlayback.paused);compare(pause.icon,"play_arrow");
  mouseClick(pause,22,17);verify(!WallpaperPlayback.paused);compare(pause.icon,"pause");
  NacreWallpapers.displayDynamic=false;
 }
 function test_narrow_action_group_and_palette_do_not_overlap(){
  const view=createTemporaryObject(page,this);wait(40);
  const pill=findChild(view,"dashboardWallpaperTransport"),palette=findChild(view,"dashboardPaletteStrip");
  const widths=[580,360,240];
  for(const width of widths){
   view.width=width;wait(20);
   const p=pill.mapToItem(view,0,0);
   verify(p.x>=0);verify(p.x+pill.width<=width);
   if(palette.visible){
    const c=palette.mapToItem(view,0,0);
    verify(c.x+palette.width<=p.x,"Palette overlaps wallpaper controls at "+width);
   }
  }
 }
 function test_changed_video_starts_without_reopening(){
  if(!__HAS_FFMPEG__){skip('ffmpeg unavailable');return;}
  NacreWallpapers.displayDynamic=true;
  NacreWallpapers.displayPath=Qt.resolvedUrl("first.mp4").toString().replace("file://","");
  const view=createTemporaryObject(page,this);
  tryVerify(()=>!!findChild(view,"dashboardWallpaperVideo"),1000);
  const video=findChild(view,"dashboardWallpaperVideo");
  tryCompare(video,"playbackState",MediaPlayer.PlayingState,5000);
  tryVerify(()=>video.position>0,5000);
  NacreWallpapers.displayPath=Qt.resolvedUrl("second.mp4").toString().replace("file://","");
  tryCompare(video,"playbackState",MediaPlayer.PlayingState,5000);
  tryVerify(()=>video.position>0,5000);
  wait(100);tryVerify(()=>video.position>0,5000);
  compare(findChild(view,"dashboardWallpaperVideo"),video);
  view.shouldUpdate=false;wait(30);verify(!findChild(view,"dashboardWallpaperVideo"));
  NacreWallpapers.displayDynamic=false;
 }
}
""".replace(
                    "__HAS_FFMPEG__",
                    str(bool(ffmpeg)).lower(),
                )
            )
            install_i18n(fixtures)
            result = subprocess.run(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                capture_output=True,
                text=True,
                timeout=20,
                env={
                    **os.environ,
                    "QT_QPA_PLATFORM": "offscreen",
                    "QT_FFMPEG_DECODING_HW_DEVICE_TYPES": ",",
                },
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
