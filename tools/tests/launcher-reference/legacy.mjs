// Freeze the existing legacy favorites provider before porting its default inputs.
import fs from 'node:fs';
import vm from 'node:vm';
import crypto from 'node:crypto';
import {execFileSync} from 'node:child_process';
const [root, output] = process.argv.slice(2);
if (!root || !output || fs.existsSync(output)) throw Error('Expected repository and NEW output path');
const revision='83a9a9a493dcdc53f8fe42c94f01ac2553694035';
const sources={};
function source(name){const bytes=execFileSync('git',['show',revision+':'+name],{cwd:root});sources[name]=crypto.createHash('sha256').update(bytes).digest('hex');return bytes.toString('utf8');}
const qml=source('nacre/shell/services/DesktopActions.qml');
const expression=qml.slice(qml.indexOf('readonly property var list:')+'readonly property var list:'.length,qml.indexOf('    IpcHandler')).trim();
const legacy=vm.createContext({});vm.runInContext(source('nacre/shell/modules/launcher/app-browser.js').replace('.pragma library',''),legacy);
const cases=[];
for(const settings of [{},{animations:false,blur:false,shadow:false}]){
 const calls=[];const ctx=vm.createContext({DesktopSettings:{data:settings},NacreI18n:{tr:(id,fallback)=>{calls.push({id,fallback});return fallback;}}});
 const input=vm.runInContext('('+expression+')',ctx);
 const result=legacy.actions(input).map(entry=>Object.assign({},entry,{operation:'legacy-action',label:entry.name,breadcrumb:'Actions'}));
 cases.push({settings,input,calls,result});
}
const bytes=JSON.stringify({source_revision:revision,sources,cases},null,2)+'\n';
fs.writeFileSync(output,bytes,{flag:'wx'});
console.log(JSON.stringify({cases:cases.length,sha256:crypto.createHash('sha256').update(bytes).digest('hex')}));
