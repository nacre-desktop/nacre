"""Load production QML unchanged, with isolated dependency modules."""

import os
from pathlib import Path
import shutil
import re
import subprocess
import tempfile
import shlex

SHELL = Path(__file__).resolve().parents[2] / "shell"
STUBS = Path(__file__).with_name("qml") / "imports"
_NATIVE = None


def native_plugin():
    global _NATIVE
    if _NATIVE is None:
        _NATIVE = tempfile.TemporaryDirectory(prefix="nacre-qml-plugin-")
        root = Path(_NATIVE.name)
        shutil.copy2(STUBS.parent / "native/plugin.cpp", root)
        flags = shlex.split(
            subprocess.check_output(
                ["pkg-config", "--cflags", "--libs", "Qt6Qml"], text=True
            )
        )
        subprocess.run(
            [
                "/usr/lib/qt6/moc",
                *[f for f in flags if f.startswith(("-I", "-D"))],
                str(root / "plugin.cpp"),
                "-o",
                str(root / "plugin.moc"),
            ],
            check=True,
        )
        subprocess.run(
            [
                "g++",
                "-std=c++17",
                "-shared",
                "-fPIC",
                str(root / "plugin.cpp"),
                "-o",
                str(root / "libnacretestplugin.so"),
                *flags,
            ],
            check=True,
            capture_output=True,
        )
    return Path(_NATIVE.name) / "libnacretestplugin.so"


def stage_qml(source, destination):
    """Register an unchanged production file, retaining its relative imports."""
    source, destination = Path(source), Path(destination)
    if source.suffix != ".qml":
        shutil.copy2(source, destination)
        return
    root = destination.parent
    while root.name in ("fixtures", "settings", "connectivity", "overview", "media"):
        root = root.parent
    tree = root / "production"
    if not tree.exists():
        shutil.copytree(SHELL, tree)
        for folder in [tree, *[p for p in tree.rglob("*") if p.is_dir()]]:
            entries = []
            for path in folder.glob("*.qml"):
                singleton = path.read_text().startswith("pragma Singleton")
                entries.append(
                    f"{'singleton ' if singleton else ''}{path.stem} 1.0 {path.name}"
                )
            if entries:
                (folder / "qmldir").write_text("\n".join(entries) + "\n")
    actual = tree / source.relative_to(SHELL)
    manifest = destination.parent / "qmldir"
    lines = manifest.read_text().splitlines() if manifest.exists() else []
    name = destination.stem
    lines = [line for line in lines if name not in line.split()[:2]]
    singleton = source.read_text().startswith("pragma Singleton")
    lines.append(
        f"{'singleton ' if singleton else ''}{name} 1.0 {os.path.relpath(actual, destination.parent)}"
    )
    manifest.write_text("\n".join(lines) + "\n")
    if destination.exists():
        destination.unlink()
    staged = root / "staged.txt"
    with staged.open("a") as stream:
        stream.write(str(source.relative_to(SHELL)) + "\n")


