//! Additive launcher method-v1 contract. Interpretation/ranking belongs to Nacre.
//! Engine implementations register methods separately from this payload module.
use crate::{ErrorCode, Failure, MAX_SAFE_INTEGER};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const VERSION: u16 = 1;
pub const MAX_ITEMS: usize = 2048;
fn apps() -> String {
    "apps".into()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MenuQuery {
    /// Passed unchanged to the shared engine, including all provider prefixes.
    #[serde(default)]
    #[schemars(length(max = 65536))]
    pub query: String,
    /// Open string vocabulary: apps/search/actions initially; no client ranking.
    #[serde(default = "apps")]
    #[schemars(length(min = 1, max = 128))]
    pub mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 256))]
    pub route: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 2048))]
    pub limit: Option<u32>,
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
}

/// Unit-conversion payload from the existing calculator provider.
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
