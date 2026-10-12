"""Optional Brain locale assets retain authentication and private-content boundaries."""

from pathlib import Path
import shutil
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[1]


class BrainWebTranslationTests(unittest.TestCase):
    def test_browser_pseudo_keeps_user_content_placeholders_and_markup(self):
        node = shutil.which("node")
        command = [node] if node else ["nacre-ai-tools", "node"]
        if not node and not shutil.which("nacre-ai-tools"):
            self.skipTest("Node.js unavailable")
        script = r"""
const fs=require("fs"),vm=require("vm"),assert=require("assert");
const caption={nodeType:3,nodeValue:" Open "},svg={nodeType:1},user={nodeType:3,nodeValue:"MY_NOTE_TITLE"};
const element={dataset:{i18nNode:"test.open"},childNodes:[svg,caption]},field={attributes:{"data-i18n-placeholder":"test.search",placeholder:"Search"},getAttribute(key){return this.attributes[key]},setAttribute(key,value){this.attributes[key]=value}};
const properties={};
const context={window:{NacreEnglish:{}},document:{documentElement:{dataset:{nacreLanguage:"pseudo"},style:{setProperty(key,value){properties[key]=value}}},querySelectorAll(selector){return selector==="[data-i18n-node]"?[element]:selector==="[data-i18n-placeholder]"?[field]:[]}},Node:{TEXT_NODE:3},Intl};
vm.createContext(context);vm.runInContext(fs.readFileSync(process.argv[2],"utf8"),context);vm.runInContext(fs.readFileSync(process.argv[1],"utf8"),context);
assert(caption.nodeValue.trim().startsWith("["));assert(!caption.nodeValue.includes("Open"));assert.strictEqual(element.childNodes[0],svg);assert.strictEqual(user.nodeValue,"MY_NOTE_TITLE");assert(field.attributes.placeholder.startsWith("["));
const tr=context.window.NacreI18n.tr;
const value=tr("missing.key","Open {name}",{name:"MY_NOTE_TITLE"});
assert(value.startsWith("[")&&value.endsWith("]"));assert(value.includes("MY_NOTE_TITLE"));assert(!value.includes("Open"));
const link=tr("missing.link","<a href='{url}'>Open</a>",{url:"https://example.test/ASSET"});
assert(link.includes("<a href='https://example.test/ASSET'>"));assert(link.includes("</a>"));
const app=fs.readFileSync(process.argv[3],"utf8");
vm.runInContext(app.slice(0,app.indexOf("async function load(")),context);
vm.runInContext('graph={theme:{surface:"112233",onSurface:"aabbcc"}};theme();',context);
assert.strictEqual(properties["--text"],"#aabbcc");
vm.runInContext(app.slice(app.indexOf("function confidenceLabel("),app.indexOf("function noteRow(")),context);
assert(context.confidenceLabel("verified").startsWith("["));assert.strictEqual(context.confidenceLabel("MY_UNKNOWN_CONFIDENCE"),"MY_UNKNOWN_CONFIDENCE");
"""
        subprocess.run(
            [
                *command,
                "-e",
                script,
                str(ROOT / "web/i18n.js"),
                str(ROOT / "web/translation-engine.js"),
                str(ROOT / "web/app.js"),
            ],
            check=True,
            capture_output=True,
            text=True,
        )

    def test_html_static_labels_have_ids_without_replacing_svg_children(self):
        source = (ROOT / "web/index.html").read_text()
        self.assertIn("data-i18n-node=", source)
        self.assertIn('src="/i18n.js" defer', source)
        self.assertIn('<symbol id="nacre"', source)
        self.assertIn('<svg><use href="#arrow"/></svg>', source)
        self.assertNotIn('data-i18n-node="brain.ui.text.my-notes"', source)


if __name__ == "__main__":
    unittest.main()