def install_modules(directory):
    """Expose fixture providers through the same imports as the real shell."""
    directory = Path(directory)
    root = directory.parent if directory.name in ("fixtures", "services") else directory
    shutil.copy2(STUBS.parent / "fixture-control.js", root / "fixture-control.js")
    imports = root / "imports"
    imports.mkdir(exist_ok=True)
    shutil.copytree(STUBS, imports, dirs_exist_ok=True)
    tree = root / "production"
    staged = (
        set((root / "staged.txt").read_text().splitlines())
        if (root / "staged.txt").exists()
        else set()
    )
    manifest = directory / "qmldir"
    lines = manifest.read_text().splitlines() if manifest.exists() else []
    entries = {}
    for line in lines:
        fields = line.split()
        if len(fields) < 3 or fields[0] in ("module", "prefer", "import", "depends"):
            continue
        name = fields[1] if fields[0] == "singleton" else fields[0]
        path = Path(fields[-1])
        if not path.is_absolute():
            path = directory / path
        if path.exists():
            entries[name] = (fields[0] == "singleton", path.resolve())
    for path in root.rglob("*.qml"):
        if path.is_relative_to(tree) or path.is_relative_to(imports):
            continue
        if path.stem[0].isupper():
            entries[path.stem] = (
                path.read_text().startswith("pragma Singleton"),
                path.resolve(),
            )
    if "NacreHelpers" not in entries:
        entries["NacreHelpers"] = (True, STUBS.parent / "fixtures/NacreHelpers.qml")
    # Native providers can be overridden by a fixture, while their module stays intact.
    for folder in [imports / "Quickshell", *(imports / "Quickshell").rglob("*")]:
        if not folder.is_dir() or not (folder / "qmldir").exists():
            continue
        lines = (folder / "qmldir").read_text().splitlines()
        for i, line in enumerate(lines):
            fields = line.split()
            if len(fields) < 3:
                continue
            name = fields[1] if fields[0] == "singleton" else fields[0]
            if name in entries and name not in (
                "Process",
                "SplitParser",
                "StdioCollector",
                "IpcHandler",
            ):
                singleton, path = entries[name]
                lines[i] = (
                    f"{'singleton ' if singleton else ''}{name} 1.0 {os.path.relpath(path, folder)}"
                )
        (folder / "qmldir").write_text("\n".join(lines) + "\n")

    def export(folder, uri=None, originals=None):
        folder.mkdir(parents=True, exist_ok=True)
        lines = ["module " + uri] if uri else []
        types = {}
        if originals:
            for path in originals.glob("*.qml"):
                singleton = path.read_text().startswith("pragma Singleton")
                if (
                    not singleton
                    or originals.name in ("config", "utils")
                    or (
                        path.is_relative_to(tree)
                        and str(path.relative_to(tree)) in staged
                    )
                ):
                    types[path.stem] = (singleton, path)
        if originals:
            for name, value in entries.items():
                if (originals / (name + ".qml")).exists():
                    types[name] = value
        else:
            types.update(entries)
        for name, (singleton, path) in types.items():
            lines.append(
                f"{'singleton ' if singleton else ''}{name} 1.0 {os.path.relpath(path, folder)}"
            )
        (folder / "qmldir").write_text("\n".join(lines) + "\n")

    export(directory)
    if tree.exists():
        folders = [p for p in tree.rglob("*") if p.is_dir() and any(p.glob("*.qml"))]
        for folder in folders:
            export(folder, originals=folder)
        for folder in folders:
            relative = folder.relative_to(tree)
            export(imports / "qs" / relative, "qs." + ".".join(relative.parts), folder)
    else:
        for name in ("services", "widgets", "config", "utils"):
            folder = imports / "qs" / name
            export(folder, "qs." + name, SHELL / name)
    for name in ("services", "widgets", "config", "utils"):
        folder = imports / "qs" / name
        if not (folder / "qmldir").exists():
            export(folder, "qs." + name, tree / name if tree.exists() else SHELL / name)
    for uri in ("Quickshell.Services.Notifications", "Quickshell.Wayland"):
        folder = imports / uri.replace(".", "/")
        shutil.copy2(native_plugin(), folder)
        with (folder / "qmldir").open("a") as manifest:
            manifest.write("plugin nacretestplugin\n")
    return str(imports)


def run_qml(command, *, isolate=False, **kwargs):
    """Optionally run each singleton test with a fresh engine, as in a new shell."""
    target = Path(command[command.index("-input") + 1])
    if target.is_file():
        target = target.parent
    production = target / "production"
    if production.exists():
        for path in production.rglob("*"):
            if path.suffix in (".qml", ".js") and path.is_file():
                relative = path.relative_to(production)
                if path.read_bytes() != (SHELL / relative).read_bytes():
                    raise RuntimeError(
                        "Production source was changed in fixture: " + str(relative)
                    )
    fixtures = target / "fixtures"
    providers = fixtures if fixtures.exists() else target
    install_paths(providers)
    imports = install_modules(providers)
    kwargs["env"] = {**kwargs.get("env", os.environ), "QML_IMPORT_PATH": imports}
    if not isolate:
        return subprocess.run(command, **kwargs)
    names = []
    for path in target.glob("tst_*.qml"):
        source = path.read_text()
        case = re.search(r'\bname\s*:\s*["\']([^"\']+)["\']', source)
        if not case:
            raise RuntimeError("Missing literal test case name: " + path.name)
        for name in re.findall(r"\bfunction\s+(test_\w+)\s*\(", source):
            if not name.endswith("_data"):
                names.append(case[1] + "::" + name)
    if not names:
        raise RuntimeError("No QML test functions discovered")
    results = [subprocess.run([*command, name], **kwargs) for name in names]
    failed = [r for r in results if r.returncode]
    reports = failed or results
    return subprocess.CompletedProcess(
        command,
        max(r.returncode for r in results),
        "".join(r.stdout[:8000] for r in reports),
        "".join(r.stderr[:1500] for r in reports),
    )


