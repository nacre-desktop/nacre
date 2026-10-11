"""Shared native dashboard card and service fixtures."""

from pathlib import Path
import shutil

from qml_source import install_foundation_interaction

ROOT = Path(__file__).resolve().parents[1]
SHELL = ROOT.parent / "shell"


def prepare_dashboard(target):
    fixtures = target / "fixtures"
    shutil.copytree(ROOT / "tests/qml/fixtures", fixtures)
    install_foundation_interaction(fixtures, SHELL / "widgets")
    for name in ("NacreSurface", "NacreText", "FastScroll"):
        shutil.copy2(SHELL / "widgets" / (name + ".qml"), fixtures / (name + ".qml"))
    (fixtures / "NacreIcon.qml").write_text("import QtQuick\nText {}")
    (fixtures / "NacreClip.qml").write_text("import QtQuick\nRectangle {clip:true}")
    (fixtures / "BrandLogo.qml").write_text("import QtQuick\nRectangle {color:'white'}")
    (fixtures / "FileView.qml").write_text("""import QtQuick
QtObject {
 property string path:""
 property bool printErrors:false
 signal loaded()
 function text(){return path==='/proc/uptime' ? '9001 0' : 'PRETTY_NAME="Fixture Linux"'}
 function reload(){if(path)loaded()}
 onPathChanged:if(path)Qt.callLater(reload)
}""")
    providers = {
        "NacreWeather": 'property real temperature:22;property string displayTemperature:"22°C";property string description:"Clear";property string icon:"sunny";property string location:"Fixture city";property bool stale:false',
        "NacreTime": "property date date:new Date(2026,9,9,12,34);function format(value){return Qt.formatDateTime(date,value)}",
        "NacrePlayers": "property var active:null",
        "NacreSystemUsage": 'property real cpuPerc:.15;property real memPerc:.63;property real storagePerc:.37;property string kernel:"7.2-fixture"',
    }
    with (fixtures / "qmldir").open("a") as manifest:
        for name in ("NacreIcon", "NacreClip", "BrandLogo", "FileView"):
            manifest.write(f"\n{name} 1.0 {name}.qml\n")
        for name, source in providers.items():
            (fixtures / (name + ".qml")).write_text(
                "pragma Singleton\nimport QtQuick\nQtObject {" + source + "}"
            )
            manifest.write(f"\nsingleton {name} 1.0 {name}.qml\n")
    (target / "media").mkdir(exist_ok=True)
    shutil.copy2(SHELL / "modules/dashboard/media/media.js", target / "media/media.js")
    (target / "NacreAlbumHalo.qml").write_text(
        (SHELL / "modules/dashboard/NacreAlbumHalo.qml")
        .read_text()
        .replace("import qs.widgets", 'import "fixtures"')
        .replace("import qs.services", "")
    )
    (target / "overview").mkdir()
    for path in (SHELL / "modules/dashboard/overview").iterdir():
        if path.suffix == ".js":
            shutil.copy2(path, target / "overview" / path.name)
            continue
        source = (
            path.read_text()
            .replace("import Quickshell.Io", "")
            .replace("import Quickshell", "")
        )
        source = source.replace("import qs.widgets", 'import "../fixtures"').replace(
            "import qs.services", ""
        )
        source = source.replace('Quickshell.env("USER")', '"fixture-user"')
        (target / "overview" / path.name).write_text(source)
