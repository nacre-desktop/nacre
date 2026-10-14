"""Process CPU rates warm up while visible and stop sampling when hidden."""

import os
from pathlib import Path
import tempfile
import unittest

from qml_fixtures import install_i18n, run_qml, stage_qml

SHELL = Path(__file__).resolve().parents[2] / "shell"


class DashboardProcessCpuTests(unittest.TestCase):
    def test_production_sampler_warms_up_rates_and_handles_pid_reuse(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            fixtures = target / "fixtures"
            fixtures.mkdir()
            definitions = {
                "NacrePanelState": "property bool hidden:false;property var providerViews:[]",
                "NacreNetwork": 'property bool active:false;property string wifiInterface:""',
            }
            manifest = ""
            for name, fields in definitions.items():
                (fixtures / (name + ".qml")).write_text(
                    "pragma Singleton\nimport QtQuick\nimport Quickshell\nSingleton {"
                    + fields
                    + "}"
                )
                manifest += f"singleton {name} 1.0 {name}.qml\n"
            (fixtures / "qmldir").write_text(manifest)
            install_i18n(fixtures)
            stage_qml(
                SHELL / "services/NacreDashboardSystem.qml",
                fixtures / "NacreDashboardSystem.qml",
            )
            (target / "tst_cpu.qml").write_text("""import QtQuick
import QtTest
import "fixtures"
TestCase {
 name:"DashboardProcessCpu";when:windowShown
 property var state:NacreDashboardSystem
 function reader(){for(const child of state.children)if(child.stdout)return child;fail("snapshot reader missing")}
 function sample(value){const worker=reader();worker.stdout.text=JSON.stringify({processes:[{pid:10,start:"100",name:"Demo worker",memory:1048576,cpuSeconds:value}],disks:[]});worker.finish();worker.stdout.streamFinished();}
 function test_visible_sampler_gets_second_read_quickly_and_stops_hidden(){
  const worker=reader(),timer=findChild(state,"dashboardProcessSampler");
  NacrePanelState.providerViews=[{dashboard:true,dashboardTab:3}];verify(state.shown);compare(timer.interval,750);verify(timer.running);
  sample(1);compare(Object.keys(state.cpuByPid).length,0);compare(state.processCpuReady,false);
  const starts=worker.starts;tryVerify(()=>worker.starts>starts,1200);
  sample(1.2);verify(Number.isFinite(state.cpuByPid["10:100"]));verify(state.cpuByPid["10:100"]>0);compare(timer.interval,2000);
  NacrePanelState.providerViews=[];compare(timer.running,false);worker.finish();const hiddenStarts=worker.starts;wait(800);compare(worker.starts,hiddenStarts);
  NacrePanelState.providerViews=[{dashboard:true,dashboardTab:3}];compare(state.processCpuReady,false);compare(timer.interval,750);compare(Object.keys(state.cpuByPid).length,0);
  NacrePanelState.providerViews=[];worker.finish();
 }
 function test_exact_rates_zero_usage_new_identity_and_invalid_counters(){
  state.previousProcesses={};state.processSampleTime=0;
  state.sampleProcesses([{pid:10,start:"100",cpuSeconds:5}],100);
  state.sampleProcesses([{pid:10,start:"100",cpuSeconds:5.25}],101);compare(state.cpuByPid["10:100"],25);
  state.sampleProcesses([{pid:10,start:"100",cpuSeconds:5.25}],102);compare(state.cpuByPid["10:100"],0);
  state.sampleProcesses([{pid:10,start:"200",cpuSeconds:1}],103);compare(state.cpuByPid["10:200"],undefined);compare(state.cpuByPid["10:100"],undefined);
  state.sampleProcesses([{pid:10,start:"200",cpuSeconds:1.5}],104);compare(state.cpuByPid["10:200"],50);
  state.sampleProcesses([{pid:10,start:"200",cpuSeconds:undefined}],105);compare(state.cpuByPid["10:200"],undefined);
 }
}
""")
            result = run_qml(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                capture_output=True,
                text=True,
                timeout=20,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
