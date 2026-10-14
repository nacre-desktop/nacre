"""City photo metadata admits bounded, geographically relevant free raster images."""

from pathlib import Path
import shutil
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[2]


class WeatherPhotoTests(unittest.TestCase):
    def test_provider_metadata_urls_geography_credit_and_cache(self):
        node = shutil.which("node")
        if not node:
            self.skipTest("Node unavailable")
        model = ROOT / "shell/services/weather-photo.js"
        code = r"""
const fs=require('fs'),vm=require('vm'),assert=require('assert'),x={};vm.createContext(x);
vm.runInContext(fs.readFileSync(process.argv[1],'utf8').replace('.pragma library',''),x);
const url='https://upload.wikimedia.org/wikipedia/commons/a/a5/City.jpg';
const source='https://commons.wikimedia.org/wiki/File:City.jpg';
const info={thumburl:url+'?utm_source=example',descriptionurl:source,mime:'image/jpeg',thumbwidth:960,thumbheight:600,extmetadata:{Artist:{value:'<a href="https://evil.test">Public photographer</a>'},LicenseShortName:{value:'CC BY-SA 4.0'}}};
const data={query:{pages:[{imageinfo:[info]}]}};
const accepted=x.metadata(data);assert(accepted);assert.strictEqual(accepted.author,'Public photographer');assert.strictEqual(accepted.url,url);
for(const bad of ['http://upload.wikimedia.org/wikipedia/commons/a/a5/City.jpg','https://upload.wikimedia.org.evil.test/a.jpg','https://upload.wikimedia.org@127.0.0.1/a.jpg','file:///etc/passwd','https://upload.wikimedia.org/wikipedia/commons/%2e%2e/secret'])assert(!x.imageUrl(bad));
assert(!x.metadata({query:{pages:[{imageinfo:[{...info,mime:'image/svg+xml'}]}]}}));
assert(!x.metadata({query:{pages:[{imageinfo:[{...info,thumbwidth:20000}]}]}}));
assert(!x.metadata({query:{pages:[{imageinfo:[{...info,extmetadata:{...info.extmetadata,LicenseShortName:{value:'All rights reserved'}}}]}]}}));
const place={query:{pages:[{ns:0,pageimage:'City.jpg',coordinates:[{lat:58.1,lon:8,globe:'earth'}]}]}};
assert.strictEqual(x.cityImage(place,58.1,8),'City.jpg');assert.strictEqual(x.cityImage(place,40,-80),'');assert.strictEqual(x.cityImage(place,undefined,8),'');
const cache={...accepted,city:'Demo city',target:'local-coordinate-key',checked:100};
assert(x.cached(cache,'Demo city',200,'local-coordinate-key'));assert(!x.cached(cache,'Other city',200,'local-coordinate-key'));assert(!x.cached(cache,'Demo city',700000,'local-coordinate-key'));assert(!x.cached(cache,'Demo city',200,'other-coordinate-key'));
"""
        result = subprocess.run(
            [node, "-e", code, str(model)], capture_output=True, text=True, timeout=10
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
