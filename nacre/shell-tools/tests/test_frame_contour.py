"""Joined geometry must remove internal seams in the actual QML JS engine."""

from pathlib import Path
import os
import shutil
import subprocess
import tempfile
import unittest

SOURCE = Path(__file__).resolve().parents[2] / "shell/modules/drawers/contour.js"


class ContourTests(unittest.TestCase):
    def test_union_seams_holes_partial_edges_and_small_transitions(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            shutil.copy2(SOURCE, target / "contour.js")
            (target / "tst_contour.qml").write_text(r"""import QtQuick
import QtTest
import "contour.js" as Contour
TestCase {
 name:"JoinedNacreContour";visible:true;when:windowShown
 function rect(x,y,width,height){return {x,y,width,height};}
 function frame(){return [rect(0,0,600,50),rect(0,0,10,400),rect(590,0,10,400),rect(0,390,600,10)];}
 function surface(boxes){return Contour.geometry(600,400,boxes,20,[]);}
 function sum(result){return result.loops.reduce((value,loop)=>value+Contour.area(loop),0);}
 function test_frame_and_join_have_no_internal_seam(){
  const closed=surface(frame());compare(closed.loops.length,2);compare(sum(closed),42800);
  const joined=surface([...frame(),rect(200,50,200,100)]);
  compare(joined.loops.length,2);compare(sum(joined),62800);
  for(const loop of joined.loops) for(let i=0;i<loop.length;i++){
   const a=loop[i],b=loop[(i+1)%loop.length];
   verify(!(a.y===50&&b.y===50&&Math.min(a.x,b.x)<210&&Math.max(a.x,b.x)>210));
  }
 }
 function test_overlaps_and_partial_edges(){
  const overlap=surface([...frame(),rect(380,50,200,200),rect(490,50,100,220)]);
  compare(overlap.loops.length,2);compare(sum(overlap),86800);
  const edges=frame(),partial=surface([edges[0],edges[1],edges[3]]);
  compare(partial.loops.length,1);verify(partial.rim.length>0);
  const empty=surface([]);compare(empty.body,"");compare(empty.rim,"");compare(empty.loops.length,0);
  const diagonal=surface([rect(0,0,20,20),rect(20,20,20,20)]);compare(sum(diagonal),800);
 }
 function test_closing_slivers_and_flat_handle(){
  for(const width of [0,.0001,.001,.2,1,2,5,200]){
   const result=surface([...frame(),rect(590-width,120,width,140)]);
   verify(!/NaN|undefined|Infinity/.test(result.body+result.rim));
  }
  const handle={x:232,y:50,width:136,height:6,edge:"top",shoulder:12};
  const held=Contour.geometry(600,400,[...frame(),handle],20,[handle]);compare(sum(held),43616);
 }
}
""")
            result = subprocess.run(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                capture_output=True,
                text=True,
                timeout=20,
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
