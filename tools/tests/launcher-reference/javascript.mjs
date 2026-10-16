// Developer-only reference capture. Never overwrite an existing frozen output.
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import crypto from 'node:crypto';
import {execFileSync} from 'node:child_process';

const [root, output] = process.argv.slice(2);
if (!root || !output) throw new Error('Expected repository root and new output path');
if (fs.existsSync(output)) throw new Error('Frozen output already exists');
const sources = {};
function load(relative) {
    const bytes = fs.readFileSync(path.join(root, relative));
    sources[relative] = crypto.createHash('sha256').update(bytes).digest('hex');
    const context = vm.createContext({});
    vm.runInContext(bytes.toString('utf8').replace('.pragma library', ''), context);
    return context;
}
const search = load('nacre/shell/services/app-search.js');
const model = load('nacre/shell/modules/launcher/menu-model.js');
const browser = load('nacre/shell/modules/launcher/app-browser.js');
const sourceOptions = load('nacre/shell/modules/launcher/launcher-sources.js');
const menuPath = 'nacre/shell-tools/launcher-menu.json';
const menuText = fs.readFileSync(path.join(root, menuPath), 'utf8');
sources[menuPath] = crypto.createHash('sha256').update(menuText).digest('hex');
const menu = model.merge(menuText, '');
if (menu.error || menu.rows.length !== 148) throw new Error('Unexpected current menu catalog');
const rawApps = [
    {id:'discord',name:'Discord',genericName:'Chat',comment:'Talk with friends',keywords:['chat','voice'],icon:'discord',categories:['Network']},
    {id:'mission',name:'Mission Center',genericName:'System monitor',comment:'CPU and memory',keywords:['Disc','CPU','Memory'],icon:'monitor',categories:['System']},
    {id:'code',name:'Visual Studio Code',genericName:'Editor',comment:'Edit files',keywords:['code','development'],icon:'code',categories:['Development']},
    {id:'editor-b',name:'Editor',genericName:'Text editor',comment:'Edit notes',keywords:[],icon:'edit',categories:['Utility']},
    {id:'editor-a',name:'Editor',genericName:'Text editor',comment:'Edit documents',keywords:['document'],icon:'edit',categories:['Office']},
    {id:'nacre-settings.desktop',name:'Nacre Settings',genericName:'Preferences',comment:'Configure the desktop',keywords:['wifi','display'],icon:'settings',categories:['Settings']},
    {id:'cafe',name:'Café Éditeur',genericName:'Editor',comment:'Résumé notes',keywords:['cafe','writing'],icon:'edit',categories:['Utility']},
    {id:'istanbul',name:'İstanbul Iı',genericName:'Language',comment:'Turkish letters',keywords:['şehir'],icon:'language',categories:['Education']},
    {id:'greek',name:'ΟΣ Σίσυφος',genericName:'Greek',comment:'Letters',keywords:['Ελλάδα'],icon:'language',categories:['Education']},
    {id:'emoji',name:'Emoji 😀 Studio',genericName:'Symbols',comment:'Smile',keywords:['faces'],icon:'mood',categories:['Graphics']},
    {id:'boundary-a',name:'A'.repeat(255)+'😀x',genericName:'Boundary',comment:'UTF16 search limit',keywords:[],icon:'edit',categories:[]},
    {id:'boundary-b',name:'A'.repeat(255)+'😁x',genericName:'Boundary',comment:'UTF16 search limit',keywords:[],icon:'edit',categories:[]}
];
const largeApps = Array.from({length:320},(_,i)=>({id:'app-'+String(i).padStart(4,'0'),name:'Application '+String(i).padStart(4,'0'),genericName:'Fixture application',comment:'Public search fixture',keywords:['sample'],icon:'application-x-executable',categories:['Utility']}));
const appRows = rawApps.map(app=>model.application(app));
const staticRows = menu.rows.map(row=>model.entry(row,menu.rows));
const specialRows = [
    {id:'clip-disc',kind:'action',name:'disc',description:'Copied Discord text',clipboard:true},
    {id:'wifi-action',kind:'action',name:'Wi-Fi',description:'Wireless settings',aliases:['wireless','network'],disabled:true},
    {id:'alias',kind:'action',name:'Other tool',aliases:['editor','Visual Studio'],breadcrumb:'Actions'},
    {id:'hidden-caption',kind:'action',label:'Caption only',description:'A fallback label'},
    {id:'favorite-token',kind:'action',name:'Pinned note',favoriteId:'custom-stable',description:'Fixture favorite'}
];
const actions = [
    {name:'Terminal',action:'terminal',value:'',aliases:['console']},
    {name:'Power',action:'power',value:''},
    {name:'Editor',action:'editor',value:'space & café/😀!()\'',aliases:['text editor','edit']},
    {name:'Other editor',action:'open',value:'fixture',aliases:['edit']}
];
const catalogs = {rawApps,largeApps,appRows,staticRows,specialRows,mixed:appRows.concat(staticRows,specialRows),largeRows:largeApps.map(app=>model.application(app)),actions};
const now=1720000000;
const history={mission:{count:100000,last:now},'editor-a':{count:20,last:now-86400},'editor-b':{count:0,last:now+100},'nacre-menu:clip-disc':{count:1e9,last:now},'nacre-menu:wifi-action':{count:-2,last:null}};
const recent=['mission','nacre-menu:clip-disc','editor-a','editor-b',...Array.from({length:16},(_,i)=>'unused-'+i)];
const cases=[];
function add(op,input,calculate) {
    const expected=calculate();
    cases.push({id:cases.length,op,...input,expected:expected===undefined?null:expected});
}
const queries=['',' ','d','di','dis','disc','disco','discord','cpu','vsc','spotfy','edtor','editor','Editor One','text editor','wifi','wireless','setings','aplication','cafe','café','résumé','İ','ΟΣ','😀','studio','unknown result','  mixed\t whitespace  ','a_b:c-d','\uFEFFeditor\uFEFF','\u0085editor\u0085','A'.repeat(255)+'😀','a'.repeat(257),'> wifi','= 1+1','# notes','? question','/ help',': report'];
for (const query of queries) {
    add('app.normalize',{value:query},()=>search.normalize(query));
    add('model.normalized',{value:query},()=>model.normalized(query));
    add('model.scope',{query},()=>model.scope(query));
    add('app.query',{catalog:'rawApps',query},()=>search.query(rawApps.map(app=>search.prepare(app)),query).map(app=>app.id));
    for (const catalog of ['mixed','largeRows']) {
        add('model.ranked',{catalog,query,usage:history,recent,now},()=>model.ranked(catalogs[catalog],query,history,recent,now).map(row=>row.id));
    }
    add('browser.rankedActions',{catalog:'actions',query},()=>browser.rankedActions(actions,query));
}
for (const [value,query] of [['','a'],['abc',''],['abc','abc'],['abcdef','abc'],['one two','two'],['visual studio code','vsc'],['alphabet','pha'],['alphabet','abt'],['alphabet','zzz'],['😀😀','😀'],['a 😀 b','😀'],['résumé','ré'],['a'.repeat(220)+'b','ab']]) {
    add('app.score',{value,query},()=>search.score(value,query));
}
for (const [a,b] of [['editor','edtor'],['application','aplication'],['abcd','abxd'],['abc','abcdef'],['😀','x'],['😀a','😀b'],['café','cafe'],['','a']]) {
    for (const limit of [0,1,2]) add('model.distance',{a,b,limit},()=>model.distance(a,b,limit));
}
for (const catalog of ['appRows','staticRows','specialRows','mixed','largeRows']) {
    add('model.alphabetical',{catalog},()=>model.alphabetical(catalogs[catalog]).map(row=>row.id));
    for (const row of catalogs[catalog].slice(0,6)) add('model.stable',{row},()=>model.stable(row));
}
for (const entry of rawApps) add('model.application',{entry},()=>model.application(entry));
add('browser.available',{catalog:'rawApps'},()=>browser.available(rawApps));
add('browser.actions',{catalog:'actions'},()=>browser.actions(actions));
add('app.query',{catalog:'largeApps',query:''},()=>search.query(largeApps.map(app=>search.prepare(app)),'').map(app=>app.id));
add('app.query',{catalog:'largeApps',query:'application'},()=>search.query(largeApps.map(app=>search.prepare(app)),'application').map(app=>app.id));
for (const route of ['home','apps','settings','clipboard','power','poweroff','software.apps','style.colors','missing']) {
    add('model.resolve',{route},()=>model.resolve(route,menu.rows));
    add('model.children',{route},()=>model.children(route,menu.rows));
    add('model.trail',{route},()=>model.trail(route,menu.rows));
}
add('model.rail',{preferences:{order:['power','style','apps'],hidden:['software','home']}},()=>model.rail(menu.rows,{order:['power','style','apps'],hidden:['software','home']}));
for (const text of ['',JSON.stringify({version:1,files:false,brain:false,webEngine:'brave'}),JSON.stringify({version:2,files:false})]) add('sources.decode',{text},()=>sourceOptions.decode(text));
for (const engine of ['google','duckduckgo','brave']) add('sources.webUrl',{engine,query:'space & café 😀'},()=>sourceOptions.webUrl(engine,'space & café 😀'));
const reference={source_revision:execFileSync('git',['rev-parse','HEAD'],{cwd:root,encoding:'utf8'}).trim(),sources,locale:Intl.DateTimeFormat().resolvedOptions().locale,clock:now,catalogs,cases};
fs.writeFileSync(output,JSON.stringify(reference,null,2)+'\n',{flag:'wx'});
console.log(JSON.stringify({cases:cases.length,bytes:fs.statSync(output).size,sha256:crypto.createHash('sha256').update(fs.readFileSync(output)).digest('hex'),source_revision:reference.source_revision,locale:reference.locale}));
