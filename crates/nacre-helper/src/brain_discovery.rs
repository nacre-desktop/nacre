//! Native subject discovery for the Brain web API.
//!
//! The input and output deliberately use the same JSON boundary as
//! `brain.discovery.organize`; this keeps the data contract independent of
//! the server which hosts it.
use crate::{Error, Result};
use regex::Regex;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use unicode_normalization::UnicodeNormalization;
use unicode_casefold::UnicodeCaseFold;

fn err(s: impl Into<String>) -> Error { s.into().into() }

fn key(s: &str) -> String { s.nfkc().case_fold().collect::<String>().trim().to_owned() }

fn digest(s: &str, n: usize) -> String {
    let bytes = Sha256::digest(s.as_bytes());
    bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()[..n].to_owned()
}
fn prefix(s:&str,n:usize)->&str{let mut end=0;for (i,c) in s.char_indices(){if i+c.len_utf8()>n{break;}end=i+c.len_utf8();} &s[..end]}
fn suffix(s:&str,n:usize)->&str{let mut start=s.len();for (i,_) in s.char_indices().rev(){if s.len()-i>n{break;}start=i;} &s[start..]}

fn slug(s: &str) -> String {
    let normalized: String = s.nfkd().filter(|c| c.is_ascii()).collect::<String>().to_lowercase();
    let re = Regex::new("[^a-z0-9]+").expect("constant regex");
    let value = re.replace_all(&normalized, "-").trim_matches('-').chars().take(80).collect::<String>();
    if value.is_empty() { format!("subject-{}", digest(&key(s), 12)) } else { value }
}

fn items(v: &Value) -> Vec<String> {
    let s = v.as_str().unwrap_or("").trim_matches(|c| c == '[' || c == ']');
    s.split(',').map(|x| x.trim().trim_matches(['"', '\'']).trim().to_owned()).filter(|x| !x.is_empty()).collect()
}

fn tokens(s:&str)->Vec<String>{
    const STOP:&str="a an the and or but of to in on for with from by at as is are was were be been being this that these those it its my your our their we you they i me he she them have has had do does did can could should would will may into about after before through more most some any all each only other no not new now then than also just how what when where which who why use used using make made first last one two current latest source recorded reviewed confidence entity name parent project worlds topics tags category aliases status evidence note notes knowledge verified reported unverified sourcepage test tests passed check checks result results update updated updating completed complete ready fixed fix added add deployed deploy deployment implement implementation source code files file path paths branch worktree commit git user assistant workflow context saved private local scope preserve existing unchanged previous version changes changed change correct documented requested default actual explicit native shell runtime process processes window windows gui ui cli service services script scripts command commands configuration config settings clean done shows show setup verified validation milestone historical observations observation captured capture memory suggestions suggested subject subjects";
    let stop:BTreeSet<&str>=STOP.split_whitespace().collect();
    Regex::new(r"[^\W\d_]{3,}").unwrap().find_iter(s).map(|m|m.as_str().to_lowercase()).filter(|w|!stop.contains(w.as_str())).collect()
}
fn note_similarity(a:&Value,b:&Value)->f64{
    let x=tokens(&format!("{} {}",a["label"].as_str().unwrap_or(""),clean(a["text"].as_str().unwrap_or(""))));
    let y=tokens(&format!("{} {}",b["label"].as_str().unwrap_or(""),clean(b["text"].as_str().unwrap_or(""))));
    let xs:BTreeSet<_>=x.iter().collect();let ys:BTreeSet<_>=y.iter().collect();
    if xs.is_empty()||ys.is_empty(){return 0.0;} xs.intersection(&ys).count() as f64 / ((xs.len()*ys.len()) as f64).sqrt()
}
fn vectors(notes:&[Value])->BTreeMap<String,BTreeMap<String,f64>>{
    let mut counts=BTreeMap::new();let mut df:BTreeMap<String,usize>=BTreeMap::new();
    for n in notes{let id=n["id"].as_str().unwrap_or("").to_owned();let mut c:BTreeMap<String,usize>=BTreeMap::new();let (words,_)=features(n);for w in words{*c.entry(w).or_default()+=1;}for w in c.keys(){*df.entry(w.clone()).or_default()+=1;}counts.insert(id,c);}
    counts.into_iter().map(|(id,c)|{let mut v:BTreeMap<String,f64>=c.into_iter().map(|(w,tf)|{let freq=*df.get(&w).unwrap_or(&0) as f64;let weight=(1.0+(tf as f64).ln())*(1.0+(notes.len() as f64)/(1.0+freq)).ln();(w,weight)}).collect();let norm=v.values().map(|x|x*x).sum::<f64>().sqrt();if norm>0.0{for x in v.values_mut(){*x/=norm;}}(id,v)}).collect()
}
fn vector_similarity(a:&str,b:&str,lex:&BTreeMap<String,BTreeMap<String,f64>>,dense:&Value)->f64{
    let x=lex.get(a);let y=lex.get(b);let sparse=x.zip(y).map(|(x,y)|x.iter().map(|(w,v)|v*y.get(w).unwrap_or(&0.0)).sum::<f64>()).unwrap_or(0.0);
    let da=dense.get(a).and_then(Value::as_array);let db=dense.get(b).and_then(Value::as_array);
    if let (Some(a),Some(b))=(da,db){if a.len()!=b.len(){return sparse;}let semantic=a.iter().zip(b).map(|(x,y)|x.as_f64().unwrap_or(0.0)*y.as_f64().unwrap_or(0.0)).sum::<f64>();0.65*semantic+0.35*sparse}else{sparse}
}
fn sparse_centroid<'a>(ids:impl IntoIterator<Item=&'a String>,lex:&BTreeMap<String,BTreeMap<String,f64>>)->BTreeMap<String,f64>{let mut out:BTreeMap<String,f64>=BTreeMap::new();for id in ids{for (w,v) in lex.get(id).into_iter().flat_map(|m|m.iter()){*out.entry(w.clone()).or_default()+=*v;}}let norm=out.values().map(|v|v*v).sum::<f64>().sqrt();if norm>0.0{for v in out.values_mut(){*v/=norm;}}out}
fn dense_centroid<'a>(ids:impl IntoIterator<Item=&'a String>,dense:&Value)->Vec<f64>{let vals:Vec<Vec<f64>>=ids.into_iter().filter_map(|id|dense.get(id)?.as_array().map(|a|a.iter().map(|v|v.as_f64().unwrap_or(0.0)).collect())).collect();let Some(first)=vals.first() else{return vec![];};let mut out=vec![0.0;first.len()];for v in &vals{for (i,x) in v.iter().enumerate().take(out.len()){out[i]+=x/vals.len() as f64;}}let norm=out.iter().map(|x|x*x).sum::<f64>().sqrt();if norm>0.0{for x in &mut out{*x/=norm;}}out}
fn sparse_cos(a:&BTreeMap<String,f64>,b:&BTreeMap<String,f64>)->f64{a.iter().map(|(w,v)|v*b.get(w).unwrap_or(&0.0)).sum()}
fn dense_cos(a:&[f64],b:&[f64])->f64{if a.is_empty()||b.is_empty()||a.len()!=b.len(){return 0.0;}a.iter().zip(b).map(|(x,y)|x*y).sum()}
fn assign(assigned:&mut BTreeMap<String,Vec<String>>,note:&str,subject:&str){let values=assigned.entry(note.to_owned()).or_default();if !values.iter().any(|id|id==subject){values.push(subject.to_owned());}}
fn community_groups(mut ids:Vec<String>,lex:&BTreeMap<String,BTreeMap<String,f64>>,dense:&Value,minimum:usize)->Vec<Vec<String>>{ids.sort();let threshold=if dense.as_object().is_some_and(|m|!m.is_empty()){0.53}else{0.32};let mut groups:Vec<BTreeSet<String>>=ids.into_iter().map(|id|[id].into_iter().collect()).collect();loop{let mut best:Option<(f64,usize,usize)>=None;for i in 0..groups.len(){for j in i+1..groups.len(){let vals:Vec<f64>=groups[i].iter().flat_map(|a|groups[j].iter().map(|b|vector_similarity(a,b,lex,dense))).collect();let score=vals.iter().sum::<f64>()/vals.len() as f64;let weakest=vals.iter().copied().fold(f64::INFINITY,f64::min);if score>=threshold&&weakest>=threshold*0.65&&best.is_none_or(|(v,_,_)|score>v){best=Some((score,i,j));}}}let Some((_,i,j))=best else{break;};let right=groups.remove(j);groups[i].extend(right);}groups.into_iter().filter(|g|g.len()>=minimum).map(|g|g.into_iter().collect()).collect()}
fn label_cluster(ids:&[String],by:&BTreeMap<String,Value>,lex:&BTreeMap<String,BTreeMap<String,f64>>)->String{
    let mut scores:BTreeMap<String,f64>=BTreeMap::new();let mut phrases:BTreeMap<String,BTreeSet<String>>=BTreeMap::new();let mut spellings:BTreeMap<String,String>=BTreeMap::new();let word_re=Regex::new(r"[^\W\d_]{3,}").unwrap();
    for id in ids{if let Some(n)=by.get(id){for (w,v) in lex.get(id).into_iter().flat_map(|m|m.iter()){*scores.entry(w.clone()).or_default()+=*v;}let words:Vec<String>=word_re.find_iter(n["label"].as_str().unwrap_or("")).map(|m|m.as_str().to_owned()).collect();for w in &words{spellings.entry(key(w)).or_insert_with(||w.clone());}for size in [2,3]{if words.len()<size{continue;}for i in 0..=words.len()-size{let segment=&words[i..i+size];if segment.iter().all(|w|!tokens(&key(w)).is_empty()){let phrase=segment.join(" ");phrases.entry(phrase).or_default().insert(id.clone());}}}}}
    let best=scores.iter().max_by(|a,b|a.1.total_cmp(b.1).then_with(||a.0.cmp(b.0))).map(|(w,_)|w.clone()).unwrap_or_else(||"Unsorted knowledge".into());
    let options:Vec<(f64,String)>=phrases.iter().filter(|(_,members)|members.len()>=2).filter_map(|(phrase,_)|{let ts:Vec<_>=phrase.split_whitespace().collect();if !tokens(phrase).contains(&best){return None;}Some((ts.iter().map(|w|scores.get(&key(w)).copied().unwrap_or(0.0)).sum::<f64>()/ts.len() as f64*1.2,phrase.clone()))}).collect();
    if let Some((_,phrase))=options.into_iter().max_by(|a,b|a.0.total_cmp(&b.0).then_with(||a.1.cmp(&b.1))){return phrase;}
    let word=spellings.get(&best).cloned().unwrap_or(best);if word.chars().skip(1).any(char::is_uppercase){word}else{let mut c=word.chars();c.next().map(|x|x.to_uppercase().collect::<String>()+c.as_str()).unwrap_or_default()}
}

