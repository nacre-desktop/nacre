"""Optional Brain locale assets retain authentication and private-content boundaries."""

from pathlib import Path
import shutil
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[1]


class BrainWebTranslationTests(unittest.TestCase):
    def test_browser_pseudo_keeps_user_content_placeholders_and_markup(self):
        node = shutil.which("node")
        if not node:
            self.skipTest("Node.js unavailable")
        script = r"""
const fs=require("fs"),vm=require("vm"),assert=require("assert");
const context={window:{NacreEnglish:{}},document:{documentElement:{dataset:{nacreLanguage:"pseudo"}},querySelectorAll:()=>[]},Node:{TEXT_NODE:3},Intl};
vm.createContext(context);vm.runInContext(fs.readFileSync(process.argv[1],"utf8"),context);
const tr=context.window.NacreI18n.tr;
const value=tr("missing.key","Open {name}",{name:"MY_NOTE_TITLE"});
assert(value.startsWith("[")&&value.endsWith("]"));assert(value.includes("MY_NOTE_TITLE"));assert(!value.includes("Open"));
const link=tr("missing.link","<a href='{url}'>Open</a>",{url:"https://example.test/ASSET"});
assert(link.includes("<a href='https://example.test/ASSET'>"));assert(link.includes("</a>"));
"""
        subprocess.run(
            [node, "-e", script, str(ROOT / "web/i18n.js")],
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
