from qml_source import install_i18n

"""Native summary layout keeps calendar, gauges and real-spectrum baselines aligned."""

import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from dashboard_fixtures import prepare_dashboard

ROOT = Path(__file__).resolve().parents[1]
SHELL = ROOT.parent / "shell"


class DashboardSummaryTests(unittest.TestCase):
    def test_actual_cards_alignment_narrow_layout_and_spectrum_states(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            prepare_dashboard(target)
            fixtures = target / "fixtures"
            definitions = {
                "Updates": "property int count:8",
                "NacreMediaDetails": "property var spectrum:[]",
                "Environment": 'function env(name){return "fixture-user";}property var launched:[];function execDetached(args){launched=args;}',
            }
            for name, body in definitions.items():
                (fixtures / (name + ".qml")).write_text(
                    "pragma Singleton\nimport QtQuick\nQtObject {" + body + "}"
                )
                with (fixtures / "qmldir").open("a") as manifest:
                    manifest.write(f"\nsingleton {name} 1.0 {name}.qml\n")
            for name in (
                "NacreDashboardSummary",
                "NacreCompactMetric",
                "NacreAlbumHalo",
            ):
                source = (
                    (SHELL / "modules/dashboard" / (name + ".qml"))
                    .read_text()
                    .replace("import qs.widgets", 'import "fixtures"')
                    .replace("import qs.services", "")
                    .replace("import Quickshell", "")
                    .replace("Quickshell.", "Environment.")
                )
                (target / (name + ".qml")).write_text(source)
            (target / "tst_summary.qml").write_text("""import QtQuick
import QtTest
import "fixtures"
TestCase {
 name:"DashboardSummary";width:1100;height:1200;visible:true;when:windowShown
 Component {id:summary;NacreDashboardSummary {width:940;height:300;active:true}}
 SignalSpy {id:systemSpy;signalName:"systemRequested"}
 Component {id:meter;NacreAlbumHalo {width:176;height:176;coverSize:120;active:true}}
 function init(){DesktopSettings.data={animations:false};NacreMediaDetails.spectrum=[];NacrePlayers.active=null;}
 function test_bottom_alignment_and_compact_controls_fit(){
  const scene=createTemporaryObject(summary,this);wait(40);
  const calendar=findChild(scene,"dashboardCalendar"),host=findChild(scene,"dashboardHost"),metrics=findChild(scene,"dashboardMetrics"),media=findChild(scene,"dashboardMedia");
  compare(calendar.y+calendar.height,metrics.y+metrics.height);compare(calendar.y+calendar.height,media.y+media.height);
  verify(host.y+host.height<metrics.y);verify(!findChild(scene,"dashboardSpectrum"));
  verify(host.x>=calendar.x+calendar.width);verify(media.x>=host.x+host.width);
  compare(findChild(host,"hostLine4").text,"8 updates");
  compare(findChild(host,"hostLine5").text,"7.2-fixture");
  const background=findChild(scene,"dashboardSystemSurface");
  compare(background.x,host.x);compare(background.y,host.y);
  compare(background.y+background.height,metrics.y+metrics.height);compare(host.color.a,0);
  verify(!findChild(media,"mediaDuration"));
  systemSpy.target=scene;systemSpy.clear();
  mouseClick(findChild(host,"hostAction_system"),10,10);compare(systemSpy.count,1);
  mouseClick(findChild(host,"hostAction_updates"),10,10);compare(Environment.launched[1],"updates");
  const play=findChild(media,"mediaToggle");const point=play.mapToItem(media,0,0);verify(point.y+play.height<=media.height);
  scene.width=580;scene.height=scene.implicitHeight;wait(30);
  verify(host.y>=calendar.y+calendar.height);verify(metrics.y>=host.y+host.height);
  verify(media.y>=metrics.y+metrics.height);compare(media.y+media.height,scene.height);
 }
 function test_spectrum_samples_and_inactive_motion_are_honest(){
  const halo=createTemporaryObject(meter,this);wait(30);
  halo.samples=Array(32).fill(0);verify(!halo.audible);verify(!halo.visible);
  halo.samples=Array(32).fill(.5);verify(halo.audible);verify(halo.visible);
  halo.active=false;verify(!halo.audible);verify(!halo.visible);
  halo.active=true;halo.samples=[NaN,Infinity,-1];verify(!halo.audible);
 }

}
""")
            install_i18n(fixtures)
            result = subprocess.run(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                capture_output=True,
                text=True,
                timeout=20,
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
