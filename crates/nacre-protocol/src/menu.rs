//! Additive launcher method-v1 contract. Interpretation/ranking belongs to Nacre.
//! Engine implementations register methods separately from this payload module.
use crate::{ErrorCode, Failure, MAX_SAFE_INTEGER};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const VERSION: u16 = 1;
pub const MAX_ITEMS: usize = 2048;
pub const UI_ACTIVATE: &str = "ui.menu.activate";
fn apps() -> String {
    "apps".into()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MenuQuery {
    /// Passed unchanged to the shared engine, including all provider prefixes.
    #[serde(default)]
    #[schemars(length(max = 65536))]
    pub query: String,
    /// Open string vocabulary: apps/search/actions/palette; no client ranking.
    #[serde(default = "apps")]
    #[schemars(length(min = 1, max = 128))]
    pub mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 256))]
    pub route: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 2048))]
    pub limit: Option<u32>,
    /// Existing launcher result filter; omitted retains "all".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 128))]
    pub filter: Option<String>,
    /// Existing software browser category; omitted retains "all".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 128))]
    pub category: Option<String>,
    /// Include existing view metadata for the compatibility QML view.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MenuItem {
    #[schemars(length(max = 4096))]
    pub name: String,
    /// Extensible engine vocabulary; unknown kinds still have a display label.
    #[schemars(length(min = 1, max = 128))]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 16384))]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 4096))]
    pub breadcrumb: Option<String>,
    /// Theme name or local cache path, never image/base64 data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 4096))]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 128))]
    pub emoji: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 128))]
    pub brand: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 128))]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 4096))]
    pub reason: Option<String>,
    #[serde(default)]
    pub disabled: bool,
    /// Opaque engine descriptor, not a command or permission grant. Echo only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 1, max = 512))]
    pub activation: Option<String>,
    /// Display/context metadata only. Activation must still echo the opaque ticket.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<serde_json::Value>,
}

/// Arithmetic result or unit-conversion payload from the existing calculator.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MenuConversion {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 2048))]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 4096))]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 4096))]
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MenuQueryResult {
    #[schemars(length(max = 65536))]
    pub query: String,
    #[schemars(length(min = 1, max = 128))]
    pub mode: String,
    #[schemars(length(max = 256))]
    pub route: String,
    #[schemars(range(max = 9007199254740991_u64))]
    pub catalog_rev: u64,
    /// Stable-ID keyed entities; order is separate and authoritative.
    #[schemars(length(max = 2048))]
    pub items: BTreeMap<String, MenuItem>,
    #[schemars(length(max = 2048))]
    pub order: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversion: Option<MenuConversion>,
    /// Nonfatal provider guidance, such as installing a missing search tool.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 4096))]
    pub message: Option<String>,
}

/// `menu.groups` takes no parameters.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MenuGroups {}

