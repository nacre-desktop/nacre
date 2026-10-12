"""Approved artwork stays one source across small, colored and raster outputs."""

import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from xml.etree import ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("nacre_brand", ROOT / "branding.py")
brand = importlib.util.module_from_spec(spec)
spec.loader.exec_module(brand)


class NacreBrandingTests(unittest.TestCase):
    def test_batched_vector_publication_matches_individual_role_rendering(self):
        colors = json.loads((ROOT / "reference-style.json").read_text())["colours"]
        patterns, roles = brand.templates(), brand.role_colors(colors)
        for variant, template in patterns.items():
            self.assertEqual(
                brand.paint_svg(template, roles), brand.svg(False, colors, variant)
            )

    def test_master_is_the_exact_approved_archive_artwork(self):
        self.assertEqual(
            hashlib.sha256(brand.MASTER.read_bytes()).hexdigest(),
            "679c5ef56949cb64dc13e266831cb588b0b6b9e23fffcca65dc3d4aa9d83b42f",
        )
        master = ET.fromstring(brand.MASTER.read_text())
        symbolic = ET.fromstring(brand.templates()["symbolic"])
        chambers = [
            n
            for n in master
            if n.tag.endswith("path") and n.get("fill", "").startswith("@")
        ]
        self.assertEqual(len(chambers), 8)
        self.assertEqual(symbolic[0].get("d"), " ".join(n.get("d") for n in chambers))
        self.assertEqual(symbolic[1].get("r"), "3.14")
        self.assertFalse(
            any(n.tag.endswith(("script", "animate", "image")) for n in master.iter())
        )

    def test_compact_roles_clear_graphics_contrast_in_light_dark_and_neutral(self):
        for bg, fg in [
            ("101014", "f4f1ef"),
            ("fff3df", "161310"),
            ("777777", "ffffff"),
        ]:
            colors = dict(
                surface=bg,
                frame=bg,
                onSurface=fg,
                primary="818181",
                secondary="fefeee",
                tertiary="222233",
                primaryFixed="ffffff",
            )
            roles = brand.role_colors(colors)
            for role in ("PRIMARY", "SECONDARY", "TERTIARY"):
                a, b = sorted((brand.luminance(roles[role]), brand.luminance(bg)))
                self.assertGreaterEqual((b + 0.05) / (a + 0.05), 3.4)
            for role in ("AI_PRIMARY", "AI_SECONDARY"):
                a, b = sorted((brand.luminance(roles[role]), brand.luminance(bg)))
                self.assertGreaterEqual((b + 0.05) / (a + 0.05), 4.5)
            self.assertNotIn("@", brand.svg(False, colors, "compact"))

    @unittest.skipUnless(
        shutil.which("rsvg-convert"), "Native SVG rasterizer unavailable"
    )
    def test_small_and_large_assets_keep_alpha_and_viewbox(self):
        colors = dict(
            primary="ce5483",
            secondary="7ea4da",
            tertiary="69babc",
            primaryFixed="f3e1ea",
            surface="12121a",
            onSurface="eeeef4",
        )
        for variant in brand.templates():
            source = brand.svg(False, colors, variant)
            self.assertEqual(ET.fromstring(source).get("viewBox"), brand.VIEWBOX)
            for size in (16, 26, 30, 48, 256, 512):
                image = brand.raster(source, size)
                self.assertEqual(image.size, (size, size))
                self.assertIsNotNone(image.getbbox())
                for corner in (
                    (0, 0),
                    (size - 1, 0),
                    (0, size - 1),
                    (size - 1, size - 1),
                ):
                    self.assertEqual(image.getpixel(corner)[3], 0)

    def test_qt_javascript_color_binding_agrees_with_publisher(self):
        node = shutil.which("node")
        command = (
            [node]
            if node
            else ["nacre-ai-tools", "node"]
            if shutil.which("nacre-ai-tools")
            else None
        )
        if command is None:
            self.skipTest("JavaScript runtime unavailable")
        source = (
            (ROOT.parent / "shell/branding/LogoData.js")
            .read_text()
            .replace(".pragma library", "")
        )
        colors = dict(
            primary="fefaf1",
            secondary="918178",
            tertiary="404077",
            primaryFixed="fffafa",
            surface="fff3df",
            frame="fff3df",
            onSurface="171310",
        )
        args = [
            "#" + colors[k]
            for k in (
                "primary",
                "secondary",
                "tertiary",
                "primaryFixed",
                "frame",
                "onSurface",
            )
        ] + [True]
        with tempfile.TemporaryDirectory() as folder:
            script = Path(folder) / "binding.js"
            for application in ("", *brand.APPLICATIONS):
                for compact in (True, False):
                    variant = (application + "-" if application else "") + (
                        "compact" if compact else "full"
                    )
                    flags = [application == app for app in brand.APPLICATIONS]
                    script.write_text(
                        source
                        + "\nprocess.stdout.write(colored(..."
                        + json.dumps([*args[:-1], compact, *flags])
                        + "));"
                    )
                    result = subprocess.check_output([*command, str(script)], text=True)
                    self.assertEqual(result, brand.svg(False, colors, variant))

    def test_ai_mark_adds_only_role_colored_letters_under_the_right_lip(self):
        regular = ET.fromstring(brand.templates()["compact"])
        ai = ET.fromstring(brand.templates()["ai-compact"])
        self.assertEqual(
            [ET.tostring(node).strip() for node in regular],
            [ET.tostring(node).strip() for node in list(ai)[:-1]],
        )
        self.assertEqual(ai[-1].get("aria-label"), "AI")
        self.assertEqual(
            [node.get("fill") for node in ai[-1]],
            ["@AI_PRIMARY@", "@AI_SECONDARY@"],
        )
        colors = dict(primary="ce5483", secondary="7ea4da", frame="12121a")
        normal = brand.raster(brand.svg(False, colors, "compact"), 580)
        labeled = brand.raster(brand.svg(False, colors, "ai-compact"), 580)
        # The shell geometry is untouched; the empty lower-right area gains ink.
        self.assertEqual(
            normal.crop((0, 0, 580, 320)).tobytes(),
            labeled.crop((0, 0, 580, 320)).tobytes(),
        )
        self.assertIsNone(normal.crop((390, 330, 560, 460)).getbbox())
        self.assertIsNotNone(labeled.crop((390, 330, 560, 460)).getbbox())