pub fn clean(text: &str) -> String {
    let mut s = text.to_owned();
    for pat in [r"(?ms)^```[^\n]*\n.*?^```[^\n]*$", r"`[^`]+`", r"(?mi)^(Source|Recorded|Reviewed|Confidence|Entity|Name|Parent|Project|Worlds|Topics|Tags|Category|Aliases|Status):[^\n]*", r"(?i)https?://\S+|(?:~|/)[\w./%-]+|\b[0-9a-f]{12,}\b"] {
        s = Regex::new(pat).expect("constant regex").replace_all(&s, " ").to_string();
    }
    s
}

pub fn features(note: &Value) -> (Vec<String>, String) {
    let title = note["label"].as_str().unwrap_or("");
    let body = clean(note["text"].as_str().unwrap_or(""));
    let title_words= tokens(title);
    let mut words=Vec::with_capacity(title_words.len()*3);
    for _ in 0..3 { words.extend(title_words.iter().cloned()); }
    words.extend(tokens(prefix(&body,5000)));
    let excerpt = format!("{}\n{}\n{}", title, prefix(&body,1100), suffix(&body,350));
    (words, excerpt)
}

/// Compatibility entry point used by `run.py --command 'nacre-helper brain-discovery'`.
pub fn api(operation: &str, request: &Value) -> Result<Value> {
    if operation != "organize" { return Err(err(format!("unknown operation: {operation}"))); }
    let args = request.get("args").and_then(Value::as_array).cloned().unwrap_or_default();
    let kwargs=request.get("kwargs").and_then(Value::as_object);
    let notes = args.first().cloned().or_else(||kwargs.and_then(|k|k.get("notes")).cloned()).unwrap_or_else(|| json!([]));
    let dense = args.get(1).cloned().or_else(||kwargs.and_then(|k|k.get("dense")).cloned()).unwrap_or_else(|| json!({}));
    let prior = args.get(2).cloned().or_else(||kwargs.and_then(|k|k.get("prior")).cloned()).unwrap_or_else(|| json!({}));
    let overrides = args.get(3).cloned().or_else(||kwargs.and_then(|k|k.get("overrides")).cloned()).unwrap_or_else(|| json!({}));
    organize(&notes, &dense, &prior, &overrides)
}

#[derive(Default)]
struct Subject { id:String, label:String, aliases:Vec<String>, origin:String, category:Value, path:Value, members:Map<String,Value>, topics:Vec<Value>, parent:Value }

pub fn organize(notes: &Value, _dense: &Value, _prior: &Value, overrides: &Value) -> Result<Value> {
    let mut copied = notes.clone();
    organize_mut(&mut copied, _dense, _prior, overrides)
}

