import fs from 'node:fs';
const port=process.env.BRAIN_TEST_CDP||17946,url=process.env.BRAIN_TEST_URL||'http://127.0.0.1:17845';
const target=await(await fetch(`http://127.0.0.1:${port}/json/new?about:blank`,{method:'PUT'})).json(),socket=new WebSocket(target.webSocketDebuggerUrl);
await new Promise(r=>socket.addEventListener('open',r,{once:true}));let seq=0;const pending=new Map(),errors=[];
socket.addEventListener('message',e=>{const m=JSON.parse(e.data);if(m.id){const p=pending.get(m.id);pending.delete(m.id);m.error?p.reject(m.error):p.resolve(m.result);}else if(m.method==='Runtime.exceptionThrown')errors.push(m.params.exceptionDetails.exception?.description||m.params.exceptionDetails.text);});
function call(method,params={}){return new Promise((resolve,reject)=>{const id=++seq;pending.set(id,{resolve,reject});socket.send(JSON.stringify({id,method,params}));});}
async function evaluate(expression){const r=await call('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true});if(r.exceptionDetails)throw Error(r.exceptionDetails.exception?.description||r.exceptionDetails.text);return r.result.value;}
async function until(expression){for(let i=0;i<60;i++){if(await evaluate(expression))return;await new Promise(r=>setTimeout(r,150));}throw Error('Timed out: '+expression);}
const output=process.env.HOME+'/.local/state/nacre/brain/qa/redesign';fs.mkdirSync(output,{recursive:true,mode:0o700});
async function screenshot(name){const r=await call('Page.captureScreenshot',{format:'png'});fs.writeFileSync(output+'/'+name+'.png',Buffer.from(r.data,'base64'));}
try{
 await call('Runtime.enable');await call('Emulation.setDeviceMetricsOverride',{width:1560,height:1000,deviceScaleFactor:1,mobile:false});await call('Page.navigate',{url});await until('typeof ready!=="undefined"&&ready&&document.querySelectorAll(".subject-card").length>0');
 const initial=await evaluate('({subjects:subjects().length,notes:graph.noteCount,canvas:document.querySelectorAll("canvas").length,accent:getComputedStyle(document.documentElement).getPropertyValue("--accent").trim()})');if(initial.canvas||initial.subjects<3||initial.notes<1)throw Error('Overview failed');await screenshot('overview');
 await evaluate('select(subjects()[0].id)');await until('document.querySelector("#title").textContent===subjects()[0].label');await screenshot('subject');
 await evaluate('document.querySelector("[data-view=map]").click()');await until('document.querySelectorAll(".map-node").length>1');
 const point=await evaluate('(()=>{const b=document.querySelectorAll(".map-node")[1].getBoundingClientRect();return {x:b.x+b.width/2,y:b.y+b.height/2,before:state.focus};})()');
 await call('Input.dispatchMouseEvent',{type:'mousePressed',x:point.x,y:point.y,button:'left',clickCount:1});await call('Input.dispatchMouseEvent',{type:'mouseReleased',x:point.x,y:point.y,button:'left',clickCount:1});if(await evaluate('state.focus')===point.before)throw Error('Real map click did not focus');await screenshot('connections');
 await evaluate('select(graph.nodes.find(n=>n.path==="wiki/knowledge-workflow.md")?.id||graph.nodes.find(n=>n.kind==="note").id,{view:"notes"})');await until('document.querySelector(".reader p")!==null');await screenshot('reader');if(!(await evaluate('document.querySelector(".grouping")!==null')))throw Error('Grouping feedback missing');
 const security=await evaluate('(()=>{const c=document.createElement("div");markdown(c,"# Test\\n\\n<script>window.bad=1</script>\\n\\n[bad](javascript:alert(1))\\n\\n**Bold** and `code`","fixture.md");return {script:c.querySelectorAll("script").length,unsafe:c.querySelectorAll("[href^=javascript]").length,bold:c.querySelectorAll("strong").length};})()');if(security.script||security.unsafe||security.bold!==1)throw Error('Unsafe or broken Markdown');
 await evaluate('document.querySelector(".reader").dataset.identity="kept"');await evaluate('load()');if(!(await evaluate('document.querySelector(".reader").dataset.identity==="kept"')))throw Error('Reader lost on refresh');
 await evaluate('$("search").value="private";$("search").dispatchEvent(new Event("input"))');
 await until('searchIds!==null');
 const searchState = await evaluate(`({
   view: state.view,
   matches: searchIds.size,
   rows: document.querySelectorAll("#content .note-row").length,
   readerHidden: document.querySelector("#inspector").hidden,
   resultsVisible: !document.querySelector("#content").hidden
 })`);
 if (searchState.view !== "notes" || !searchState.matches || !searchState.rows ||
     !searchState.readerHidden || !searchState.resultsVisible)
   throw Error('Full text search did not leave the note reader');
 await screenshot('search');
 await evaluate('home();graph.nodes.push({id:"qa-subject",kind:"hub",label:"QA subject",count:0,activity:.2});byId.set("qa-subject",graph.nodes.at(-1));render()');if(!(await evaluate('document.querySelectorAll(".subject").length===subjects().length&&document.querySelectorAll(".subject-card").length===subjects().length')))throw Error('Dynamic subject growth failed');await evaluate('load(true)');
 await evaluate('const n={id:"qa-unverified",kind:"note",label:"QA unverified",path:"qa.md",confidence:"unverified",date:"2026-10-03"};graph.nodes.push(n);byId.set(n.id,n);home();state.view="notes";state.filter="verified";renderMain()');if(await evaluate('[...document.querySelectorAll(".note-title")].some(n=>n.textContent==="QA unverified")'))throw Error('Unverified incorrectly shown as verified');await evaluate('state.filter="all";load(true)');
 await evaluate('graph.theme={...graph.theme,surface:"faf8f3",surfaceContainer:"f1eee8",surfaceContainerHigh:"e9e5dc",onSurface:"25211b",onSurfaceVariant:"625b50",outlineVariant:"d3cabc",primary:"73592a",onPrimary:"ffffff",secondary:"665b45",tertiary:"526441"};theme();home()');await screenshot('light-palette');await evaluate('load(true)');
 for(const [width,height]of [[900,850],[720,850],[420,850]]){await call('Emulation.setDeviceMetricsOverride',{width,height,deviceScaleFactor:1,mobile:false});await evaluate('home()');if(!(await evaluate(`document.documentElement.scrollWidth<=${width}+1`)))throw Error('Horizontal overflow at '+width);await screenshot('responsive-'+width);}
 if(errors.length)throw Error([...new Set(errors)].join('\n'));console.log(JSON.stringify({initial,overview:'passed',mapClick:'passed',noteReader:'passed',fullText:'passed',markdownSafety:'passed',dynamicSubjects:'passed',confidenceFilter:'passed',lightPalette:'passed',responsive:'passed',browserErrors:errors.length,screenshots:output}));
}finally{socket.close();await fetch(`http://127.0.0.1:${port}/json/close/${target.id}`);}