def install_paths(directory):
    """Extend only fake path providers; the real path singleton stays unchanged."""
    directory = Path(directory)
    target = directory / "NacrePaths.qml"
    if not target.exists():
        stage_qml(SHELL / "utils/NacrePaths.qml", target)
        return
    source = target.read_text()
    # These files are dependency fakes created by the caller, never production.
    if "NacreTestPaths {" not in source:
        if "QtObject {" not in source:
            raise RuntimeError("Unexpected path fixture root")
        shutil.copy2(STUBS.parent / "fixtures/NacreTestPaths.qml", directory)
        target.write_text(source.replace("QtObject {", "NacreTestPaths {", 1))


def install_foundation_interaction(fixtures, widgets):
    fixtures, widgets = Path(fixtures), Path(widgets)
    manifest = fixtures / "qmldir"
    text = manifest.read_text()
    for name in ("NacreTokens", "NacreInteraction"):
        shutil.copy2(widgets / (name + ".qml"), fixtures)
        if name not in text:
            text += f"\n{'singleton ' if name == 'NacreTokens' else ''}{name} 1.0 {name}.qml\n"
    if not (fixtures / "DesktopSettings.qml").exists():
        (fixtures / "DesktopSettings.qml").write_text(
            "pragma Singleton\nimport QtQuick\nQtObject {property var data:({animations:true})}\n"
        )
        text += "\nsingleton DesktopSettings 1.0 DesktopSettings.qml\n"
    manifest.write_text(text)


def install_i18n(directory):
    """Stage the real message engine for native UI tests without host preferences."""
    from pathlib import Path
    import shutil

    source = Path(__file__).resolve().parents[2] / "shell/i18n/translation.js"
    import json

    options = json.loads(source.with_name("options.json").read_text())
    messages = json.loads(source.with_name("en.json").read_text())
    shutil.copy2(source, directory / "translation.js")
    (directory / "NacreI18n.qml").write_text(
        """pragma Singleton
import QtQuick
import "translation.js" as Translation
QtObject {
    property string language: "en"
    property bool developerMode: false
    property var locale: Qt.locale("en_US")
    property var messages: MESSAGES
    property var options: OPTIONS
    function option(domain, key) { const row = options[domain]?.[key]; return row ? tr(row.id, row.fallback) : key; }
    function modelLabel(row, key) { return row[key + "Id"] ? tr(row[key + "Id"], row[key]) : row[key]; }
    function applicationName(name) { const row=options["application-name"]?.[name]; return row?tr(row.id,row.fallback):name; }
    function applicationRow(row) { const copy=Object.assign({},row);if(row.brand&&/^nacre-/.test(row.id)){copy.name=applicationName(row.name);copy.label=copy.name}return copy; }
    function windowCaption(value, className) {
        if(className!=="org.quickshell"&&!String(className||"").startsWith("nacre-"))return value;
        const pearl=/^Nacre Pearl (Launcher|Dashboard|AI|Controls|Media) ([0-9]+)$/.exec(value);
        return pearl?tr("pearl.window.title","Nacre Pearl {panel} {number}",{panel:tr("pearl.kind."+pearl[1].toLowerCase(),pearl[1]),number:pearl[2]}):applicationName(value);
    }
    function tr(id, fallback, args = {}) { return Translation.translate(messages, language, id, fallback, args); }
    function localize(value) { return Translation.localize(value, tr); }
    function number(value, decimals = 0) { return Number(value).toLocaleString(locale, "f", decimals); }
    function displayValue(value) { return language === "pseudo" && /[A-Za-z]/.test(value) ? Translation.pseudo(value) : value; }
    function dayName(day, format = Locale.ShortFormat) { return displayValue(locale.dayName(day, format)); }
    function dateTime(value, format = Locale.ShortFormat) { return new Date(value).toLocaleString(locale, format); }
    function date(value, format = Locale.ShortFormat) { return displayValue(new Date(value).toLocaleDateString(locale, format)); }
    function time(value, format = Locale.ShortFormat) { return new Date(value).toLocaleTimeString(locale, format); }
}
""".replace("OPTIONS", json.dumps(options)).replace("MESSAGES", json.dumps(messages))
    )
    manifest = directory / "qmldir"
    value = manifest.read_text() if manifest.exists() else ""
    if "singleton NacreI18n " not in value:
        manifest.write_text(value + "\nsingleton NacreI18n 1.0 NacreI18n.qml\n")

    if directory.parent.name != "qs":
        install_paths(directory)
        install_modules(directory)