/// Organize notes in place, preserving the Python helper's `memberships` side effect.
pub fn organize_mut(notes_value: &mut Value, dense_value: &Value, prior: &Value, overrides: &Value) -> Result<Value> {
    let notes = notes_value.as_array_mut().ok_or_else(|| err("notes must be an array"))?;
    let lex=vectors(notes);
    let by_id:BTreeMap<String,Value>=notes.iter().map(|n|(n["id"].as_str().unwrap_or("").to_owned(),n.clone())).collect();
    let mut subjects: BTreeMap<String, Subject> = BTreeMap::new();
    let mut order:Vec<String>=Vec::new();
    let mut assigned: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut aliases: BTreeMap<String,String> = BTreeMap::new();
    let mut topic_members: BTreeMap<(String,String),BTreeSet<String>> = BTreeMap::new();
    let mut topic_labels: BTreeMap<(String,String),String> = BTreeMap::new();
    let mut add = |label: &str, origin: &str, category: Value, path: Value| -> String {
        if let Some(id) = aliases.get(&key(label)) { return id.clone(); }
        let mut id = prior["discovered"].as_array().into_iter().flatten().find(|v|key(v["label"].as_str().unwrap_or(""))==key(label)).and_then(|v|v["id"].as_str()).map(str::to_owned).unwrap_or_else(||slug(label));
        if subjects.get(&id).is_some_and(|s|key(&s.label)!=key(label)){id.push_str(&format!("-{}",digest(label,6)));}
        if !subjects.contains_key(&id){order.push(id.clone());}
        let s = subjects.entry(id.clone()).or_insert_with(|| Subject { id:id.clone(), label:label.to_owned(), aliases:vec![], origin:origin.to_owned(), category, path, members:Map::new(), topics:vec![], parent:Value::Null });
        for name in [label.to_owned(), id.clone()] { if !aliases.contains_key(&key(&name)) { aliases.insert(key(&name), id.clone()); s.aliases.push(name); } }
        id
    };
    // Curated wiki declarations have priority and stable path-derived IDs.
    for n in notes.iter() {
        let path=n["path"].as_str().unwrap_or(""); let meta=&n["meta"];
        let kind=meta["entity"].as_str().unwrap_or("");
        let label=meta["name"].as_str().filter(|s| !s.is_empty()).unwrap_or(n["label"].as_str().unwrap_or(""));
        let eligible_kind=["world","project","person","interest","idea","device"].contains(&kind);
        let short_untagged=kind.is_empty() && n["label"].as_str().unwrap_or("").split_whitespace().count()<=5;
        if path.starts_with("wiki/") && (eligible_kind||short_untagged) && meta["parent"].as_str().unwrap_or("").is_empty() {
            let stem=path.rsplit('/').next().unwrap_or(path).strip_suffix(".md").unwrap_or(path);
            let sid=aliases.get(&key(label)).cloned().unwrap_or_else(||meta["project"].as_str().map(slug).unwrap_or_else(||slug(stem)));
            let mut names=vec![label.to_owned(),sid.clone()]; names.extend(items(&meta["aliases"])); names.push(stem.to_owned());
            let was_existing=subjects.contains_key(&sid);
            if !was_existing{order.push(sid.clone());}
            let sub=subjects.entry(sid.clone()).or_insert_with(|| Subject{id:sid.clone(),label:label.to_owned(),aliases:vec![],origin:"annotated".into(),category:meta.get("category").cloned().unwrap_or(Value::Null),path:json!(path),members:Map::new(),topics:vec![],parent:Value::Null});
            for name in names { if !aliases.contains_key(&key(&name)) {aliases.insert(key(&name),sid.clone());sub.aliases.push(name);} }
            aliases.insert(key(label),sid.clone());
            let nid=n["id"].as_str().unwrap_or(""); sub.members.insert(nid.into(),json!({"reason":"curated","score":1})); assign(&mut assigned,nid,&sid);
        }
    }
    for n in notes.iter() {
        let id=n["id"].as_str().unwrap_or("").to_owned();
        let path=n["path"].as_str().unwrap_or(""); let meta=&n["meta"];
        if !path.starts_with("raw/") {
            for label in items(&meta["worlds"]) {
                let sid=add(&label,"annotated",Value::Null,Value::Null);
                subjects.get_mut(&sid).unwrap().members.insert(id.clone(),json!({"reason":"annotation","score":1}));
                assign(&mut assigned,&id,&sid);
            }
        }
    }
    drop(add);
    // Projects are explicit declarations, except on topic and device pages.
    for n in notes.iter() {
        if n["path"].as_str().unwrap_or("").starts_with("raw/") { continue; }
        let project=n["meta"]["project"].as_str().unwrap_or("");
        if project.is_empty() || ["topic","device"].contains(&n["meta"]["entity"].as_str().unwrap_or("")) { continue; }
        let nid=n["id"].as_str().unwrap_or("");
        let resolved=aliases.get(&key(project)).cloned();
        let sid=if let Some(id)=resolved { id } else {
            let label=project.replace('-'," ").split_whitespace().map(|x| { let mut c=x.chars(); c.next().map(|f| f.to_uppercase().collect::<String>()+c.as_str()).unwrap_or_default() }).collect::<Vec<_>>().join(" ");
            let id=slug(project);
            if !subjects.contains_key(&id) { order.push(id.clone());subjects.insert(id.clone(),Subject{id:id.clone(),aliases:vec![label.clone(),id.clone()],label,origin:"annotated".into(),category:Value::Null,path:Value::Null,members:Map::new(),topics:vec![],parent:Value::Null}); aliases.insert(key(project),id.clone()); }
            id
        };
        subjects.get_mut(&sid).unwrap().members.insert(nid.into(),json!({"reason":"annotation","score":1}));
        assign(&mut assigned,nid,&sid);
    }
    // A manual grouping replaces all historic assignments for that note.
    if let Some(rows)=overrides.as_object() {
        for n in notes.iter_mut() {
            let nid=n["id"].as_str().unwrap_or("").to_owned();
            let Some(labels)=rows.get(&nid).and_then(Value::as_array) else { continue; };
            for s in subjects.values_mut() { s.members.remove(&nid); }
            let mut ids=Vec::new();
            for label in labels.iter().filter_map(Value::as_str) {
                let sid=if let Some(x)=aliases.get(&key(label)) { x.clone() } else {
                    let sid=slug(label); if !subjects.contains_key(&sid) { order.push(sid.clone());subjects.insert(sid.clone(),Subject{id:sid.clone(),label:label.to_owned(),aliases:vec![label.to_owned(),sid.clone()],origin:"manual".into(),category:Value::Null,path:Value::Null,members:Map::new(),topics:vec![],parent:Value::Null}); aliases.insert(key(label),sid.clone()); } sid
                };
                subjects.get_mut(&sid).unwrap().members.insert(nid.clone(),json!({"reason":"manual grouping","score":1})); ids.push(sid);
            }
            assigned.insert(nid,ids);
        }
    }
    // Curated reference links ground legacy notes; explicit grouping corrections win.
    let paths:BTreeMap<String,String>=notes.iter().map(|n|(n["path"].as_str().unwrap_or("").to_owned(),n["id"].as_str().unwrap_or("").to_owned())).collect();
    let curated:Vec<(String,String)>=order.iter().filter_map(|id|subjects.get(id)).filter_map(|s|s.path.as_str().map(|p|(s.id.clone(),p.to_owned()))).collect();
    for (sid,path) in curated {if let Some(page_id)=paths.get(&path){if let Some(page)=by_id.get(page_id){for target in page["references"].as_array().into_iter().flatten().filter_map(Value::as_str){if overrides.get(target).is_some(){continue;}subjects.get_mut(&sid).unwrap().members.insert(target.into(),json!({"reason":"curated reference","score":1}));assign(&mut assigned,target,&sid);}}}}
    for n in notes.iter(){let nid=n["id"].as_str().unwrap_or("");let path=n["path"].as_str().unwrap_or("");let entity=n["meta"]["entity"].as_str().unwrap_or("");if path.starts_with("wiki/")&&["topic","device"].contains(&entity){let parent=n["meta"]["parent"].as_str().filter(|s|!s.is_empty()).or(n["meta"]["project"].as_str()).unwrap_or("");if let Some(sid)=aliases.get(&key(parent)).cloned(){let stem=path.rsplit('/').next().unwrap_or(path).strip_suffix(".md").unwrap_or(path);let label=n["meta"]["name"].as_str().filter(|s|!s.is_empty()).unwrap_or(n["label"].as_str().unwrap_or(""));let topic=json!({"id":format!("{sid}:entity-{}",slug(stem)),"label":label,"parent":sid,"members":[nid],"origin":"annotated","category":n["meta"].get("category").cloned().unwrap_or(Value::Null)});subjects.get_mut(&sid).unwrap().topics.push(topic);if overrides.get(nid).is_none(){subjects.get_mut(&sid).unwrap().members.insert(nid.into(),json!({"reason":"curated topic","score":1}));assign(&mut assigned,nid,&sid);}}}}
    // Named titles and introductions may attach to curated identities, with ambiguous prefixes excluded.
    let mut names:Vec<(String,String)>=order.iter().filter_map(|id|subjects.get(id)).flat_map(|s|s.aliases.iter().filter(|name|name.chars().count()>=4).map(|name|(name.clone(),s.id.clone()))).collect();
    names.sort_by(|a,b|b.0.chars().count().cmp(&a.0.chars().count()));
    for n in notes.iter(){let nid=n["id"].as_str().unwrap_or("");if assigned.get(nid).is_some_and(|v|!v.is_empty()){continue;}let title=key(n["label"].as_str().unwrap_or(""));let mut found:Vec<(String,String)>=Vec::new();for (name,sid) in &names{let k=key(name);let ambiguous=subjects.values().any(|other|other.id!=*sid&&key(&other.label).starts_with(&(k.clone()+" ")));if ambiguous&&title!=k{continue;}let pat=format!(r"(?i)(^|[^\w]){}([^\w]|$)",regex::escape(&k));if Regex::new(&pat).unwrap().is_match(&title)&&!found.iter().any(|(x,_)|key(x).contains(&k)){found.push((name.clone(),sid.clone()));}}
        for (_,sid) in found.into_iter().take(2){subjects.get_mut(&sid).unwrap().members.insert(nid.into(),json!({"reason":"named title","score":1}));assign(&mut assigned,nid,&sid);}}
    for n in notes.iter(){let nid=n["id"].as_str().unwrap_or("");if assigned.get(nid).is_some_and(|v|!v.is_empty()){continue;}let intro=key(&clean(n["text"].as_str().unwrap_or("")).chars().take(900).collect::<String>());for (name,sid) in &names{let k=key(name);if subjects.values().any(|other|other.id!=*sid&&key(&other.label).starts_with(&(k.clone()+" "))){continue;}let pat=format!(r"(?i)(^|[^\w]){}([^\w]|$)",regex::escape(&k));let Some(m)=Regex::new(&pat).unwrap().find(&intro) else{continue;};let before=&intro[..m.start()];if Regex::new(r"\b(unrelated|unchanged|preserve|without|no|not|example|fixture)\b").unwrap().is_match(before){continue;}subjects.get_mut(sid).unwrap().members.insert(nid.into(),json!({"reason":"named evidence","score":1}));assign(&mut assigned,nid,sid);if assigned[nid].len()>=2{break;}}}
    // Repeated, specific names become subjects only with multiple independent, coherent notes.
    let camel=Regex::new(r"\b(?:[A-Z][a-z]+[A-Z][A-Za-z]*|[A-Z]{4,})\b").unwrap();
    let mut named:BTreeMap<String,BTreeSet<String>>=BTreeMap::new();
    for n in notes.iter(){let nid=n["id"].as_str().unwrap_or("");if assigned.get(nid).is_some_and(|v|!v.is_empty()){continue;}for m in camel.find_iter(n["label"].as_str().unwrap_or("")){let name=m.as_str().to_owned();if !aliases.contains_key(&key(&name)){named.entry(name).or_default().insert(nid.into());}}}
    let mut discovered=Vec::new();
    let mut named:Vec<_>=named.into_iter().collect();named.sort_by(|a,b|b.1.len().cmp(&a.1.len()).then_with(||a.0.cmp(&b.0)));
    for (label,ids) in named {
        if ids.len()<3 {continue;}
        let rows:Vec<_>=ids.iter().filter_map(|i|by_id.get(i)).collect();
        let hashes:BTreeSet<_>=rows.iter().map(|n|digest(&format!("{}\n{}",n["label"].as_str().unwrap_or(""),clean(n["text"].as_str().unwrap_or(""))).to_lowercase(),64)).collect();
        if hashes.len()<3 {continue;}
        let mut sims=Vec::new();for i in 0..rows.len(){for j in i+1..rows.len(){sims.push(vector_similarity(rows[i]["id"].as_str().unwrap_or(""),rows[j]["id"].as_str().unwrap_or(""),&lex,dense_value));}}
        let minimum=if dense_value.as_object().is_some_and(|m|!m.is_empty()){0.35}else{0.18};
        if sims.iter().sum::<f64>()/sims.len().max(1) as f64 < minimum {continue;}
        let prior_id=prior["discovered"].as_array().into_iter().flatten().find(|v|key(v["label"].as_str().unwrap_or(""))==key(&label)).and_then(|v|v["id"].as_str()).map(str::to_owned);
        let sid=prior_id.unwrap_or_else(||format!("discovered-{}",slug(&label)));
        if !subjects.contains_key(&sid){order.push(sid.clone());subjects.insert(sid.clone(),Subject{id:sid.clone(),label:label.clone(),aliases:vec![label.clone(),sid.clone()],origin:"discovered".into(),category:Value::Null,path:Value::Null,members:Map::new(),topics:vec![],parent:Value::Null});aliases.insert(key(&label),sid.clone());}
        let mut members=Vec::new();
        for nid in ids {if assigned.get(&nid).is_some_and(|v|!v.is_empty()){continue;}subjects.get_mut(&sid).unwrap().members.insert(nid.clone(),json!({"reason":"repeated named subject","score":0.75}));assign(&mut assigned,&nid,&sid);members.push(nid);}
        discovered.push(json!({"id":sid,"label":label,"members":members}));
    }
    // Learn a small lexical profile from grounded members and assign a clear best match.
    let profiles:BTreeMap<String,(BTreeMap<String,f64>,Vec<f64>)>=subjects.values().filter(|s|!s.members.is_empty()).map(|s|{let ids=s.members.keys().collect::<Vec<_>>();(s.id.clone(),(sparse_centroid(ids.iter().copied(),&lex),dense_centroid(ids.iter().copied(),dense_value)))}).collect();
    for n in notes.iter() {
        let nid=n["id"].as_str().unwrap_or(""); if assigned.get(nid).is_some_and(|v|!v.is_empty()){continue;}
        let mut scores:Vec<(f64,String)>=profiles.iter().map(|(sid,(sparse,semantic))|{let sparse_score=lex.get(nid).map(|v|sparse_cos(v,sparse)).unwrap_or(0.0);let score=if dense_value.get(nid).is_some()&&!semantic.is_empty(){let vector:Vec<f64>=dense_value[nid].as_array().into_iter().flatten().map(|v|v.as_f64().unwrap_or(0.0)).collect();0.65*dense_cos(&vector,semantic)+0.35*sparse_score}else{sparse_score};(score,sid.clone())}).collect();
        scores.sort_by(|a,b|b.0.total_cmp(&a.0).then_with(||b.1.cmp(&a.1)));
        if let Some((score,sid))=scores.first(){
            let minimum=if dense_value.as_object().is_some_and(|m|!m.is_empty()){0.47}else{0.24};
            if *score>=minimum && !(scores.len()>1 && scores[1].0>=score*0.91 && score-scores[1].0<0.055) {
                subjects.get_mut(sid).unwrap().members.insert(nid.into(),json!({"reason":"learned similarity","score":(score*1000.0).round()/1000.0})); assign(&mut assigned,nid,sid);
            }
        }
    }
    // Average-link communities catch coherent evidence without requiring a named entity.
    let mut residual:Vec<String>=notes.iter().filter(|n|!n["path"].as_str().unwrap_or("").starts_with("raw/") && assigned.get(n["id"].as_str().unwrap_or("")).is_none_or(Vec::is_empty)).map(|n|n["id"].as_str().unwrap_or("").to_owned()).collect();
    let title_name=Regex::new(r"\b[A-Z][A-Za-z]{3,}\b").unwrap();let mut named_residual:BTreeMap<String,BTreeSet<String>>=BTreeMap::new();
    for id in &residual{if let Some(n)=by_id.get(id){for m in title_name.find_iter(n["label"].as_str().unwrap_or("")){let label=m.as_str();if !tokens(&key(label)).is_empty(){named_residual.entry(label.to_owned()).or_default().insert(id.clone());}}}}
    let mut named_residual:Vec<_>=named_residual.into_iter().collect();named_residual.sort_by(|a,b|b.1.len().cmp(&a.1.len()).then_with(||a.0.cmp(&b.0)));
    for (raw_label,members) in named_residual{if members.len()<3{continue;}let values:Vec<_>=members.iter().collect();let mut pairs=Vec::new();for i in 0..values.len(){for j in i+1..values.len(){pairs.push(vector_similarity(values[i],values[j],&lex,dense_value));}}let min_sim=if dense_value.as_object().is_some_and(|m|!m.is_empty()){0.35}else{0.18};if (pairs.iter().sum::<f64>()/pairs.len().max(1) as f64)<min_sim{continue;}let group:Vec<String>=members.into_iter().filter(|id|assigned.get(id).is_none_or(Vec::is_empty)).collect();let rows:Vec<_>=group.iter().filter_map(|id|by_id.get(id)).collect();let evidence:BTreeSet<_>=rows.iter().map(|n|digest(&format!("{}\n{}",n["label"].as_str().unwrap_or(""),clean(n["text"].as_str().unwrap_or("")).to_lowercase()),64)).collect();if group.len()<3||evidence.len()<3{continue;}let label=if raw_label.chars().all(char::is_uppercase)||raw_label.chars().skip(1).any(char::is_uppercase){raw_label.clone()}else{label_cluster(&group,&by_id,&lex)};let mut chars=label.chars();let label=chars.next().map(|x|x.to_uppercase().collect::<String>()+chars.as_str()).unwrap_or_default();let group_vec=sparse_centroid(group.iter(),&lex);let group_dense=dense_centroid(group.iter(),dense_value);let mut related:Vec<(f64,String)>=profiles.iter().map(|(sid,(sparse,semantic))|{let ss=sparse_cos(&group_vec,sparse);let score=if dense_value.as_object().is_some_and(|m|!m.is_empty())&&!semantic.is_empty(){0.65*dense_cos(&group_dense,semantic)+0.35*ss}else{ss};(score,sid.clone())}).collect();related.sort_by(|a,b|b.0.total_cmp(&a.0).then_with(||b.1.cmp(&a.1)));if let Some((score,parent))=related.first(){if *score>=if dense_value.as_object().is_some_and(|m|!m.is_empty()){0.4}else{0.2}{for id in &group{subjects.get_mut(parent).unwrap().members.insert(id.clone(),json!({"reason":"suggested named topic","score":(score*1000.0).round()/1000.0}));assign(&mut assigned,id,parent);}subjects.get_mut(parent).unwrap().topics.push(json!({"id":format!("{parent}:learned-{}",slug(&label)),"label":label,"parent":parent,"members":group,"origin":"suggested","category":null}));continue;}}
        let sid=format!("discovered-{}",slug(&label));if !subjects.contains_key(&sid){order.push(sid.clone());subjects.insert(sid.clone(),Subject{id:sid.clone(),label:label.clone(),aliases:vec![label.clone(),sid.clone()],origin:"discovered".into(),category:Value::Null,path:Value::Null,members:Map::new(),topics:vec![],parent:Value::Null});aliases.insert(key(&label),sid.clone());}for id in &group{subjects.get_mut(&sid).unwrap().members.insert(id.clone(),json!({"reason":"repeated named subject","score":0.7}));assign(&mut assigned,id,&sid);}discovered.push(json!({"id":sid,"label":label,"members":group}));
    }
    residual=notes.iter().filter(|n|!n["path"].as_str().unwrap_or("").starts_with("raw/") && assigned.get(n["id"].as_str().unwrap_or("")).is_none_or(Vec::is_empty)).map(|n|n["id"].as_str().unwrap_or("").to_owned()).collect();
    let threshold=if dense_value.as_object().is_some_and(|m|!m.is_empty()){0.53}else{0.32};
    let mut groups:Vec<BTreeSet<String>>=residual.iter().map(|id|[id.clone()].into_iter().collect()).collect();
    loop {let mut best:Option<(f64,usize,usize)>=None;for i in 0..groups.len(){for j in i+1..groups.len(){let vals:Vec<f64>=groups[i].iter().flat_map(|a|groups[j].iter().map(|b|vector_similarity(a,b,&lex,dense_value))).collect();let score=vals.iter().sum::<f64>()/vals.len().max(1) as f64;if score>=threshold&&vals.iter().copied().fold(f64::INFINITY,f64::min)>=threshold*0.65&&best.as_ref().is_none_or(|v|score>v.0){best=Some((score,i,j));}}}let Some((_,i,j))=best else{break;};let merge=groups.remove(j);groups[i].extend(merge);}
    for group in groups.into_iter().filter(|g|g.len()>=3){
        let rows:Vec<_>=group.iter().filter_map(|id|by_id.get(id)).collect();
        let evidence:BTreeSet<_>=rows.iter().map(|n|digest(&format!("{}\n{}",n["label"].as_str().unwrap_or(""),clean(n["text"].as_str().unwrap_or("")).to_lowercase()),64)).collect(); if evidence.len()<3{continue;}
        let mut label=label_cluster(&group.iter().cloned().collect::<Vec<_>>(),&by_id,&lex);let mut chars=label.chars();label=chars.next().map(|x|x.to_uppercase().collect::<String>()+chars.as_str()).unwrap_or_default();
        if evidence.len()<5 {
            let group_vec=sparse_centroid(group.iter(),&lex);let group_dense=dense_centroid(group.iter(),dense_value);
            let mut related:Vec<(f64,String)>=profiles.iter().map(|(sid,(sparse,semantic))|{let sparse_score=sparse_cos(&group_vec,sparse);let score=if dense_value.as_object().is_some_and(|m|!m.is_empty())&&!semantic.is_empty(){0.65*dense_cos(&group_dense,semantic)+0.35*sparse_score}else{sparse_score};(score,sid.clone())}).collect();
            related.sort_by(|a,b|b.0.total_cmp(&a.0).then_with(||b.1.cmp(&a.1)));
            if let Some((score,parent))=related.first(){if *score>=if dense_value.as_object().is_some_and(|m|!m.is_empty()){0.4}else{0.2}{let members:Vec<_>=group.iter().cloned().collect();for nid in &members{subjects.get_mut(parent).unwrap().members.insert(nid.clone(),json!({"reason":"suggested community","score":(score*1000.0).round()/1000.0}));assign(&mut assigned,nid,parent);}subjects.get_mut(parent).unwrap().topics.push(json!({"id":format!("{parent}:learned-{}",slug(&label)),"label":label,"parent":parent,"members":members,"origin":"suggested","category":null}));continue;}}
            continue;
        }
        let mut old:Option<(String,String,f64)>=None;
        for candidate in prior["discovered"].as_array().into_iter().flatten(){let old_members:BTreeSet<String>=candidate["members"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect();let overlap=group.intersection(&old_members).count() as f64/group.len().min(old_members.len()).max(1) as f64;if overlap>0.55&&old.as_ref().is_none_or(|v|overlap>v.2){old=Some((candidate["id"].as_str().unwrap_or("").to_owned(),candidate["label"].as_str().unwrap_or("").to_owned(),overlap));}}
        let (sid,label)=old.map(|(id,label,_)|(id,label)).unwrap_or_else(||(format!("discovered-{}",slug(&label)),label));
        if !subjects.contains_key(&sid){order.push(sid.clone());subjects.insert(sid.clone(),Subject{id:sid.clone(),label:label.clone(),aliases:vec![label.clone(),sid.clone()],origin:"discovered".into(),category:Value::Null,path:Value::Null,members:Map::new(),topics:vec![],parent:Value::Null});aliases.insert(key(&label),sid.clone());}
        for nid in &group{subjects.get_mut(&sid).unwrap().members.insert(nid.clone(),json!({"reason":"discovered community","score":0.65}));assign(&mut assigned,nid,&sid);}
        discovered.push(json!({"id":sid,"label":label,"members":group}));
    }
    // Topic annotations are attached only to notes with that exact topic declaration.
    for n in notes.iter() {
        let nid=n["id"].as_str().unwrap_or("");
        for sid in assigned.get(nid).cloned().unwrap_or_default() {
            for label in items(&n["meta"].get("topics").or_else(||n["meta"].get("tags")).cloned().unwrap_or(Value::Null)) {
                let k=(sid.clone(),key(&label)); topic_members.entry(k.clone()).or_default().insert(nid.to_owned()); topic_labels.insert(k,label);
            }
        }
    }
    for ((parent,k),members) in topic_members {
        if let Some(s)=subjects.get_mut(&parent) {
            let label=topic_labels.get(&(parent.clone(),k.clone())).cloned().unwrap_or(k);
            s.topics.push(json!({"id":format!("{parent}:tag-{}",slug(&label)),"label":label,"parent":parent,"members":members,"origin":"annotated","category":null}));
        }
    }
    // Learn uncovered topic clusters inside each grounded subject.
    let subject_ids=order.clone();
    for sid in &subject_ids {
        let Some(subject)=subjects.get(sid) else{continue;};
        let covered:BTreeSet<String>=subject.topics.iter().flat_map(|t|t["members"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_owned)).collect();
        let ids:Vec<String>=subject.members.keys().filter(|id|!covered.contains(*id)).cloned().collect();
        for group in community_groups(ids,&lex,dense_value,3){let label=label_cluster(&group,&by_id,&lex);if key(&label)==key(&subject.label){continue;}subjects.get_mut(sid).unwrap().topics.push(json!({"id":format!("{sid}:learned-{}",slug(&label)),"label":label,"parent":sid,"members":group,"origin":"suggested","category":null}));}
    }
    // Merge duplicate topic representations, preferring annotated names and unioning members.
    let mut topic_order:Vec<(String,String)>=Vec::new();let mut merged:BTreeMap<(String,String),Value>=BTreeMap::new();
    for sid in &subject_ids {if let Some(s)=subjects.get(sid){for topic in &s.topics{let ident=(sid.clone(),key(topic["label"].as_str().unwrap_or("")));if let Some(old)=merged.get_mut(&ident){let mut members:BTreeSet<String>=old["members"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect();members.extend(topic["members"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_owned));if topic["origin"]=="annotated"&&old["origin"]!="annotated"{*old=topic.clone();}old["members"]=json!(members);}else{topic_order.push(ident.clone());merged.insert(ident,topic.clone());}}}}
    for sid in &subject_ids {if let Some(s)=subjects.get_mut(sid){s.topics.clear();}}
    for ident in topic_order {if let Some(mut topic)=merged.remove(&ident){let parent=ident.0.clone();let label=topic["label"].as_str().unwrap_or("").to_owned();let mut members:BTreeSet<String>=topic["members"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect();for n in notes.iter(){let nid=n["id"].as_str().unwrap_or("");if assigned.get(nid).is_some_and(|v|v.contains(&parent))&&!members.contains(nid){let pat=format!(r"(?i)(^|[^\w]){}([^\w]|$)",regex::escape(&key(&label)));if Regex::new(&pat).unwrap().is_match(&key(n["label"].as_str().unwrap_or(""))){members.insert(nid.to_owned());}}}topic["members"]=json!(members);subjects.get_mut(&parent).unwrap().topics.push(topic);}}
    // Promote only topics with distinct recorded evidence across days and substantial parent weight.
    let mut promoted=Vec::new();
    let topic_pairs:Vec<_>=order.iter().filter_map(|id|subjects.get(id)).flat_map(|s|s.topics.iter().map(move|t|(s.id.clone(),t.clone()))).collect();
    for (parent,topic) in topic_pairs {
        let label=topic["label"].as_str().unwrap_or("").to_owned();
        if aliases.contains_key(&key(&label)){continue;}
        let members=topic["members"].as_array().cloned().unwrap_or_default();
        let rows:Vec<_>=members.iter().filter_map(|id|by_id.get(id.as_str()?)).collect();
        let support:BTreeSet<_>=rows.iter().map(|n|digest(&clean(n["text"].as_str().unwrap_or("")).to_lowercase(),64)).collect();
        let days:BTreeSet<_>=rows.iter().filter_map(|n|n["date"].as_str()).collect();
        let weight:f64=rows.iter().filter(|n|n.get("recorded").is_some()).map(|n|n["activity"].as_f64().unwrap_or(0.0)).sum();
        let total:f64=subjects.get(&parent).into_iter().flat_map(|s|s.members.keys()).filter_map(|id|by_id.get(id)).filter(|n|n.get("recorded").is_some()).map(|n|n["activity"].as_f64().unwrap_or(0.0)).sum();
        let prior_ids=prior["promoted"].as_array().cloned().unwrap_or_default();
        let sid=format!("subject-{}",slug(&label));
        if !((support.len()>=5 && days.len()>=2 && weight/total.max(0.001)>=0.35) || (prior_ids.iter().any(|v|v.as_str()==Some(&sid)) && support.len()>=3)){continue;}
        if !subjects.contains_key(&sid){order.push(sid.clone());subjects.insert(sid.clone(),Subject{id:sid.clone(),label:label.clone(),aliases:vec![label.clone(),sid.clone()],origin:"promoted".into(),category:Value::Null,path:Value::Null,members:Map::new(),topics:vec![],parent:json!(parent)});aliases.insert(key(&label),sid.clone());}
        for row in rows {let nid=row["id"].as_str().unwrap_or("");subjects.get_mut(&sid).unwrap().members.insert(nid.into(),json!({"reason":"promoted topic","score":1}));assign(&mut assigned,nid,&sid);}
        promoted.push(sid);
    }
    let mut review=0;
    for n in notes.iter() {
        let nid=n["id"].as_str().unwrap_or("");
        if assigned.get(nid).is_none_or(Vec::is_empty) { review+=1; }
    }
    if review>0 {
        if !subjects.contains_key("review"){order.push("review".into());}
        let s=subjects.entry("review".into()).or_insert_with(||Subject{id:"review".into(),label:"Needs grouping".into(),aliases:vec!["Needs grouping".into(),"review".into()],origin:"review".into(),category:Value::Null,path:Value::Null,members:Map::new(),topics:vec![],parent:Value::Null});
        for n in notes.iter() { let nid=n["id"].as_str().unwrap_or(""); if assigned.get(nid).is_none_or(Vec::is_empty) { s.members.insert(nid.into(),json!({"reason":"unresolved","score":0})); } }
    }
    for n in notes.iter_mut() {
        let nid=n["id"].as_str().unwrap_or("");
        let memberships=assigned.get(nid).cloned().unwrap_or_default().into_iter().filter_map(|sid| subjects.get(&sid).and_then(|s|s.members.get(nid)).cloned().map(|reason|json!({"subject":sid,"reason":reason["reason"],"score":reason["score"]}))).collect::<Vec<_>>();
        if let Some(obj)=n.as_object_mut() { obj.insert("memberships".into(),json!(memberships)); }
    }
    let result:Vec<Value>=order.into_iter().filter_map(|id|subjects.remove(&id)).filter(|s|!s.members.is_empty()).map(|s|json!({"id":s.id,"label":s.label,"aliases":s.aliases,"origin":s.origin,"category":s.category,"path":s.path,"members":s.members,"topics":s.topics,"parent":s.parent})).collect();
    let method=if dense_value.as_object().is_some_and(|m|!m.is_empty()) {"semantic + lexical"} else {"lexical fallback"};
    Ok(json!([result,{"discovered":discovered,"promoted":promoted,"review":review,"method":method}]))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn note(i:u32,title:&str,body:&str,meta:Value,date:&str)->Value{json!({"id":format!("note:{i}"),"label":title,"text":format!("# {title}\n{body}"),"path":format!("inbox/{i}.md"),"meta":meta,"date":date,"recorded":format!("{date}T12:00:00+00:00"),"activity":1,"revision":i.to_string(),"references":[]})}
    fn run(notes:&mut Value,dense:&Value,prior:&Value,overrides:&Value)->Value{organize_mut(notes,dense,prior,overrides).unwrap()}
    #[test] fn unknown_worlds_and_topics(){let mut ns=json!([note(1,"A training result","Grip practice",json!({"worlds":"Climbing","topics":"Technique"}),"2026-10-01"),note(2,"Circuit result","Measure supply ripple",json!({"worlds":"Electronics","topics":"Measurements"}),"2026-10-01")]);let out=run(&mut ns,&json!({}),&json!({}),&json!({}));let subjects=out[0].as_array().unwrap();assert_eq!(subjects.iter().map(|s|s["label"].as_str().unwrap()).collect::<BTreeSet<_>>(),["Climbing","Electronics"].into_iter().collect());assert_eq!(subjects[0]["topics"][0]["label"],"Technique");}
    #[test] fn manual_correction_and_shared_annotations(){let mut n=json!([note(1,"A training outcome","A grip technique",json!({"worlds":"Wrong label"}),"2026-10-01")]);let before=n[0]["text"].clone();run(&mut n,&json!({}),&json!({}),&json!({"note:1":["Climbing"]}));assert_eq!(n[0]["memberships"][0]["subject"],"climbing");assert_eq!(n[0]["text"],before);let mut shared=json!([note(2,"An interdisciplinary idea","A wearable for training",json!({"worlds":"Climbing, Electronics"}),"2026-10-01")]);shared[0]["confidence"]=json!("unverified");run(&mut shared,&json!({}),&json!({}),&json!({}));assert_eq!(shared[0]["memberships"].as_array().unwrap().len(),2);assert_eq!(shared[0]["confidence"],"unverified");}
    #[test] fn duplicate_templates_stay_review(){let mut ns=json!([note(1,"GardenSense baseline","Identical sensor baseline",json!({}),"2026-10-01"),note(2,"GardenSense baseline","Identical sensor baseline",json!({}),"2026-10-01"),note(3,"GardenSense baseline","Identical sensor baseline",json!({}),"2026-10-01"),note(4,"GardenSense baseline","Identical sensor baseline",json!({}),"2026-10-01")]);let out=run(&mut ns,&json!({}),&json!({}),&json!({}));assert!(out[0].as_array().unwrap().iter().all(|s|s["origin"]=="review"));}
    #[test] fn repeated_named_project_discovers_and_keeps_identity(){let mut ns=json!([note(1,"GardenSense moisture readings","GardenSense soil moisture irrigation sensors",json!({}),"2026-10-01"),note(2,"GardenSense watering experiment","GardenSense soil moisture irrigation timing",json!({}),"2026-10-01"),note(3,"GardenSense sensor calibration","GardenSense soil moisture irrigation calibration",json!({}),"2026-10-01")]);let out=run(&mut ns,&json!({}),&json!({}),&json!({}));let found=out[0].as_array().unwrap().iter().find(|s|s["origin"]=="discovered").unwrap();assert!(found["label"].as_str().unwrap().contains("GardenSense"));let prior=out[1].clone();let mut reversed=json!([ns[2].clone(),ns[1].clone(),ns[0].clone()]);let again=run(&mut reversed,&json!({}),&prior,&json!({}));assert_eq!(again[0].as_array().unwrap().iter().find(|s|s["origin"]=="discovered").unwrap()["id"],found["id"]);}
    #[test] fn named_project_is_not_swallowed_by_related_world(){let mut ns=json!([note(0,"Sensor electronics","Soil moisture irrigation sensors",json!({"worlds":"Electronics"}),"2026-10-01"),note(1,"GardenSense moisture readings","GardenSense soil moisture irrigation sensors",json!({}),"2026-10-01"),note(2,"GardenSense watering experiment","GardenSense soil moisture irrigation timing",json!({}),"2026-10-01"),note(3,"GardenSense sensor calibration","GardenSense soil moisture irrigation calibration",json!({}),"2026-10-01")]);let dense=json!({"note:0":[1,0],"note:1":[1,0],"note:2":[1,0],"note:3":[1,0]});let out=run(&mut ns,&dense,&json!({}),&json!({}));assert!(out[0].as_array().unwrap().iter().any(|s|s["label"]=="GardenSense"));assert!(ns[1]["memberships"].as_array().unwrap().iter().any(|m|m["subject"]=="discovered-gardensense"));}
    #[test] fn annotation_wins_over_similarity(){let mut ns=json!([note(1,"Acme camera work","Camera firmware optics",json!({"worlds":"Acme"}),"2026-10-01"),note(2,"A camera hobby","Camera optics photographs",json!({"worlds":"Photography"}),"2026-10-01")]);run(&mut ns,&json!({}),&json!({}),&json!({}));assert_eq!(ns[1]["memberships"][0]["subject"],"photography");assert_eq!(ns[0]["memberships"][0]["subject"],"acme");}
    #[test] fn namespace_prefix_does_not_assign_personal_subject(){let mut ns=json!([note(1,"Ada","Personal climbing education",json!({"entity":"world","name":"Ada","project":"personal"}),"2026-10-01"),note(2,"Nacre","Desktop settings",json!({"entity":"world","name":"Nacre","project":"os"}),"2026-10-01"),note(3,"A desktop tweak","Nacre panel configuration",json!({}),"2026-10-01")]);ns[0]["path"]=json!("wiki/personal.md");ns[1]["path"]=json!("wiki/os.md");run(&mut ns,&json!({}),&json!({}),&json!({}));let subjects:BTreeSet<_>=ns[2]["memberships"].as_array().unwrap().iter().map(|m|m["subject"].as_str().unwrap()).collect();assert!(!subjects.contains("personal"));}
    #[test] fn one_mention_does_not_create_named_subject(){let mut ns=json!([note(1,"A telescope thought","Try astronomy",json!({}),"2026-10-01")]);let out=run(&mut ns,&json!({}),&json!({}),&json!({}));assert_eq!(out[0][0]["origin"],"review");}
    #[test] fn promoted_topic_identity_survives_parent_growth(){let mut ns:Vec<Value>=(0..6).map(|i|note(i,&format!("Climbing {i}"),&format!("Distinct grip result {i}"),json!({"worlds":"Personal","topics":"Climbing"}),if i%2==0{"2026-10-01"}else{"2026-10-02"})).collect();let mut value=Value::Array(ns.clone());let prior=run(&mut value,&json!({}),&json!({}),&json!({}))[1].clone();for i in 20..45{ns.push(note(i,"A different interest","Distinct unrelated interest",json!({"worlds":"Personal"}),"2026-10-03"));}let mut expanded=Value::Array(ns);let out=run(&mut expanded,&json!({}),&prior,&json!({}));assert!(out[1]["promoted"].as_array().unwrap().contains(&json!("subject-climbing")));assert!(out[0].as_array().unwrap().iter().any(|s|s["id"]=="subject-climbing"));}
    #[test] fn topic_promotes_only_with_diverse_evidence(){let mut ns=Value::Array((0..6).map(|i|note(i,&format!("Climbing {i}"),&format!("Distinct grip result {i}"),json!({"worlds":"Personal","topics":"Climbing"}),if i%2==0{"2026-10-01"}else{"2026-10-02"})).collect());let out=run(&mut ns,&json!({}),&json!({}),&json!({}));assert!(out[1]["promoted"].as_array().unwrap().contains(&json!("subject-climbing")));for n in ns.as_array_mut().unwrap(){n["date"]=json!("2026-10-01");}let out=run(&mut ns,&json!({}),&json!({}),&json!({}));assert_eq!(out[1]["promoted"],json!([]));}
}
