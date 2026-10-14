import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
const context=vm.createContext({});
vm.runInContext(fs.readFileSync(process.argv[2],'utf8').replace('.pragma library',''),context);
const source=fs.readFileSync(process.argv[3],'utf8'), menu=context.merge(source,'');
assert.equal(menu.rows.length,142);
assert.equal(context.resolve('software.apps',menu.rows),'software.discover');
assert.equal(menu.rows.find(row=>row.id==='software.themes').provider,'software-themes');
assert.equal(menu.rows.find(row=>row.id==='software.extensions').provider,'software-extensions');
assert.equal(menu.rows.find(row=>row.id==='actions.capture.record-screen').value,'record-screen');
assert.equal(context.rail(menu.rows).length,9);
const support = menu.rows.find(row => row.id === 'health.support-report');
assert.equal(support.operation, 'tool');
assert.equal(support.value, 'support-report');
assert.equal(context.resolve('settings',menu.rows),'setup');
assert.equal(context.resolve('clipboard',menu.rows),'actions.clipboard');
assert.equal(context.resolve('poweroff',menu.rows),'');
assert.equal(context.resolve('power',menu.rows),'power');
const custom=context.merge(source,JSON.stringify({version:1,entries:[{id:'actions.terminal',label:'My terminal'},{id:'actions.mine',kind:'action',label:'My command',operation:'command',arguments:['my-tool','literal $(value)'] }]}));
assert.equal(custom.rows.find(row=>row.id==='actions.terminal').label,'My terminal');
assert.equal(custom.rows.find(row=>row.id==='actions.mine').arguments[1],'literal $(value)');
for(const entries of [[{id:'missing.child',kind:'action',label:'Bad',operation:'url',value:'https://example.org'}],[{id:'home',hidden:true}],[{id:'actions.mine',kind:'action',label:'Bad',operation:'eval',value:'x'}],[{id:'actions.mine',kind:'action',label:'Bad',operation:'command',arguments:'shell text'}]]) {
    const result=context.merge(source,JSON.stringify({version:1,entries}));
    assert(result.error);assert.equal(result.rows.length,menu.rows.length);
}
const hidden=context.merge(source,JSON.stringify({version:1,entries:[{id:'actions.capture',hidden:true}]}));
assert(!hidden.rows.some(row=>row.id.startsWith('actions.capture')));
const rows=menu.rows.map(row=>context.entry(row,menu.rows));
assert.equal(context.ranked(rows,'wifi')[0].id,'setup.network');
assert.equal(context.ranked(rows,'')[0].id,rows[0].id);
assert.equal(context.scope('   > wifi').kind,'actions');
assert.equal(context.scope('# notes').text,'notes');
assert.equal(context.scope('? question').kind,'ai');
assert.equal(context.firstEnabled([{disabled:true},{disabled:false}]),1);
assert.equal(context.step([{disabled:false},{disabled:true},{disabled:false}],0,1),2);
assert.equal(context.stable({id:'style.colors',kind:'action'}),'nacre-menu:style.colors');
assert.equal(context.stable({id:'editor',kind:'app'}),'editor');
console.log('Menu hierarchy, overrides, routing, ranking and stable IDs passed');

const apps=[{id:"code",kind:"app",name:"Visual Studio Code"},{id:"spotify",kind:"app",name:"Spotify"},{id:"edit-one",kind:"app",name:"Editor One"},{id:"edit-two",kind:"app",name:"Editor Two"}];
assert.equal(context.ranked(apps,"vsc")[0].id,"code");
assert.equal(context.ranked(apps,"spotfy")[0].id,"spotify");
assert.equal(context.ranked(apps,"editor",{"edit-two":{count:20,last:100}},[],100)[0].id,"edit-two");
assert.equal(context.ranked(apps,"Editor One",{"edit-two":{count:100000,last:100}},[],100)[0].id,"edit-one");
assert.equal(context.ranked(apps,"unrelated").length,0);
assert.equal(context.scope(": report").kind,"files");

// An exact desktop keyword must not displace the app name being typed, even
// when that keyword's app is heavily used and recently launched.
const desktopApps = [
    context.application({id:'io.missioncenter.MissionCenter',name:'Mission Center',keywords:['Disc','CPU','Memory']}),
    context.application({id:'discord',name:'Discord',keywords:[]})
];
const missionUsage = {'io.missioncenter.MissionCenter':{count:100000,last:100}};
for (const query of ['d','di','dis','disc','disco','discord']) {
    assert.equal(context.ranked(desktopApps,query,missionUsage,['io.missioncenter.MissionCenter'],100)[0].id,'discord');
}
assert.equal(context.ranked(desktopApps,'cpu')[0].id,'io.missioncenter.MissionCenter');
assert.equal(context.ranked(desktopApps,'disc')[1].id,'io.missioncenter.MissionCenter');

// Copied search text must not jump above the app it refers to.
const copiedText={id:"copied-disc",kind:"action",name:"disc",clipboard:true};
const appDiscord={id:"discord",kind:"app",name:"Discord"};
assert.equal(context.ranked([copiedText,appDiscord],"disc",{"nacre-menu:copied-disc":{count:1e9,last:Date.now()/1000}},["nacre-menu:copied-disc"])[0].id,"discord");

const sourceSettings=vm.createContext({});
vm.runInContext(fs.readFileSync(new URL('../../shell/modules/launcher/launcher-sources.js',import.meta.url),'utf8').replace('.pragma library',''),sourceSettings);
assert.equal(sourceSettings.decode('').webEngine,'duckduckgo');
assert.equal(sourceSettings.decode(JSON.stringify({version:1,files:false,brain:false,webEngine:'brave'})).files,false);
for (const text of ['broken',JSON.stringify({version:2,files:false}),JSON.stringify({version:1,files:'false'}),JSON.stringify({version:1,webEngine:'unknown'})]) assert.equal(sourceSettings.decode(text).files,true);
assert.equal(sourceSettings.webUrl('google','space & value'),'https://www.google.com/search?q=space%20%26%20value');
assert.equal(sourceSettings.webUrl('brave','space'),'https://search.brave.com/search?q=space');
