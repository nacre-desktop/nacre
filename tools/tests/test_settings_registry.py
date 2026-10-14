"""Validate the portable Settings contract and execute its actual search/router."""

import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[2]
REGISTRY = ROOT / "nacre/shell/modules/settings/registry"


def catalog():
    manifest = json.loads((REGISTRY / "manifest.json").read_text())
    return [json.loads((REGISTRY / name).read_text()) for name in manifest["fragments"]]


class SettingsRegistryTests(unittest.TestCase):
    @unittest.skipUnless(
        importlib.util.find_spec("jsonschema"),
        "Install developer-only jsonschema==4.26.0",
    )
    def test_json_schema_and_invalid_fragment_rejection(self):
        import jsonschema

        schema = json.loads((REGISTRY / "fragment.schema.json").read_text())
        jsonschema.Draft202012Validator.check_schema(schema)
        validator = jsonschema.Draft202012Validator(schema)
        for fragment in catalog():
            validator.validate(fragment)
        for bad in [
            {"version": 2},
            {"version": 1, "logic": "eval()"},
            {"version": 1, "pages": [{"id": "x"}]},
            {"version": 1, "settings": [{"visibleWhen": "battery"}]},
        ]:
            with self.assertRaises(jsonschema.ValidationError):
                validator.validate(bad)

    def test_ids_ownership_defaults_i18n_and_control_budget(self):
        fragments = catalog()
        merged = {
            key: sum((row.get(key, []) for row in fragments), [])
            for key in ["groups", "pages", "sections", "settings", "actions"]
        }
        english = json.loads((ROOT / "nacre/shell/i18n/en.json").read_text())
        pages = {row["id"]: row for row in merged["pages"]}
        sections = {row["id"]: row for row in merged["sections"]}
        identities = []
        for key in ["pages", "sections", "settings", "actions"]:
            identities.extend(row["id"] for row in merged[key])
        self.assertEqual(len(identities), len(set(identities)))
        for page in pages.values():
            if "parent" in page:
                self.assertIn(page["parent"], pages)
            self.assertIn(page["group"], {row["id"] for row in merged["groups"]})
        for setting in merged["settings"]:
            self.assertEqual(setting["id"], setting["section"] + "." + setting["key"])
            self.assertIn(setting["page"], pages)
            self.assertEqual(sections[setting["section"]]["page"], setting["page"])
            self.assertIn(
                setting["store"]["id"],
                {
                    row["id"]
                    for row in json.loads(
                        (ROOT / "nacre/shell-tools/nacre-state-files.json").read_text()
                    )["files"]
                },
            )
            self.assertTrue(setting["store"]["key"])
            value, kind = setting["default"], setting["type"]
            types = {
                "boolean": bool,
                "integer": int,
                "number": (int, float),
                "string": str,
                "array": list,
            }
            if kind == "enum":
                self.assertIn(value, [row["value"] for row in setting["options"]])
                self.assertTrue(
                    all(type(row["value"]) is type(value) for row in setting["options"])
                )
            elif kind == "number":
                self.assertIsInstance(value, types[kind])
                self.assertNotIsInstance(value, bool)
            else:
                self.assertIs(type(value), types[kind])
            if "range" in setting:
                limits = setting["range"]
                self.assertLess(limits["min"], limits["max"])
                self.assertGreater(limits["step"], 0)
                self.assertLessEqual(limits["min"], value)
                self.assertLessEqual(value, limits["max"])
            if "dependsOn" in setting:
                self.assertIn(
                    setting["dependsOn"], {row["id"] for row in merged["settings"]}
                )
            if setting.get("backend") == "planned":
                self.assertIn(setting["unavailableId"], english)
        for page in pages:
            self.assertLessEqual(
                sum(
                    row["page"] == page and not row.get("editorOwned")
                    for row in merged["settings"]
                ),
                15,
            )

        def visit(value):
            if isinstance(value, dict):
                for key, item in value.items():
                    if key.endswith("Id") and key != "storeId":
                        self.assertIn(item, english)
                    else:
                        visit(item)
            elif isinstance(value, list):
                for item in value:
                    visit(item)

        for fragment in fragments:
            visit(fragment)
        defaults = json.loads(
            (ROOT / "nacre/shell-tools/desktop-defaults.json").read_text()
        )
        for setting in merged["settings"]:
            if (
                setting["store"]["id"] == "desktop"
                and setting.get("backend") != "planned"
            ):
                self.assertEqual(
                    setting["default"], defaults[setting["store"]["key"][0]]
                )
        for section in merged["sections"]:
            if section.get("reserved"):
                self.assertEqual(section["visibleWhen"], {"capability": "packages"})

    def test_actual_javascript_search_routes_gates_and_types(self):
        node = (
            [shutil.which("node")]
            if shutil.which("node")
            else ["nacre-ai-tools", "node"]
        )
        script = r"""
const fs = require('fs'), vm = require('vm'), assert = require('assert');
const base = process.argv[1];
const api = {};
vm.createContext(api);
vm.runInContext(fs.readFileSync(base + '/registry.js', 'utf8').replace('.pragma library', ''), api);
const manifest = JSON.parse(fs.readFileSync(base + '/manifest.json'));
const schema = api.merge(manifest.fragments.map(name => JSON.parse(fs.readFileSync(base + '/' + name))));
const english = JSON.parse(fs.readFileSync(process.argv[2]));
const text = id => english[id] || '';
const caps = {battery:true,ai:true,brain:true,bluetooth:true,wifi:true,packages:true,bottomPanel:true,batterySaver:true};
for (const setting of schema.settings) {
    assert(api.validValue(setting, setting.default), setting.id);
    for (const fragment of [setting.key, setting.id]) {
        const resolved = api.resolve(schema, setting.page + '#' + fragment, caps, true);
        assert(resolved && resolved.setting === setting.id, setting.id);
    }
    assert(api.search(schema, text(setting.labelId), caps, text, true).some(row => row.id === setting.id), setting.id);
    assert(!api.resolve(schema, 'colors#' + setting.id, caps) || setting.page === 'colors');
}
for (const [alias, target] of Object.entries(schema.aliases)) {
    assert(api.resolve(schema, alias, caps)?.page === target.split("#")[0], alias);
}
for (const link of ['', 'missing', '../desktop', 'colors#absent', 'colors?command=bad', 'panels/dashboard#clickEdgeMenus']) assert(api.resolve(schema, link, caps) === null, link);
assert(api.resolve(schema, 'power', {}) === null);
assert(api.resolve(schema, 'panels/ai-sidebar', {}) === null);
assert(api.resolve(schema, 'ai', {})?.page === 'addons');
assert(!api.search(schema, 'Battery saver', {}, text).some(row => row.page === 'power'));
assert(!api.search(schema, 'Nacre AI', {}, text).some(row => row.page === 'panels/ai-sidebar' || row.id === 'launcher.sources.ai'));
assert(api.search(schema, 'orient', caps, text).some(row => row.page === 'colors'));
assert(api.search(schema, 'pearls', caps, text).some(row => row.page === 'panels'));
assert(api.search(schema, 'bead', caps, text).some(row => row.page === 'panels'));

const boolean = schema.settings.find(row => row.type === 'boolean');
assert(!api.validValue(boolean, 'true'));
const number = schema.settings.find(row => row.type === 'integer');
assert(!api.validValue(number, 99999));assert(!api.validValue(number, NaN));assert(!api.validValue(number, true));
const original = {nested:{value:1},unknown:'kept'};
const updated = api.writePath(original,['nested','value'],2);
assert(updated.nested.value === 2 && updated.unknown === 'kept' && original.nested.value === 1);
assert(api.readPath(updated,['absent'],4) === 4);
assert.throws(() => api.writePath({},['__proto__','polluted'],true));
assert(api.readPath({},['constructor'],4) === 4);
assert(!api.allowed({capability:'future'},{}));
assert(api.allowed({all:[{capability:'ai'},{not:{capability:'future'}}]},caps));
assert(!api.allowed({javascript:'true'},caps));
"""
        result = subprocess.run(
            [
                *node,
                "-e",
                script,
                str(REGISTRY),
                str(ROOT / "nacre/shell/i18n/en.json"),
            ],
            capture_output=True,
            text=True,
            timeout=20,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
