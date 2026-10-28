use crate::{
    context::{array, hash, yes, Context},
    model, text, Result,
};
use serde_json::{json, Value};

pub fn category(entry: &Value) -> &'static str {
    if entry["kind"] == "app" {
        "apps"
    } else if yes(&entry["emoji"]) {
        "emoji"
    } else if yes(&entry["note"]) {
        "notes"
    } else if yes(&entry["clipboard"]) {
        "clipboard"
    } else if yes(&entry["file"]) {
        "files"
    } else if entry["operation"] == "focus" {
        "windows"
    } else if entry["operation"] == "workflow" {
        "saved"
    } else {
        "actions"
    }
}
impl Context {
    pub fn rank(&self, rows: &[Value], query: &str) -> Vec<Value> {
        model::ranked(
            rows,
            query,
            &self.preferences["usage"],
            array(&self.preferences["recent"]),
            self.now,
        )
        .into_iter()
        .cloned()
        .collect()
    }
    pub fn file_results(&self, query: &str) -> Vec<Value> {
        if self.file_query != query {
            return Vec::new();
        }
        self.files
            .iter()
            .map(|item| {
                self.value_row(
                    "actions.files",
                    &text::field(item, "id"),
                    &text::field(item, "name"),
                    "open-file",
                    item["path"].clone(),
                    "description",
                    &text::field(item, "path"),
                    json!({"file":true}),
                )
            })
            .collect()
    }
    pub fn brain_rows(&self, query: &str) -> Vec<Value> {
        if self.brain_query != query {
            return Vec::new();
        }
        self.notes.iter().map(|item|{let path=text::field(item,"path");self.value_row("actions.assistant.search",&hash(&path),&text::field(item,"title"),"note",item["path"].clone(),"description",&text::field(item,"preview"),json!({"breadcrumb":"Nacre Brain","note":true,"matchedQuery":query,"filePath":format!("{}/{}",text::field(&self.workflows,"brainRoot"),path),"previewText":item["preview"]}))}).collect()
    }
    pub fn workflow_rows(&self, query: &str) -> Vec<Value> {
        let mut rows = Vec::new();
        for item in array(&self.workflows["entries"]) {
            let kind = text::field(item, "type");
            let icon =
                item["icon"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .unwrap_or(match kind.as_str() {
                        "action" => "terminal",
                        "snippet" => "content_paste",
                        "search" => "public",
                        "bookmark" => "bookmark",
                        "workspace" => "workspaces",
                        _ => "",
                    });
            let description = if kind == "snippet" {
                text::field(item, "text")
            } else {
                item["directory"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .or_else(|| item["url"].as_str())
                    .unwrap_or("")
                    .into()
            };
            rows.push(self.value_row("actions.saved",&text::field(item,"id"),&text::field(item,"label"),"workflow",item["id"].clone(),icon,&description,json!({"workflow":item,"searchQuery":query,"previewText":if kind=="snippet"{text::field(item,"text")}else{String::new()}})));
        }
        for item in array(&self.workflows["projects"]) {
            let id = text::field(item, "id");
            rows.push(
                self.value_row(
                    "actions.saved",
                    &format!("project-{id}"),
                    item["label"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .unwrap_or(&id),
                    "workflow",
                    json!(format!("project:{id}")),
                    "workspaces",
                    &text::field(item, "path"),
                    json!({"searchQuery":query}),
                ),
            );
        }
        rows
    }
    pub fn capture_rows(&self) -> Vec<Value> {
        array(&self.workflows["captures"])
            .iter()
            .map(|item| {
                self.value_row(
                    "actions.capture",
                    &hash(&text::field(item, "path")),
                    &text::field(item, "name"),
                    "open-file",
                    item["path"].clone(),
                    "perm_media",
                    &text::field(item, "path"),
                    json!({"file":true,"capture":true}),
                )
            })
            .collect()
    }
    pub fn web_search_rows(&self, query: &str) -> Vec<Value> {
        array(&self.workflows["entries"])
            .iter()
            .filter(|item| item["type"] == "search")
            .map(|item| {
                let short =
                    String::from_utf16_lossy(&query.encode_utf16().take(100).collect::<Vec<_>>());
                self.value_row(
                    "actions.saved",
                    &text::field(item, "id"),
                    &self.tr(
                        "launcher.workflows.search-engine",
                        "Search {engine} for “{query}”",
                        json!({"engine":item["label"],"query":short}),
                    ),
                    "workflow",
                    item["id"].clone(),
                    "public",
                    &self.tr(
                        "ui.menu-context.description.opens-your-browser-only-when-selected",
                        "Opens your browser only when selected",
                        json!({}),
                    ),
                    json!({"workflow":item,"searchQuery":query}),
                )
            })
            .collect()
    }
    pub fn favorites(&self) -> Result<Vec<Value>> {
        let mut candidates = self.all_static();
        candidates.extend(self.workflow_rows(""));
        let favorites = array(&self.preferences["favorites"]);
        for row in self.rows().iter().filter(|row| {
            yes(&row["provider"])
                && favorites.iter().any(|id| {
                    text::string(id).starts_with(&format!("nacre-menu:{}.", text::field(row, "id")))
                })
        }) {
            candidates.extend(self.provider(&text::field(row, "id"), "")?);
        }
        let apps = self.apps();
        let selected: Vec<_> = favorites
            .iter()
            .filter_map(|id| {
                model::favorite(&text::string(id), &apps, &candidates, &self.legacy_actions)
            })
            .cloned()
            .collect();
        Ok(model::alphabetical(&selected, &self.ordering)
            .into_iter()
            .cloned()
            .collect())
    }
    pub fn install_offers(&self) -> Vec<Value> {
        self.software_rows("software.discover")
            .into_iter()
            .filter(|r| !yes(&r["checked"]))
            .map(|mut row| {
                row["installOffer"] = json!(true);
                row["description"] = json!(self.tr(
                    "launcher.install-offer.description",
                    "Install {app} · {purpose}",
                    json!({"app":row["name"],"purpose":row["description"]})
                ));
                row
            })
            .collect()
    }
    pub fn installed_ai_rows(&self) -> Vec<Value> {
        self.software_rows("software.discover.ai")
            .into_iter()
            .filter(|r| r["category"] == "ai" && yes(&r["checked"]) && !yes(&r["desktopId"]))
            .collect()
    }
    pub(crate) fn global_prefix(&self) -> Result<Vec<Value>> {
        let actions = yes(&self.source_options["actions"]);
        let mut rows = self.apps();
        rows.extend(self.all_static().into_iter().filter(|r| {
            actions || r["kind"] == "group" || text::field(r, "id").starts_with("software.")
        }));
        rows.extend(self.provider("actions.clipboard", "")?);
        Ok(rows)
    }
    pub(crate) fn global_suffix(&self) -> Result<Vec<Value>> {
        let actions = yes(&self.source_options["actions"]);
        let mut rows = Vec::new();
        rows.extend(self.provider("style.wallpapers", "")?);
        rows.extend(self.provider("style.fonts", "")?);
        rows.extend(self.provider("style.mono-fonts", "")?);
        rows.extend(self.install_offers());
        rows.extend(self.theme_rows());
        rows.extend(
            self.extension_rows()
                .into_iter()
                .filter(|r| !yes(&r["extension"]["enabled"])),
        );
        rows.extend(self.extension_actions());
        rows.extend(self.environment_rows());
        rows.extend(self.installed_ai_rows());
        if actions {
            for section in ["health", "setup", "hardware", "speed"] {
                rows.extend(self.system_rows(section));
            }
        }
        let apps = self.apps();
        rows.extend(self.tui_rows().into_iter().filter(|row| {
            !apps.iter().any(|app| {
                text::field(app, "id")
                    .strip_suffix(".desktop")
                    .unwrap_or(app["id"].as_str().unwrap_or(""))
                    == format!("nacre-tui-{}", text::field(row, "value"))
            })
        }));
        rows.extend(self.capture_rows());
        Ok(rows)
    }
    pub(crate) fn global_dynamic_without_emoji(&self, query: &str) -> Result<Vec<Value>> {
        let mut rows = Vec::new();
        rows.extend(self.provider("actions.windows", query)?);
        if yes(&self.source_options["files"]) {
            rows.extend(self.file_results(query));
        }
        if yes(&self.source_options["brain"]) {
            rows.extend(self.brain_rows(query));
        }
        rows.extend(self.workflow_rows(query));
        Ok(rows)
    }
    pub(crate) fn global_dynamic(&self, query: &str) -> Result<Vec<Value>> {
        let mut rows = self.global_dynamic_without_emoji(query)?;
        rows.extend(self.provider("actions.emoji", query)?);
        Ok(rows)
    }
    pub fn global_rows(&self, query: &str) -> Result<Vec<Value>> {
        let mut rows = self.global_prefix()?;
        rows.extend(self.global_dynamic(query)?);
        rows.extend(self.global_suffix()?);
        Ok(rows)
    }
    pub fn content(
        &self,
        route: &str,
        query: &str,
        filter: &str,
        software_category: &str,
    ) -> Result<Vec<Value>> {
        let scope = model::scope(query);
        let kind = scope["kind"].as_str().unwrap();
        let term = scope["text"].as_str().unwrap();
        let tr = |id, fallback| self.tr(id, fallback, json!({}));
        if matches!(kind, "brain" | "files" | "calculator" | "actions" | "ai")
            && self.source_options[kind] == false
        {
            return Ok(vec![self.value_row(
                "setup",
                "source-disabled",
                &tr(
                    "launcher.sources.disabled",
                    "This search source is turned off in Settings",
                ),
                "info",
                json!("sources"),
                "settings",
                &tr(
                    "launcher.sources.disabled-description",
                    "Enable it in Settings to use this prefix",
                ),
                json!({"disabled":true}),
            )]);
        }
        match kind {
            "files" => return Ok(self.file_results(term)),
            "calculator" => return Ok(Vec::new()),
            "brain" => return Ok(self.brain_rows(term)),
            "ai" => {
                return Ok(if term.is_empty() {
                    Vec::new()
                } else {
                    vec![self.value_row("actions.assistant","question",&tr("ui.menu-context.label.ask-nacre-ai","Ask Nacre AI"),"ask",json!(term),"chat",term,json!({"brand":"ai","disabled":!self.can_ask||!yes(&self.snapshot["capabilities"]["ai"]),"reason":if !yes(&self.snapshot["capabilities"]["ai"]){tr("launcher.disabled.nacre-ai-is-optional-open-setup-to-add-it","Nacre AI is optional; open Setup to add it")}else if !self.can_ask{tr("launcher.disabled.an-assistant-task-or-unsent-draft-is-active-open-nacre-ai-instead","An assistant task or unsent draft is active; open Nacre AI instead")}else{String::new()}}))]
                })
            }
            "help" => {
                let mut rows = Vec::new();
                for (source, name, name_id, label_id, description, description_id, icon, prefix) in [
                    (
                        "actions",
                        "> Actions",
                        "ui.menu-context.name.actions",
                        "ui.menu-context.label.actions",
                        "Search desktop commands",
                        "ui.menu-context.description.search-desktop-commands",
                        "bolt",
                        "> ",
                    ),
                    (
                        "calculator",
                        "= Calculator",
                        "ui.menu-context.name.calculator",
                        "ui.menu-context.label.calculator",
                        "Calculate and copy a result",
                        "ui.menu-context.description.calculate-and-copy-a-result",
                        "calculate",
                        "= ",
                    ),
                    (
                        "brain",
                        "# Brain",
                        "ui.menu-context.name.brain",
                        "ui.menu-context.label.brain",
                        "Search private notes",
                        "ui.menu-context.description.search-private-notes",
                        "neurology",
                        "# ",
                    ),
                    (
                        "files",
                        ": Files",
                        "ui.menu-context.name.files",
                        "ui.menu-context.label.files",
                        "Search your usual folders",
                        "ui.menu-context.description.search-your-usual-folders",
                        "folder",
                        ": ",
                    ),
                    (
                        "ai",
                        "? Nacre AI",
                        "ui.menu-context.name.nacre-ai",
                        "ui.menu-context.label.nacre-ai",
                        "Ask an explicit question",
                        "ui.menu-context.description.ask-an-explicit-question",
                        "chat",
                        "? ",
                    ),
                ] {
                    if self.source_options[source] != false {
                        rows.push(json!({"id":format!("scope-{source}"),"kind":"action","name":tr(name_id,name),"label":tr(label_id,name),"description":tr(description_id,description),"operation":"scope","value":prefix,"icon":icon}));
                    }
                }
                return Ok(rows);
            }
            "actions" => {
                return Ok(self.rank(
                    &self
                        .all_static()
                        .into_iter()
                        .filter(|r| r["kind"] != "group")
                        .collect::<Vec<_>>(),
                    term,
                ))
            }
            _ => {}
        }
        if !term.is_empty() {
            let mut rows = self.global_rows(term)?;
            if route == "software.packages" {
                rows.extend(self.provider(route, term)?);
            }
            rows.retain(|r| filter == "all" || category(r) == filter);
            let mut result = self.rank(&rows, term);
            if filter == "all" {
                result.extend(self.web_search_rows(term));
                let short =
                    String::from_utf16_lossy(&term.encode_utf16().take(100).collect::<Vec<_>>());
                result.push(self.value_row(
                    "actions",
                    "search-web",
                    &self.tr(
                        "ui.menu-context.label.search-the-web-for-value0",
                        "Search the web for “{value0}”",
                        json!({"value0":short}),
                    ),
                    "search-web",
                    json!(term),
                    "public",
                    &tr(
                        "ui.menu-context.description.opens-your-browser-only-when-selected",
                        "Opens your browser only when selected",
                    ),
                    json!({}),
                ));
            }
            return Ok(result);
        }
        if route == "home" {
            return self.favorites();
        }
        if route == "apps" {
            return Ok(self.apps());
        }
        if route == "hidden" {
            return Ok(self
                .all_apps
                .iter()
                .filter(|app| array(&self.preferences["hidden"]).contains(&app["id"]))
                .map(|app| {
                    let mut row = model::application(app);
                    row["hidden"] = json!(true);
                    row
                })
                .collect());
        }
        if route == "software.discover" && software_category != "all" {
            return Ok(self
                .provider(route, "")?
                .into_iter()
                .filter(|r| r["category"] == software_category)
                .collect());
        }
        let mut result: Vec<_> = model::children(route, self.rows())
            .into_iter()
            .map(|r| self.decorate(r))
            .collect();
        result.extend(if route == "actions.saved" {
            self.workflow_rows("")
        } else if route == "actions.capture" {
            self.capture_rows()
        } else {
            self.provider(route, "")?
        });
        Ok(result)
    }
}
