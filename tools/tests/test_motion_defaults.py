from pathlib import Path
import shutil
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[2]


@unittest.skipUnless(shutil.which("lua"), "Lua unavailable")
class MotionDefaultsTests(unittest.TestCase):
    def test_native_inheritance_and_curve_ownership(self):
        source = """
local curves, leaves = {}, {}
hl = {
    config = function(values) assert(values["animations.enabled"] == true) end,
    curve = function(name, spec)
        assert(not curves[name])
        assert(name:match("^nacre_"))
        assert(spec.type == "bezier" and #spec.points == 2)
        curves[name] = spec
    end,
    animation = function(spec)
        assert(not leaves[spec.leaf])
        assert(spec.speed > 0 and spec.enabled == true)
        assert(spec.bezier == "default" or curves[spec.bezier])
        assert(spec.style ~= "loop")
        leaves[spec.leaf] = spec
    end,
}
dofile(arg[1])
local count = 0
for _ in pairs(curves) do count = count + 1 end
assert(count == 4)
count = 0
for _ in pairs(leaves) do count = count + 1 end
assert(count == 11)
-- Explicitly setting these would destroy the current parent inheritance.
for _, name in ipairs({"global", "windowsMove", "layers", "fadeIn", "fadeOut", "fadeLayers", "workspacesIn", "workspacesOut", "borderangle", "glowangle", "shadowangle"}) do
    assert(leaves[name] == nil)
end
assert(leaves.specialWorkspace.style == "slidevert")
assert(leaves.windows.style == leaves.windowsIn.style)
"""
        subprocess.run(
            ["lua", "-", str(ROOT / "hypr/conf/animation.lua")],
            input=source,
            capture_output=True,
            text=True,
            check=True,
        )