/// One top-level launcher group (category rail entry), localized like menu rows.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MenuGroup {
    /// Route ID accepted by `menu.query` `route` (for example `style`).
    #[schemars(length(min = 1, max = 128))]
    pub id: String,
    #[schemars(length(max = 4096))]
    pub label: String,
    /// Material symbol name; empty when the group has none.
    #[schemars(length(max = 128))]
    pub icon: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MenuGroupsResult {
    #[schemars(range(max = 9007199254740991_u64))]
    pub catalog_rev: u64,
    /// Catalog order is the default rail order; ordering and hiding stay with the caller.
    #[schemars(length(max = 64))]
    pub groups: Vec<MenuGroup>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MenuActivate {
    #[schemars(length(min = 1, max = 256))]
    pub id: String,
    #[schemars(length(min = 1, max = 512))]
    pub activation: String,
    #[schemars(range(max = 9007199254740991_u64))]
    pub catalog_rev: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MenuView {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MenuActivated {
    #[serde(default)]
    pub keep_open: bool,
    /// A group/navigation result changes view state, never asks a UI to execute.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<MenuView>,
}

/// A selected row's literal operand; never a shell expression or an RPC method.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum MenuActionValue {
    Text(#[schemars(length(max = 65536))] String),
    Integer(#[schemars(range(min = -9007199254740991_i64, max = 9007199254740991_i64))] i64),
}

/// Existing view-owned action data, selected by the engine after ticket checks.
/// Unknown operations must be refused, never interpreted as executable text.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MenuUiRow {
    #[schemars(length(min = 1, max = 256))]
    pub id: String,
    #[schemars(length(min = 1, max = 128))]
    pub kind: String,
    #[schemars(length(max = 4096))]
    pub name: String,
    #[schemars(length(min = 1, max = 128))]
    pub operation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<MenuActionValue>,
    #[serde(default)]
    pub disabled: bool,
    /// A prompt, not evidence that the owner has confirmed an action.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 4096))]
    pub confirm: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 4096))]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 16384))]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 4096))]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 4096))]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 4096))]
    pub action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 32))]
    pub arguments: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 16384))]
    pub directory: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 128))]
    pub environment_operation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 64))]
    pub terminal_command: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 65536))]
    pub web_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 65536))]
    pub search_query: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 128))]
    pub bundle_scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 128))]
    pub theme_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 128))]
    pub extension_action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 128))]
    pub source_extension: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 128))]
    pub theme_preset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transient: Option<bool>,
}

/// Daemon-to-provider only. Public clients must use MenuActivate's opaque ticket.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MenuUiActivate {
    pub row: MenuUiRow,
    #[schemars(range(max = 9007199254740991_u64))]
    pub catalog_rev: u64,
}

