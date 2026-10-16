use crate::{
    context::{array, hash, yes, Context},
    model, text,
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

impl Context {
    fn app_index(&self) -> (BTreeSet<String>, BTreeSet<String>, BTreeMap<String, String>) {
        let mut names = BTreeSet::new();
        let mut ids = BTreeSet::new();
        let mut visible = BTreeMap::new();
        for app in &self.all_apps {
            let id = text::field(app, "id");
            let id = id.strip_suffix(".desktop").unwrap_or(&id);
            names.insert(model::normalized(&text::field(app, "name")));
            ids.insert(model::normalized(id));
            ids.insert(model::normalized(id.rsplit('.').next().unwrap_or(id)));
        }
        for app in &self.visible_apps {
            let id = text::field(app, "id");
            let short = id.strip_suffix(".desktop").unwrap_or(&id);
            for key in [
                text::field(app, "name"),
                short.to_owned(),
                short.rsplit('.').next().unwrap_or(short).to_owned(),
            ] {
                visible.insert(model::normalized(&key), id.clone());
            }
        }
        (names, ids, visible)
    }
    pub fn software_rows(&self, route: &str) -> Vec<Value> {
        let (names, ids, visible) = self.app_index();
        let packages = array(&self.snapshot["packages"]);
        let tr = |id, fallback| self.tr(id, fallback, json!({}));
        array(&self.catalog["software"]).iter().map(|raw|{let entry=self.localized(raw);let package=text::field(&entry,"package");let short=package.strip_suffix("-bin").or_else(||package.strip_suffix("-git")).unwrap_or(&package);let package_id=model::normalized(short);let app_id=model::normalized(&text::field(&entry,"appId"));let name=model::normalized(&text::field(&entry,"label"));
            let mut alternatives=vec![package.clone()];alternatives.extend(array(&entry["packageAlternatives"]).iter().map(text::string));let installed_package=alternatives.into_iter().find(|name|packages.iter().any(|p|p["name"]==*name));let installed=installed_package.is_some()||names.contains(&name)||ids.contains(&package_id)||(!app_id.is_empty()&&ids.contains(&app_id));let desktop_id=visible.get(&name).or_else(||visible.get(&app_id)).or_else(||visible.get(&package_id)).cloned().unwrap_or_default();let aur=entry["repository"]=="AUR"&&!yes(&self.snapshot["capabilities"]["aur"]);
            let web=text::field(&entry,"webUrl");let saved=array(&self.snapshot["webapps"]).iter().find(|v|text::field(v,"url").strip_suffix('/').unwrap_or(v["url"].as_str().unwrap_or(""))==web.strip_suffix('/').unwrap_or(&web));let package_available=entry["repository"]!="OpenAI"||self.library["ai"]["chatgptPackage"]==true;
            let mut aliases=vec![json!(package)];aliases.extend(array(&entry["aliases"]).iter().cloned());
            if !web.is_empty()&&installed_package.is_none()&&(saved.is_some()||(!installed&&(aur||!yes(&self.snapshot["capabilities"]["packages"])||!package_available))){let mut extra=json!({"software":true,"category":"ai","categoryLabel":tr("software.category.ai","AI"),"webUrl":entry["webUrl"],"webapp":saved.is_some(),"desktopId":visible.get(&name).cloned().unwrap_or_default(),"checked":saved.is_some(),"aliases":aliases,"status":if saved.is_some(){tr("software.ai.web-installed","Web app installed")}else{tr("software.ai.web-offer","Create web app")},"disabled":saved.is_none()&&self.library["ai"]["webApps"]!=true,"reason":tr("software.ai.web-requires-browser","Requires a Chromium-based browser for the web app")});if let Some(value)=entry.get("website"){extra["website"]=value.clone();}return self.value_row(route,&hash(&package),&text::field(&entry,"label"),if saved.is_some(){"webapp"}else{"webapp-offer"},saved.map(|v|v["id"].clone()).filter(yes).unwrap_or(entry["webUrl"].clone()),"public",&text::field(&entry,"description"),extra);}
            let terminal=yes(&entry["terminalCommand"]);let actual=installed_package.unwrap_or(package.clone());let category=entry["category"].as_str().filter(|s|!s.is_empty()).unwrap_or("utilities");let category_label=array(&self.constants["categories"]).iter().find(|c|c["value"]==entry["category"]).map(|v|self.localized(v)).map(|v|text::field(&v,"label")).unwrap_or_default();
            let reason=if installed&&desktop_id.is_empty(){tr("launcher.disabled.installed-use-item-actions-to-remove-it","Installed; use item actions to remove it")}else if !installed&&aur{tr("launcher.install-offer.aur","Requires git, less and makepkg for AUR review, or paru/yay for the native flow")}else if !installed&&!yes(&self.snapshot["capabilities"]["packages"]){tr("launcher.disabled.requires-an-arch-compatible-package-manager","Requires an Arch-compatible package manager")}else{String::new()};
            self.value_row(route,&hash(&package),&text::field(&entry,"label"),if installed&&terminal{"ai-terminal"}else if installed{if desktop_id.is_empty(){"info"}else{"software-open"}}else{"install-package"},json!(actual),"download",&text::field(&entry,"description"),json!({"package":actual,"terminalCommand":entry.get("terminalCommand").cloned().unwrap_or(json!([])),"software":true,"category":category,"categoryLabel":category_label,"version":packages.iter().find(|v|v["name"]==entry["package"]).map(|v|text::field(v,"version")).unwrap_or_default(),"desktopId":desktop_id,"website":text::field(&entry,"website"),"screenshots":entry.get("screenshots").cloned().unwrap_or(json!([])),"screenshotSource":text::field(&entry,"screenshotSource"),"repository":text::field(&entry,"repository"),"aliases":aliases,"checked":installed,"status":if installed{tr("ui.menu-context.status.installed","Installed")}else{tr("ui.menu-context.status.install","Install")},"disabled":if installed&&terminal{self.library["ai"]["terminal"]!=true}else{!installed&&(!yes(&self.snapshot["capabilities"]["packages"])||aur)},"reason":reason}))
        }).collect()
    }
    pub fn font_rows(&self, mono: bool) -> Vec<Value> {
        let route = if mono {
            "style.mono-fonts"
        } else {
            "style.fonts"
        };
        let key = if mono { "monoFont" } else { "interfaceFont" };
        let operation = if mono { "mono-font" } else { "font" };
        let tr = |id, fallback| self.tr(id, fallback, json!({}));
        let fonts = array(&self.snapshot["fonts"]);
        let mut rows = vec![self.value_row(
            route,
            "default",
            &tr("launcher.provider.nacre-default", "Nacre default"),
            operation,
            json!(""),
            "text_fields",
            &self.tr(
                "ui.menu-context.description.ibm-plex-sans",
                "{font}",
                json!({"font":if mono{"JetBrainsMono Nerd Font"}else{"IBM Plex Sans"}}),
            ),
            json!({"checked":!yes(&self.settings[key])}),
        )];
        let mut used = BTreeSet::new();
        for item in array(&self.catalog["fonts"]) {
            let family = fonts
                .iter()
                .find(|name| {
                    array(&item["families"]).iter().any(|alias| {
                        model::normalized(&text::string(alias))
                            == model::normalized(&text::string(name))
                    })
                })
                .map(text::string);
            if let Some(family) = &family {
                used.insert(family.clone());
            }
            let aur = item["repository"] == "AUR" && !yes(&self.snapshot["capabilities"]["aur"]);
            let disabled =
                family.is_none() && (!yes(&self.snapshot["capabilities"]["packages"]) || aur);
            let package = text::field(item, "package");
            let name = text::field(item, "name");
            let choice = family.as_deref().unwrap_or(&package);
            let mut aliases = vec![json!(package)];
            aliases.extend(array(&item["families"]).iter().cloned());
            aliases.push(json!(if mono {
                "terminal monospace font typeface nerd"
            } else {
                "interface font typeface nerd"
            }));
            let mut extra = json!({"previewFont":family.clone().unwrap_or_default(),"checked":family.as_ref().is_some_and(|v|self.settings[key]==*v),"aliases":aliases,"disabled":disabled,"reason":if aur{tr("launcher.install-offer.aur","Requires git, less and makepkg for AUR review, or paru/yay for the native flow")}else if disabled{tr("launcher.disabled.requires-an-arch-compatible-package-manager","Requires an Arch-compatible package manager")}else{String::new()},"confirm":if family.is_some(){String::new()}else{self.tr("software.fonts.confirm","Open the package manager to install {font}? Administrator permission may be required.",json!({"font":name}))}});
            if family.is_some() {
                extra["status"] = json!(tr("ui.menu-context.status.installed", "Installed"));
            } else if let Some(value) = item.get("repository") {
                extra["status"] = value.clone();
            }
            let description = if family.is_some() && mono {
                tr(
                    "software.fonts.mono-preview",
                    "Use for Kitty and Nacre monospace text",
                )
            } else if family.is_some() {
                tr(
                    "software.fonts.choose",
                    "Preview and select this interface font",
                )
            } else {
                tr(
                    "software.fonts.install",
                    "Install this font package in a visible terminal",
                )
            };
            rows.push(self.value_row(
                route,
                &hash(choice),
                &if mono {
                    self.tr(
                        "software.fonts.mono-choice",
                        "Terminal font: {font}",
                        json!({"font":name}),
                    )
                } else {
                    name
                },
                if family.is_some() {
                    operation
                } else {
                    "install-package"
                },
                json!(choice),
                "text_fields",
                &description,
                extra,
            ));
        }
        if !mono {
            for family in fonts {
                let name = text::string(family);
                if !used.contains(&name) {
                    rows.push(self.value_row(route,&hash(&name),&name,operation,family.clone(),"text_fields",&tr("launcher.provider.preview-this-interface-font","Preview this interface font"),json!({"previewFont":family,"disabled":name.encode_utf16().count()>80,"checked":self.settings[key]==*family})));
                }
            }
        }
        rows
    }
    pub fn theme_rows(&self) -> Vec<Value> {
        let themes = self
            .library
            .get("themes")
            .filter(|v| yes(v))
            .unwrap_or(&self.wallpapers["palettes"]);
        let tr = |id, fallback| self.tr(id, fallback, json!({}));
        array(themes).iter().map(|theme|{let active=self.wallpapers["appearancePreferences"]["palettePreset"]==theme["id"];let colors=theme.get("swatches").filter(|v|yes(v)).cloned().unwrap_or_else(||json!(["primary","secondary","tertiary"].iter().map(|k|theme["modes"]["dark"][k].clone()).filter(yes).collect::<Vec<_>>()));self.value_row("software.themes",&hash(&text::field(theme,"id")),&text::field(theme,"name"),"software-theme",theme["id"].clone(),"palette",&tr("software.theme.description","Use this palette throughout your desktop and applications"),json!({"theme":true,"bundle":theme["bundle"]==true,"wallpapers":theme.get("wallpapers").cloned().unwrap_or(json!([])),"modeNames":theme.get("modeNames").cloned().unwrap_or(json!({})),"credit":text::field(theme,"credit"),"upstreamCredit":text::field(&theme["upstream"],"credit"),"website":text::field(&theme["upstream"],"url"),"colors":colors,"themeModes":theme.get("modes").cloned().unwrap_or(json!({})),"surface":theme["surface"].as_str().filter(|s|!s.is_empty()).or_else(||theme["modes"]["dark"]["surface"].as_str().filter(|s|!s.is_empty())).unwrap_or("#151515"),"group":text::field(theme,"group"),"checked":active,"status":if active{tr("ui.menu-context.status.in-use","In use")}else{String::new()},"aliases":[tr("software.theme.alias","theme colors palette")]}))}).collect()
    }
    pub fn extension_rows(&self) -> Vec<Value> {
        let tr = |id, fallback| self.tr(id, fallback, json!({}));
        array(&self.library["extensions"]).iter().map(|item|{let mut aliases=vec![item["id"].clone()];aliases.extend(array(&item["kinds"]).iter().cloned());self.value_row("software.extensions",&hash(&text::field(item,"id")),&text::field(item,"name"),if yes(&item["available"]){"software-extension-install"}else{"software-extension-toggle"},item["id"].clone(),"extension",&text::field(item,"description"),json!({"extension":item,"version":text::field(item,"version"),"website":text::field(item,"website"),"preview":text::field(item,"previewPath"),"checked":item["enabled"]==true,"status":if yes(&item["available"]){tr("software.extension.available","Available")}else if yes(&item["enabled"]){tr("software.extension.enabled","Enabled")}else{tr("software.extension.disabled","Disabled")},"disabled":item["broken"]==true,"reason":if yes(&item["broken"]){text::field(item,"description")}else{String::new()},"aliases":aliases}))}).collect()
    }
    pub fn extension_actions(&self) -> Vec<Value> {
        let mut rows = Vec::new();
        for extension in array(&self.library["extensions"]) {
            if !yes(&extension["enabled"]) || yes(&extension["broken"]) {
                continue;
            }
            for action in array(&extension["actions"]) {
                rows.push(
                    self.value_row(
                        "software.extensions",
                        &hash(&format!(
                            "{}:{}",
                            text::field(extension, "id"),
                            text::field(action, "id")
                        )),
                        &text::field(action, "name"),
                        "software-extension-action",
                        extension["id"].clone(),
                        action["icon"]
                            .as_str()
                            .filter(|s| !s.is_empty())
                            .unwrap_or("extension"),
                        action["description"]
                            .as_str()
                            .filter(|s| !s.is_empty())
                            .unwrap_or_else(|| extension["name"].as_str().unwrap_or("")),
                        json!({"extension":extension,"extensionAction":action["id"]}),
                    ),
                );
            }
            if array(&extension["kinds"]).contains(&json!("panel")) {
                rows.push(self.value_row(
                    "software.extensions",
                    &hash(&format!("{}:panel", text::field(extension, "id"))),
                    &text::field(extension, "name"),
                    "software-extension-open",
                    extension["id"].clone(),
                    "extension",
                    &text::field(extension, "description"),
                    json!({"extension":extension}),
                ));
            }
        }
        rows
    }
    pub fn environment_rows(&self) -> Vec<Value> {
        let tr = |id, fallback| self.tr(id, fallback, json!({}));
        let mut rows = Vec::new();
        for profile in array(&self.library["environments"]["profiles"]) {
            for operation in ["install", "remove"] {
                let installed = profile["installed"] == true;
                let name = self.tr(
                    if operation == "install" {
                        "software.development.install"
                    } else {
                        "software.development.remove"
                    },
                    if operation == "install" {
                        "Install {name}"
                    } else {
                        "Remove {name}"
                    },
                    json!({"name":profile["name"]}),
                );
                let db = profile["kind"] == "database";
                rows.push(self.value_row("software.development",&format!("{}-{operation}",text::field(profile,"id")),&name,"environment",profile["id"].clone(),if db{"database"}else{"code"},&if db{tr("software.development.database-description","Local Docker database; removal retains its data")}else{tr("software.development.mise-description","Managed by mise; runs in a visible terminal")},json!({"environmentOperation":operation,"disabled":!yes(&profile["available"])||operation=="remove"&&!installed,"reason":profile["reason"].as_str().filter(|s|!s.is_empty()).map(str::to_owned).unwrap_or_else(||if operation=="remove"&&!installed{tr("software.development.not-installed","Not installed")}else{String::new()}),"aliases":[profile["name"],profile["id"],"development runtime mise"],"confirm":tr("software.development.open-terminal","Open a terminal to review and confirm this change?")})));
            }
        }
        rows
    }
    pub fn system_rows(&self, section: &str) -> Vec<Value> {
        let tr = |id, fallback| self.tr(id, fallback, json!({}));
        array(&self.library["system"][section]).iter().map(|entry|self.value_row(&text::field(entry,"section"),&text::field(entry,"id"),&text::field(entry,"name"),"system-action",entry["id"].clone(),"build",&text::field(entry,"description"),json!({"confirm":tr("software.actions.open-confirm","Open a terminal to review and confirm this action?"),"aliases":[entry["id"],"repair maintenance fix"],"status":if yes(&entry["root"]){tr("software.actions.admin","Administrator permission")}else{String::new()}}))).collect()
    }
    pub fn tui_rows(&self) -> Vec<Value> {
        let tr = |id, fallback| self.tr(id, fallback, json!({}));
        if yes(&self.library["tui"]["error"]) {
            return vec![self.value_row(
                "software.tui",
                "error",
                &tr(
                    "software.tui.registry-error",
                    "Terminal apps need attention",
                ),
                "info",
                json!("tui"),
                "terminal",
                &text::field(&self.library["tui"], "error"),
                json!({"disabled":true}),
            )];
        }
        array(&self.library["tui"]["rows"]).iter().map(|entry|self.value_row("software.tui",&text::field(entry,"id"),&text::field(entry,"name"),"tui-launch",entry["id"].clone(),&text::field(entry,"icon"),&array(&entry["command"]).iter().map(text::string).collect::<Vec<_>>().join(" · "),json!({"tui":true,"terminal":entry["terminal"],"disabled":!yes(&entry["available"]),"reason":if yes(&entry["available"]){String::new()}else{tr("software.tui.requires-command","Install the selected terminal and command first")}}))).collect()
    }
}
