use crate::{
    context::{array, hash, yes, Context},
    model, text, Result,
};
use serde_json::{json, Value};

impl Context {
    pub fn localized(&self, input: &Value) -> Value {
        match input {
            Value::Array(rows) => json!(rows.iter().map(|v| self.localized(v)).collect::<Vec<_>>()),
            Value::Object(values) => {
                let mut result = input.clone();
                for key in ["name", "label", "description", "confirm"] {
                    if let Some(id) = values.get(&format!("{key}Id")).and_then(Value::as_str) {
                        result[key] = json!(self.tr(
                            id,
                            values.get(key).and_then(Value::as_str).unwrap_or(""),
                            json!({})
                        ));
                    }
                }
                result
            }
            _ => input.clone(),
        }
    }
    pub fn emoji_row(&self, entry: &Value) -> Value {
        let mut aliases = vec![entry["value"].clone()];
        aliases.extend(array(&entry["aliases"]).iter().cloned());
        let mut row = self.value_row(
            "actions.emoji",
            &hash(&text::field(entry, "value")),
            &text::field(entry, "name"),
            "copy",
            entry["value"].clone(),
            "mood",
            entry["category"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    self.tr(
                        "launcher.provider.copy-this-emoji",
                        "Copy this emoji",
                        json!({}),
                    )
                })
                .as_str(),
            json!({"emoji":entry["value"],"aliases":aliases}),
        );
        row["searchIndex"] = model::search_index(&row);
        row
    }
    pub(crate) fn emoji_search_matches(&self, query: &str) -> &[Value] {
        if !query.is_empty() && self.previews["emojiQuery"] == query {
            array(&self.previews["emojiMatches"])
        } else {
            &[]
        }
    }
    pub fn provider(&self, route: &str, query: &str) -> Result<Vec<Value>> {
        let Some(node) = self.node(route) else {
            return Ok(Vec::new());
        };
        let kind = node["provider"].as_str().unwrap_or("");
        let tr = |id, fallback| self.tr(id, fallback, json!({}));
        Ok(match kind{
            ""=>Vec::new(),"apps"=>self.apps(),
            "audio"=>array(&self.audio["outputs"]).iter().map(|item|{let id=text::literal(&item["id"]);let label=item["description"].as_str().filter(|s|!s.is_empty()).or_else(||item["nickname"].as_str().filter(|s|!s.is_empty())).unwrap_or_else(||item["name"].as_str().unwrap_or(""));self.value_row(route,&format!("output-{id}"),label,"audio-output",json!(id),"speaker",&tr("launcher.provider.audio-output","Audio output"),json!({"checked":item["id"]==self.audio["sink_id"],"disabled":!yes(&item["ready"])}))}).collect(),
            "wallpapers"=>array(&self.wallpapers["list"]).iter().map(|entry|{let checked=entry["path"]==self.wallpapers["current"];let mut extra=json!({"checked":checked,"status":if checked{tr("ui.menu-context.status.in-use","In use")}else{String::new()}});if let Some(preview)=entry.get("thumbnail").filter(|v|yes(v)).or_else(||entry.get("poster")){extra["preview"]=preview.clone();}self.value_row(route,&hash(&text::field(entry,"path")),&text::field(entry,"name"),"wallpaper",entry["path"].clone(),"wallpaper",&if yes(&entry["dynamic"]){tr("launcher.provider.wallpaper.animated","Animated wallpaper")}else{tr("launcher.provider.wallpaper.static","Wallpaper")},extra)}).collect(),
            "accents"=>array(&self.wallpapers["paletteOptions"]).iter().enumerate().map(|(index,entry)|{let checked=entry["accent"]==self.wallpapers["selectedAccent"];let name=entry["name"].as_str().filter(|s|!s.is_empty()).unwrap_or("");let label=if name.is_empty(){self.tr("launcher.provider.color","Color {number}",json!({"number":(index+1).to_string()}))}else{name.to_owned()};self.value_row(route,&format!("choice-{index}"),&label,"accent",entry["accent"].clone(),"palette",&tr("launcher.provider.use-this-color-family","Use this color family"),json!({"colors":entry.get("colors").cloned().unwrap_or(json!([])),"checked":checked,"status":if checked{tr("ui.menu-context.status.in-use","In use")}else{String::new()}}))}).collect(),
            "windows"=>array(&self.compositor["windows"]).iter().map(|client|{let title=client["title"].as_str().filter(|s|!s.is_empty()).unwrap_or_else(||client["wmClass"].as_str().unwrap_or(""));self.value_row(route,&text::literal(&client["windowId"]),title,"focus",client["windowId"].clone(),"window",&self.tr("launcher.provider.workspace","Workspace {workspace}",json!({"workspace":client["workspace"]["name"]})),json!({}))}).collect(),
            "wifi"=>array(&self.network["visibleNetworks"]).iter().map(|point|{let active=yes(&point["active"]);self.value_row(route,&text::field(point,"bssid").replace(':',""),point["ssid"].as_str().filter(|s|!s.is_empty()).map(str::to_owned).unwrap_or_else(||tr("launcher.provider.hidden-network","Hidden network")).as_str(),"settings",json!("network"),"wifi",&self.tr("launcher.provider.signal","{strength}% signal",json!({"strength":text::literal(&point["strength"])})),json!({"checked":point["active"],"status":if active{tr("ui.menu-context.status.connected","Connected")}else{String::new()}}))}).collect(),
            "bluetooth"=>array(&self.bluetooth["devices"]).iter().map(|device|self.value_row(route,&text::field(device,"address").replace(':',""),&text::field(device,"alias"),"settings",json!("bluetooth"),"bluetooth",&if yes(&device["paired"]){tr("launcher.provider.paired-device","Paired device")}else{tr("launcher.provider.nearby-device","Nearby device")},json!({"checked":device["connected"],"status":if yes(&device["connected"]){tr("ui.menu-context.status.connected","Connected")}else{String::new()}}))).collect(),
            "clipboard"=>array(&self.extras["clips"]).iter().map(|entry|{let title=entry["title"].as_str().filter(|s|!s.is_empty()).map(str::to_owned).unwrap_or_else(||tr("launcher.provider.clipboard-item","Clipboard item"));self.value_row(route,&text::literal(&entry["id"]),&title,"copy-clip",entry["id"].clone(),"content_paste",&tr("launcher.provider.copied-item","Copied item"),json!({"preview":if yes(&entry["image"]){text::field(entry,"thumbnail")}else{String::new()},"previewText":if yes(&entry["image"]){String::new()}else{text::field(entry,"title")},"clipboard":true,"pinned":entry["pinned"]==true,"checked":entry["pinned"]==true,"status":if yes(&entry["pinned"]){tr("launcher.workflows.pinned","Pinned")}else{String::new()}}))}).collect(),
            "keys"=>array(&self.extras["keys"]).iter().enumerate().map(|(index,entry)|self.value_row(route,&format!("binding-{index}"),entry["description"].as_str().filter(|s|!s.is_empty()).unwrap_or_else(||entry["key"].as_str().unwrap_or("")),"info",json!("shortcut"),"keyboard",&text::field(entry,"key"),json!({"shortcut":entry["key"],"disabled":false}))).collect(),
            "emoji"=>{if !query.is_empty(){self.emoji_search_matches(query).iter().map(|e|self.emoji_row(e)).collect()}else{let mut rows:Vec<_>=self.emoji_entries.as_slice().iter().map(|e|self.emoji_row(&self.localized(e))).collect();let favorites=array(&self.preferences["favorites"]);let recent=array(&self.preferences["recent"]);rows.sort_by_key(|row|{let id=json!(model::stable(row));(!favorites.contains(&id),recent.iter().position(|v|*v==id).unwrap_or(999))});rows}},
            "timers"=>array(&self.snapshot["reminders"]).iter().map(|entry|{let unit=text::field(entry,"unit");let due=entry["due"].as_f64().unwrap_or(0.0) as i64;let time=if let Some(offset)=self.utc_offset_seconds{chrono::DateTime::from_timestamp(due+i64::from(offset),0).map(|v|v.format("%H:%M").to_string()).unwrap_or_default()}else{chrono::DateTime::from_timestamp(due,0).map(|v|v.with_timezone(&chrono::Local).format("%H:%M").to_string()).unwrap_or_default()};self.value_row(route,unit.get(..unit.len().saturating_sub(6)).unwrap_or(""),&text::field(entry,"label"),"timer-cancel",entry["unit"].clone(),"timer",&tr("launcher.provider.cancel-this-reminder","Cancel this reminder"),json!({"status":self.tr("ui.menu-context.status.until-value0","Until {value0}",json!({"value0":time}))}))}).collect(),
            "webapps"=>array(&self.snapshot["webapps"]).iter().map(|entry|self.value_row(route,&text::field(entry,"id"),&text::field(entry,"label"),"webapp",entry["id"].clone(),"public",&text::field(entry,"url"),json!({"webapp":true}))).collect(),
            "installed"=>array(&self.snapshot["packages"]).iter().filter(|e|query.is_empty()||model::normalized(&text::field(e,"name")).contains(&model::normalized(query))).take(100).map(|entry|self.value_row(route,&hash(&text::field(entry,"name")),&text::field(entry,"name"),"remove-package",entry["name"].clone(),"inventory_2",&text::field(entry,"version"),json!({"package":entry["name"],"status":tr("ui.menu-context.status.installed","Installed")}))).collect(),
            "packages"=>if self.package_query==model::scope(query)["text"].as_str().unwrap_or(""){self.packages.iter().map(|entry|{let installed=array(&self.snapshot["packages"]).iter().any(|p|p["name"]==entry["name"]);let mut extra=json!({"package":entry["name"],"checked":installed,"disabled":installed});if installed{extra["status"]=json!(tr("ui.menu-context.status.installed","Installed"));}else if let Some(v)=entry.get("repository"){extra["status"]=v.clone();}self.value_row(route,&hash(&text::field(entry,"name")),&text::field(entry,"name"),"install-package",entry["name"].clone(),"download",&text::field(entry,"description"),extra)}).collect()}else{Vec::new()},
            other=>return self.other_provider(other,route,query),
        })
    }
    pub fn other_provider(&self, kind: &str, route: &str, _query: &str) -> Result<Vec<Value>> {
        Ok(match kind {
            "fonts" => self.font_rows(route == "style.mono-fonts"),
            "software" => self.software_rows(route),
            "software-ai" => self
                .software_rows(route)
                .into_iter()
                .filter(|r| r["category"] == "ai")
                .collect(),
            "software-themes" => self.theme_rows(),
            "software-extensions" => self.extension_rows(),
            "environments" => self.environment_rows(),
            "system-speed" => self.system_rows("speed"),
            "system-hardware" => self.system_rows("hardware"),
            "system-setup" => self.system_rows("setup"),
            "health-fixits" => self.system_rows("health"),
            "tui-apps" => self.tui_rows(),
            _ => return Err(format!("Unknown provider {kind}").into()),
        })
    }
}