fn invalid() -> Failure {
    Failure::new(
        ErrorCode::InvalidRequest,
        "Invalid launcher method parameters or result",
        false,
    )
}
fn text(value: &str, max: usize, empty: bool) -> bool {
    (empty || !value.is_empty()) && value.len() <= max && !value.contains('\0')
}
impl MenuQuery {
    pub fn validate(&self) -> Result<(), Failure> {
        if text(&self.query, 65536, true)
            && text(&self.mode, 128, false)
            && self.route.as_ref().is_none_or(|s| text(s, 256, true))
            && self.limit.is_none_or(|n| (1..=2048).contains(&n))
            && self.filter.as_ref().is_none_or(|s| text(s, 128, true))
            && self.category.as_ref().is_none_or(|s| text(s, 128, true))
        {
            Ok(())
        } else {
            Err(invalid())
        }
    }
}
impl MenuQueryResult {
    pub fn validate(&self) -> Result<(), Failure> {
        let request = MenuQuery {
            query: self.query.clone(),
            mode: self.mode.clone(),
            route: Some(self.route.clone()),
            limit: None,
            filter: None,
            category: None,
            presentation: None,
        };
        request.validate()?;
        if self.message.as_ref().is_some_and(|s| !text(s, 4096, true))
            || self.conversion.as_ref().is_some_and(|v| {
                v.value.as_ref().is_some_and(|s| !text(s, 2048, true))
                    || v.description.as_ref().is_some_and(|s| !text(s, 4096, true))
                    || v.error.as_ref().is_some_and(|s| !text(s, 4096, true))
            })
        {
            return Err(invalid());
        }
        if self.catalog_rev > MAX_SAFE_INTEGER
            || self.items.len() > MAX_ITEMS
            || self.order.len() != self.items.len()
            || self.total.is_some_and(|n| (n as usize) < self.items.len())
        {
            return Err(invalid());
        }
        let mut seen = BTreeSet::new();
        for id in &self.order {
            if !text(id, 256, false) || !seen.insert(id) || !self.items.contains_key(id) {
                return Err(invalid());
            }
        }
        for (id, item) in &self.items {
            if !text(id, 256, false)
                || !text(&item.name, 4096, true)
                || !text(&item.kind, 128, false)
                || item
                    .description
                    .as_ref()
                    .is_some_and(|s| !text(s, 16384, true))
                || item
                    .breadcrumb
                    .as_ref()
                    .is_some_and(|s| !text(s, 4096, true))
                || item.icon.as_ref().is_some_and(|s| !text(s, 4096, true))
                || item.emoji.as_ref().is_some_and(|s| !text(s, 128, true))
                || item.brand.as_ref().is_some_and(|s| !text(s, 128, true))
                || item.status.as_ref().is_some_and(|s| !text(s, 128, true))
                || item.reason.as_ref().is_some_and(|s| !text(s, 4096, true))
                || item
                    .activation
                    .as_ref()
                    .is_some_and(|s| !text(s, 512, false))
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
}
impl MenuGroupsResult {
    pub fn validate(&self) -> Result<(), Failure> {
        if self.catalog_rev > MAX_SAFE_INTEGER
            || self.groups.len() > 64
            || self.groups.iter().any(|g| {
                !text(&g.id, 128, false) || !text(&g.label, 4096, true) || !text(&g.icon, 128, true)
            })
        {
            return Err(invalid());
        }
        Ok(())
    }
}
impl MenuActivate {
    pub fn validate(&self) -> Result<(), Failure> {
        if text(&self.id, 256, false)
            && text(&self.activation, 512, false)
            && self.catalog_rev <= MAX_SAFE_INTEGER
        {
            Ok(())
        } else {
            Err(invalid())
        }
    }
}
impl MenuActivated {
    pub fn validate(&self) -> Result<(), Failure> {
        if let Some(v) = &self.view {
            if v.query.as_ref().is_some_and(|s| !text(s, 65536, true))
                || v.mode.as_ref().is_some_and(|s| !text(s, 128, false))
                || v.route.as_ref().is_some_and(|s| !text(s, 256, true))
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
}

impl MenuUiActivate {
    pub fn validate(&self) -> Result<(), Failure> {
        let row = &self.row;
        if self.catalog_rev > MAX_SAFE_INTEGER
            || !text(&row.id, 256, false)
            || !text(&row.kind, 128, false)
            || !text(&row.name, 4096, true)
            || !text(&row.operation, 128, false)
        {
            return Err(invalid());
        }
        for (value, limit) in [
            (&row.confirm, 4096),
            (&row.label, 4096),
            (&row.description, 16384),
            (&row.icon, 4096),
            (&row.reason, 4096),
            (&row.action, 4096),
            (&row.directory, 16384),
            (&row.environment_operation, 128),
            (&row.web_url, 65536),
            (&row.search_query, 65536),
            (&row.bundle_scope, 128),
            (&row.theme_mode, 128),
            (&row.extension_action, 128),
            (&row.source_extension, 128),
            (&row.theme_preset, 128),
        ] {
            if value.as_ref().is_some_and(|s| !text(s, limit, true)) {
                return Err(invalid());
            }
        }
        for (values, limit) in [(&row.arguments, 32), (&row.terminal_command, 64)] {
            if values.as_ref().is_some_and(|args| {
                args.len() > limit || args.iter().any(|s| !text(s, 16384, false))
            }) {
                return Err(invalid());
            }
        }
        match &row.value {
            Some(MenuActionValue::Text(value)) if !text(value, 65536, true) => {
                return Err(invalid())
            }
            Some(MenuActionValue::Integer(value)) if value.unsigned_abs() > MAX_SAFE_INTEGER => {
                return Err(invalid());
            }
            _ => {}
        }
        if row.operation == "copy" && !matches!(row.value.as_ref(), Some(MenuActionValue::Text(_)))
        {
            return Err(invalid());
        }
        if row.operation == "command" && row.arguments.as_ref().is_none_or(Vec::is_empty) {
            return Err(invalid());
        }
        Ok(())
    }
}
