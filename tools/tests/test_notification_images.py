"""Run the actual QML URL filter without a desktop or network connection."""

from pathlib import Path
import shutil
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[2]


@unittest.skipUnless(shutil.which("node"), "Node.js unavailable")
class NotificationImageTests(unittest.TestCase):
    def test_only_local_image_sources_and_icon_names_are_accepted(self):
        script = r"""
const fs = require('fs'), vm = require('vm'), assert = require('assert');
const context = vm.createContext({});
vm.runInContext(fs.readFileSync(process.argv[1], 'utf8'), context);
for (const url of ['https://example.invalid/pixel', 'http://127.0.0.1/private',
    '//example.invalid/image', 'ftp://example.invalid/image',
    'file://example.invalid/share/image', 'data:image/svg+xml,<svg/>',
    '../relative.png', null, {}, 1]) {
    assert.strictEqual(context.localSource(url), '', String(url));
}
for (const url of ['file:///tmp/fixture.png', 'file://localhost/tmp/fixture.png',
    'image://quickshell/fixture', 'qrc:/fixture.png']) {
    assert.strictEqual(context.localSource(url), url);
}
assert.strictEqual(context.localSource('/tmp/two words.png'), 'file:///tmp/two words.png');
for (const name of ['org.example.App', 'application-icon', 'nacre-notifications'])
    assert.strictEqual(context.iconName(name), name);
for (const name of ['https://example.invalid/icon', '//example.invalid/icon',
    '../icon', 'file:///tmp/icon', '', null])
    assert.strictEqual(context.iconName(name), '');
"""
        subprocess.run(
            [
                "node",
                "-e",
                script,
                str(ROOT / "nacre/shell/utils/scripts/notification-images.js"),
            ],
            capture_output=True,
            text=True,
            timeout=5,
            check=True,
        )
        source = (
            ROOT / "nacre/shell/modules/notifications/NacreNotice.qml"
        ).read_text()
        self.assertIn("NotificationImages.localSource(root.modelData.image)", source)
        self.assertIn("NotificationImages.localSource(root.modelData.appIcon)", source)
