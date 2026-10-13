from qml_fixtures import run_qml
from qml_fixtures import install_i18n

"""Decoded preview lifetime, bounded sizes and encoded local-file paths."""

import os
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SHELL = ROOT.parent / "shell"


class ImageLifetimeTests(unittest.TestCase):
    def test_hidden_previews_release_pixels_without_losing_the_requested_image(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Native Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            (target / "NacreImage.qml").write_text(
                (SHELL / "widgets/NacreImage.qml").read_text()
            )
            install_i18n(target)
            (target / "percent % hash # image.svg").write_text(
                '<svg xmlns="http://www.w3.org/2000/svg" width="3840" '
                'height="2160"><rect width="3840" height="2160" '
                'fill="#b85d78"/></svg>'
            )
            (target / "tst_images.qml").write_text("""import QtQuick
import QtTest
TestCase {
    id: test
    name: "PreviewImageLifetime"
    width: 400
    height: 300
    visible: true
    when: windowShown
    readonly property string picture: decodeURIComponent(Qt.resolvedUrl("percent % hash # image.svg").toString().slice(7))
    Component { id: preview; NacreImage { width: 200; height: 100; releaseDelay: 120 } }
    function test_grace_reopen_and_pixel_release() {
        const image = createTemporaryObject(preview, test, {path: picture, requestedSize: Qt.size(640, 360)});
        tryCompare(image, "status", Image.Ready, 1000);
        compare(image.sourceSize, Qt.size(640, 360));
        verify(image.source.toString().includes("%23"));
        const source = image.source.toString();
        image.active = false;
        compare(image.source.toString(), source);
        image.active = true;
        tryCompare(image, "status", Image.Ready);
        compare(image.source.toString(), source);
        image.active = false;
        tryCompare(image, "status", Image.Null, 500);
        compare(image.source.toString(), "");
        compare(image.path, picture);
        image.active = true;
        tryCompare(image, "status", Image.Ready, 1000);
        compare(image.source.toString(), source);
        compare(image.error, "");
    }
    function test_initially_hidden_and_changed_while_closed() {
        const image = createTemporaryObject(preview, test, {active: false, path: picture});
        compare(image.status, Image.Null);
        compare(image.source.toString(), "");
        image.width = 120;
        image.height = 80;
        image.active = true;
        tryCompare(image, "status", Image.Ready, 1000);
        compare(image.sourceSize, Qt.size(120, 80));
        image.active = false;
        tryCompare(image, "status", Image.Null, 500);
        image.path = "";
        image.active = true;
        compare(image.source.toString(), "");
        image.path = picture;
        tryCompare(image, "status", Image.Ready, 1000);
    }
}
""")
            result = run_qml(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                env={
                    **os.environ,
                    "QT_QPA_PLATFORM": "offscreen",
                    "QT_SCALE_FACTOR": "1",
                },
                capture_output=True,
                text=True,
                timeout=15,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
