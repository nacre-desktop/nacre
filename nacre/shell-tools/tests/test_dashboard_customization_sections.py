"""Shared customization bodies keep both hosts on the same state contract."""

import os
from pathlib import Path
import shutil
import tempfile
import unittest

from qml_fixtures import (
    install_foundation_interaction,
    install_i18n,
    run_qml,
    stage_qml,
)

ROOT = Path(__file__).resolve().parents[1]
SHELL = ROOT.parent / "shell"


class DashboardCustomizationSectionsTests(unittest.TestCase):
    def test_shared_controls_section_isolation_and_keyboard_writes(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            fixtures = target / "fixtures"
            shutil.copytree(ROOT / "tests/qml/fixtures", fixtures)
            install_foundation_interaction(fixtures, SHELL / "widgets")
            for name in (
                "ActionButton",
                "NacreTextField",
                "NacreSurface",
                "NacreText",
                "NacreScrollBar",
                "FastScroll",
            ):
                stage_qml(
                    SHELL / "widgets" / (name + ".qml"), fixtures / (name + ".qml")
                )
            (fixtures / "NacreIcon.qml").write_text("import QtQuick\nText {}")
            (fixtures / "NacreDashboardState.qml").write_text("""pragma Singleton
import QtQuick
QtObject {
 property var preferences:({tabs:["today","media","weather","system"],lyrics:true,focusMinutes:25})
 readonly property var coreTabs:preferences.tabs
 property var enabledExtras:["brain"]
 property var extraOrder:["focus","notifications","clipboard","devices","network","ai-usage","screenshots","brain"]
 readonly property var tabs:[...coreTabs,...extraOrder.filter(id=>enabledExtras.includes(id))]
 property bool extrasReady:true
 property bool weatherPhotos:true
 property string message:""
 property var calls:[]
 function saveMany(values){calls=[...calls,["preferences",values]];preferences=Object.assign({},preferences,values)}
 function save(key,value){saveMany({[key]:value})}
 function saveExtras(){calls=[...calls,["extras",weatherPhotos]]}
 function toggleTab(id){const rows=coreTabs.includes(id)?coreTabs.filter(value=>value!==id):[...coreTabs,id];if(rows.length)save("tabs",rows)}
 function reorder(id,delta){calls=[...calls,["reorder",id,delta]];const rows=[...coreTabs],before=rows.indexOf(id);rows.splice(before,1);rows.splice(before+delta,0,id);preferences=Object.assign({},preferences,{tabs:rows})}
 function toggleExtra(id){calls=[...calls,["toggleExtra",id]];enabledExtras=enabledExtras.includes(id)?enabledExtras.filter(value=>value!==id):[...enabledExtras,id]}
 function reorderExtra(id,delta){calls=[...calls,["reorderExtra",id,delta]]}
}
""")
            with (fixtures / "qmldir").open("a") as manifest:
                manifest.write(
                    "\nNacreIcon 1.0 NacreIcon.qml\n"
                    "singleton NacreDashboardState 1.0 NacreDashboardState.qml\n"
                )
            stage_qml(
                SHELL / "services/NacreDashboardRegistry.qml",
                fixtures / "NacreDashboardRegistry.qml",
            )
            for name in (
                "NacreDashboardCustomize",
                "NacreDashboardCustomizeSection",
                "NacreDashboardToggleRow",
                "NacreDashboardExtrasOptions",
                "NacreDashboardNavigation",
            ):
                stage_qml(
                    SHELL / "modules/dashboard" / (name + ".qml"),
                    target / (name + ".qml"),
                )
            install_i18n(fixtures)
            (target / "tst_sections.qml").write_text("""import QtQuick
import QtTest
import "fixtures"
TestCase {
 id:test;name:"DashboardCustomizationSections";when:windowShown;visible:true;width:680;height:900
 Component{id:section;NacreDashboardCustomize{width:580;height:contentHeight}}
 Component{id:customizer;NacreDashboardCustomize{width:580;height:700}}
 function create(id,values={}){return createTemporaryObject(section,test,Object.assign({section:id},values))}
 function body(item){return findChild(item,"dashboardCustomizeSectionBody")}
 function control(item,name){const value=findChild(item,name);verify(!!value,name);return value}
 function test_shared_media_keyboard_and_photo_values(){
  const a=create("media",{showHeading:false}),b=create("media",{y:100,showHeading:false});
  compare(control(a,"dashboardCustomizeSectionHeading").visible,false);
  const first=control(a,"dashboardLyrics"),second=control(b,"dashboardLyrics");
  verify(first.checked);verify(second.checked);verify(first.height>0);compare(first.width,580);
  mouseClick(control(first,"dashboardPreferenceToggle"));compare(NacreDashboardState.preferences.lyrics,false);compare(second.checked,false);
  const toggle=control(second,"dashboardPreferenceToggle");toggle.forceActiveFocus();keyClick(Qt.Key_Space);compare(NacreDashboardState.preferences.lyrics,true);compare(first.checked,true);
  const photos=create("weather",{y:200,showHeading:false}),row=control(photos,"dashboardWeatherPhotos");
  mouseClick(control(row,"dashboardPreferenceToggle"));compare(NacreDashboardState.weatherPhotos,false);compare(NacreDashboardState.calls.slice(-1)[0][0],"extras");
  photos.visible=false;compare(body(photos).item,null);
  const unknown=create("unknown");compare(body(unknown).item,null);
 }
 function test_pages_and_extras_are_separate_without_duplicate_preview(){
  const pages=create("pages"),extras=create("extras",{y:550});
  verify(!!findChild(pages,"dashboardExtraToggle-media"));compare(findChild(pages,"dashboardExtraToggle-brain"),null);
  const preview=control(pages,"dashboardTabPreview");verify(preview.visible);
  compare(findChild(extras,"dashboardExtraToggle-media"),null);verify(!!findChild(extras,"dashboardExtraToggle-brain"));compare(control(extras,"dashboardTabPreview").visible,false);
  const up=control(pages,"dashboardExtraEarlier-weather");verify(up.enabled);mouseClick(up);
  const call=NacreDashboardState.calls.slice(-1)[0];compare(call[0],"reorder");compare(call[1],"weather");compare(call[2],-1);
 }
 function test_home_calendar_and_focus_use_existing_preference_actions(){
  const home=create("home-time");control(home,"dashboardHomeZone").text=" Europe/Paris ";control(home,"dashboardHomeCity").text=" Demo city ";mouseClick(control(home,"dashboardSaveHome"));
  compare(NacreDashboardState.preferences.homeZone,"Europe/Paris");compare(NacreDashboardState.preferences.homeCity,"Demo city");
  test.forceActiveFocus();home.visible=false;
  const calendar=create("calendar-focus");control(calendar,"dashboardCalendarFile").text=" /tmp/demo-calendar.ics ";mouseClick(control(calendar,"dashboardSaveCalendar"));compare(NacreDashboardState.preferences.calendarFile,"/tmp/demo-calendar.ics");
  mouseClick(control(calendar,"dashboardFocusMinutes-50"));compare(NacreDashboardState.preferences.focusMinutes,50);
 }
 function test_wrapper_can_embed_one_body_without_an_inner_heading(){
  const embedded=createTemporaryObject(customizer,test,{section:"media",showHeading:false});
  compare(embedded.sections.length,1);verify(!!findChild(embedded,"dashboardCustomize-media"));compare(findChild(embedded,"dashboardCustomize-pages"),null);
  const heading=control(embedded,"dashboardCustomizeSectionHeading");compare(heading.visible,false);verify(embedded.contentHeight>0);verify(embedded.contentHeight<90);compare(embedded.implicitHeight,embedded.contentHeight);
 }
}
""")
            result = run_qml(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                env=dict(os.environ, QT_QPA_PLATFORM="offscreen"),
                capture_output=True,
                text=True,
                timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
