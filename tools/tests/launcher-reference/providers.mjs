// Read-only reference capture: exact internal functions, synthetic outside services.
import fs from 'node:fs';
import vm from 'node:vm';
import crypto from 'node:crypto';
import {execFileSync} from 'node:child_process';
const root = process.argv[2];
const output = process.argv[3];
if (!root) throw Error('Expected repository root and new output path');
if (!output || fs.existsSync(output)) throw Error('Choose a new frozen output path');
const sources = {};
function source(relative) {
  const bytes = execFileSync('git', ['show', '83a9a9a493dcdc53f8fe42c94f01ac2553694035:' + relative], {cwd:root});
  sources[relative] = crypto.createHash('sha256').update(bytes).digest('hex');
  return bytes.toString('utf8');
}
function module(name) {
  const context = vm.createContext({});
  vm.runInContext(source('nacre/shell/modules/launcher/' + name + '.js').replace('.pragma library', ''), context);
  return context;
}
const Model = module('menu-model'), Legacy = module('app-browser'), Sources = module('launcher-sources');
const menu = Model.merge(source('nacre/shell-tools/launcher-menu.json'), '');
if (menu.error || menu.rows.length !== 148) throw Error('Unexpected catalog');
const qml = source('nacre/shell/services/NacreMenuContext.qml');
const functions = [...qml.matchAll(/^    function ([A-Za-z0-9_]+)\([^]*?^    }/gm)];
if (functions.length < 30 || !functions.some(m => m[1] === 'content')) throw Error('Function extraction failed');
// Each extracted body is unchanged. QML property evaluation is supplied explicitly.
const functionText = functions.map(m => m[0]).join('\n');
const property = name => {
  const match = qml.match(new RegExp('^    readonly property var ' + name + ': (\\[[^]*?^    \\])', 'm'));
  if (!match) throw Error('Missing literal property ' + name);
  return vm.runInNewContext('(' + match[1] + ')', {NacreI18n:{tr}});
};
const epoch = 1720000000;
class FixedDate extends Date { constructor(...args) { super(...(args.length ? args : [epoch * 1000])); } static now() { return epoch * 1000; } }
for (const module of [Model, Legacy, Sources]) module.Date = FixedDate;
function tr(id, fallback, args = {}) {
  let result = typeof fallback === 'object' ? (args.count === 1 ? fallback.one : fallback.other) : fallback;
  return String(result).replace(/\{([^}]+)\}/g, (_, key) => String(args[key] ?? '{' + key + '}'));
}
const rawApps = [
  {id:'editor.desktop',name:'Fixture Editor',comment:'Edit synthetic documents',icon:'edit',keywords:['text'],categories:['Utility']},
  {id:'terminal.desktop',name:'Fixture Terminal',comment:'Run synthetic commands',icon:'terminal',keywords:['console'],categories:['System']},
  {id:'hidden.desktop',name:'Hidden Fixture',comment:'Hidden synthetic application',icon:'hidden',keywords:[],categories:[]}
];
const populated = {
  snapshot: {capabilities:{packages:true,aur:true,ai:true,niriWorkspaces:true},packages:[{name:'fixture-package',version:'1.2'},{name:'editor',version:'2.0'}],fonts:['IBM Plex Sans','Fixture Mono','Long ' + 'x'.repeat(81)],reminders:[{unit:'fixture-timer.timer',label:'Tea fixture',due:epoch+120}],webapps:[{id:'fixture-web',label:'Fixture Web',url:'https://example.invalid/fixture'}],stayAwake:true},
  DesktopSettings:{data:{dnd:true,interfaceFont:'IBM Plex Sans',monoFont:'Fixture Mono',showLabels:false}},
  DesktopExtras:{keys:[{menu:'actions.windows',key:'Super+W',description:'Synthetic windows'}],clips:[{id:11,title:'Fixture clipboard text',pinned:true},{id:12,title:'Fixture image',image:true,thumbnail:'/fixture/clip.png'}]},
  NacreAudio:{available:true,micAvailable:true,muted:true,micMuted:false,outputs:[{id:1,description:'Fixture speakers',ready:true},{id:2,nickname:'Fixture headphones',ready:false}]},
  NacreNetwork:{wifiEnabled:true,wifiAvailable:true,visibleNetworks:[{bssid:'00:00:00:00:00:01',ssid:'Fixture WiFi',strength:73,active:true},{bssid:'00:00:00:00:00:02',ssid:'',strength:20,active:false}]},
  NacreBluetooth:{powered:true,available:true,devices:[{address:'00:00:00:00:01:01',alias:'Fixture mouse',paired:true,connected:true},{address:'00:00:00:00:01:02',alias:'Fixture speaker',paired:false,connected:false}]},
  NacreWallpapers:{appearancePreferences:{paletteMode:'dark',paletteBackgroundFromWallpaper:true,palettePreset:'fixture-theme'},list:[{path:'/fixture/still.png',name:'Fixture still',dynamic:false,thumbnail:'/fixture/still-thumb.png'},{path:'/fixture/video.webm',name:'Fixture video',dynamic:true,poster:'/fixture/video-poster.png'}],current:'/fixture/still.png',paletteOptions:[{name:'Blue',accent:'#123456',colors:['#123456']},{accent:'#654321'}],selectedAccent:'#123456',palettes:[]},
  NacreCompositor:{backendName:'niri',windows:[{windowId:17,title:'Synthetic document',wmClass:'editor',workspace:{name:'One'}},{windowId:18,title:'',wmClass:'terminal',workspace:{name:'Two'}}]},
  NacreControlTools:{data:{nightLightEnabled:true,nightLightSupported:true,nightLightExternal:false}},
  NacrePlayers:{active:{canTogglePlaying:true,canGoNext:false,canGoPrevious:true}},
  PowerProfiles:{profile:1,hasPerformanceProfile:true},Updates:{count:3},personality:'calm',recordingActive:false,
  workflows:{data:{ready:true,recording:false,brainRoot:'/fixture/brain',capabilities:{share:true},entries:[{id:'snippet',label:'Fixture snippet',type:'snippet',text:'Synthetic text'},{id:'search',label:'Fixture Search',type:'search',url:'https://example.invalid/?q={query}'}],projects:[{id:'project',label:'Fixture project',path:'/fixture/project'}],captures:[{path:'/fixture/capture.png',name:'Fixture capture'}]}},
  previews:{emojiQuery:'smile',emojiMatches:[{value:'😀',name:'Grinning face',aliases:['smile'],category:'Faces'}]},
  brainQuery:'fixture',notes:[{path:'note.md',title:'Fixture note',preview:'Synthetic note preview'}],
  fileQuery:'fixture',fileRows:[{id:'file-1',name:'Fixture document',path:'/fixture/document.txt'}],
  packageQuery:'fixture',packageRows:[{name:'fixture-package',description:'Synthetic installed package',repository:'extra'},{name:'fixture-new',description:'Synthetic new package',repository:'AUR'}],
  library:{data:{themes:[{id:'fixture-theme',name:'Fixture palette',bundle:false,swatches:['#123456']}],extensions:[{id:'fixture-ext',name:'Fixture extension',description:'Synthetic extension',enabled:true,available:false,kinds:['panel'],actions:[{id:'go',name:'Fixture extension action'}]}],environments:{profiles:[{id:'fixture-env',name:'Fixture runtime',kind:'runtime',installed:false,available:true}]},system:{health:[{id:'fixture-health',section:'system.health',name:'Fixture health',description:'Synthetic health action',root:true}]},tui:{terminals:['fixture-terminal'],rows:[{id:'fixture-tui',name:'Fixture TUI',icon:'terminal',command:['fixture','--view'],terminal:'fixture-terminal',available:true}]},ai:{terminal:true,webApps:true,chatgptPackage:true}}},
  LauncherPreferences:{favorites:['editor.desktop','nacre-menu:actions.clipboard.11'],recent:['terminal.desktop'],usage:{'editor.desktop':{count:3,last:epoch}},hidden:['hidden.desktop']},
  canAsk:true
};
const empty = {
  snapshot:{capabilities:{},packages:[],fonts:[],reminders:[],webapps:[]},
  DesktopSettings:{data:{}},DesktopExtras:{keys:[],clips:[]},
  NacreAudio:{available:false,micAvailable:false,muted:false,micMuted:false,outputs:[]},
  NacreNetwork:{wifiEnabled:false,wifiAvailable:false,visibleNetworks:[]},
  NacreBluetooth:{powered:false,available:false,devices:[]},
  NacreWallpapers:{appearancePreferences:{},list:[],current:'',paletteOptions:[],selectedAccent:'',palettes:[]},
  NacreCompositor:{backendName:'hyprland',windows:[]},NacreControlTools:{data:{}},NacrePlayers:{active:null},PowerProfiles:{profile:0,hasPerformanceProfile:false},Updates:{count:0},personality:'',recordingActive:false,
  workflows:{data:{entries:[],projects:[],captures:[]}},previews:{emojiQuery:'',emojiMatches:[]},brainQuery:'',notes:[],fileQuery:'',fileRows:[],packageQuery:'',packageRows:[],library:{data:{}},
  LauncherPreferences:{favorites:[],recent:[],usage:{},hidden:[]},canAsk:false
};
const contexts = {empty, populated, disabledSources:{...structuredClone(populated),sourceOptions:{brain:false,files:false,calculator:false,actions:false,ai:false}}, busyAndUnavailable:{...structuredClone(populated),canAsk:false,NacreControlTools:{data:{nightLightEnabled:true,nightLightSupported:true,nightLightExternal:true}},PowerProfiles:{profile:0,hasPerformanceProfile:false},recordingActive:true,NacrePlayers:{active:null}}};
const cases = [];
for (const [name, seed] of Object.entries(contexts)) {
  const data = structuredClone(seed);
  const ctx = vm.createContext({...data,Model,Legacy,Sources,Date:FixedDate,
    Qt:{md5:s=>crypto.createHash('md5').update(String(s)).digest('hex')},
    NacreI18n:{tr,localize:x=>x,option:(_,x)=>x,number:x=>String(x),windowCaption:(title,cls)=>title||cls,time:d=>d.toISOString().slice(11,16)},
    PowerProfile:{PowerSaver:0,Balanced:1,Performance:2},
    catalog:structuredClone(menu),rows:structuredClone(menu.rows),legacy:[],sourceOptions:data.sourceOptions||Sources.defaults(),softwareCategories:property('softwareCategories'),
    NacreApps:{all:name==='empty'?[]:structuredClone(rawApps),list:name==='empty'?[]:structuredClone(rawApps.slice(0,2))}
  });
  ctx.root=ctx;
  ctx.apps=ctx.NacreApps.list.map(app=>Model.application(app));
  ctx.NacreAudio.sink=ctx.NacreAudio.outputs[0]||null;
  ctx.windowSearchRecords=ctx.NacreCompositor.windows.map(c=>({windowId:c.windowId,title:c.title,wmClass:c.wmClass,workspaceName:c.workspace?.name}));
  ctx.clipboardSearchRecords=ctx.DesktopExtras.clips;
  ctx.packageNames=new Set(ctx.snapshot.packages.map(p=>p.name));
  const names=new Set(),ids=new Set(),visible=new Map();
  for(const a of ctx.NacreApps.all){names.add(Model.normalized(a.name));ids.add(Model.normalized(a.id.replace(/\.desktop$/,'')));ids.add(Model.normalized(a.id.replace(/\.desktop$/,'').split('.').pop()));}
  for(const a of ctx.NacreApps.list){for(const k of[a.name,a.id.replace(/\.desktop$/,''),a.id.replace(/\.desktop$/,'').split('.').pop()])visible.set(Model.normalized(k),a.id);}
  ctx.softwareAppIndex={names,ids,visible};
  vm.runInContext(functionText,ctx);
  ctx.emojiRows=(name==='empty'?[]:property('emojiData')).map(e=>ctx.emojiRow(e));
  function call(method,args){const result=ctx[method](...args);cases.push({context:name,method,params:args,result:JSON.parse(JSON.stringify(result))});}
  for(const row of menu.rows.filter(r=>r.provider))for(const q of['','fixture','smile'])call('provider',[row.id,q]);
  call('provider',['does.not.exist','']);
  for(const route of ['home','apps','hidden','actions','actions.saved','actions.capture','actions.clipboard','style.fonts','software.discover','does.not.exist'])call('content',[route,'']);
  for(const query of['fixture','> fixture','= 1+2','# fixture',': fixture','? fixture','?','help','/'])for(const filter of['all','apps','clipboard','files','notes'])call('content',['home',query,filter,'all']);
  call('content',['software.discover','','all','ai']);
  call('content',['software.packages','fixture','all','all']);
}
const result={source_revision:'83a9a9a4',sources,locale:'en-US',timezone:'UTC',clock:epoch,runtime:'Node v22; exact extracted QML functions with synthetic outside services; no QML reactive property evaluation',extracted_functions:functions.map(m=>m[1]),contexts,rawApps,cases};
fs.writeFileSync(output,JSON.stringify(result,null,2)+'\n',{flag:'wx',mode:0o600});
console.log(JSON.stringify({cases:cases.length,sha256:crypto.createHash('sha256').update(fs.readFileSync(output)).digest('hex')}));
