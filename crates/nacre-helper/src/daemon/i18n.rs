//! Publishes the UI language and serves catalog pages. The desktop settings store
//! is the only language source; the catalog files are read per request, so nothing
//! is cached or resident and a bounded page never carries the whole catalog.
use super::hub::{failure, Shared};
use super::menu::{message_language, translations};
use nacre_protocol::{
    i18n::{
        I18nCatalog, I18nCatalogParams, I18nState, Message, Plural, MAX_PAGE_BYTES,
        MAX_PAGE_ENTRIES, TOPIC,
    },
    ErrorCode, Failure,
};
use serde_json::Value;
use std::collections::BTreeMap;

pub use nacre_protocol::i18n::methods;

/// Called whenever the desktop settings store is published: the revision moves
/// only when the effective language does.
pub fn update(hub: &Shared, desktop: &Value) {
    let language = message_language(desktop);
    let previous = hub
        .borrow()
        .snapshot(TOPIC)
        .and_then(|value| serde_json::from_value::<I18nState>(value).ok());
    if previous
        .as_ref()
        .is_some_and(|state| state.language == language)
    {
        return;
    }
    let state = I18nState {
        language,
        fallback: "en".into(),
        revision: previous.map_or(1, |state| state.revision + 1),
    };
    if state.validate().is_err()
        || hub
            .borrow_mut()
            .publish(TOPIC, serde_json::to_value(&state).unwrap_or(Value::Null))
            .is_err()
    {
        tracing::warn!("i18n state could not be published");
    }
}

fn message(value: &Value, pseudo: bool) -> Option<Message> {
    let text = |value: &Value| {
        let text = value.as_str().filter(|text| text.len() <= 1024)?;
        Some(if pseudo {
            nacre_i18n::pseudo(text)
        } else {
            text.into()
        })
    };
    match value {
        Value::String(_) => text(value).map(Message::Text),
        Value::Object(forms) => Some(Message::Plural(Plural {
            one: text(forms.get("one")?)?,
            other: text(forms.get("other")?)?,
        })),
        _ => None,
    }
}

pub fn catalog(hub: &Shared, params: Value) -> Result<Value, Failure> {
    let request: I18nCatalogParams = serde_json::from_value(params).map_err(|_| {
        failure(ErrorCode::Validation, "Invalid i18n request", false).dispatch_state(false)
    })?;
    request.validate()?;
    let (state, paths) = {
        let hub = hub.borrow();
        (
            hub.snapshot(TOPIC)
                .and_then(|value| serde_json::from_value::<I18nState>(value).ok()),
            hub.settings.as_ref().map(|settings| settings.paths.clone()),
        )
    };
    let (Some(state), Some(paths)) = (state, paths) else {
        return Err(failure(
            ErrorCode::Unavailable,
            "Catalog is unavailable",
            true,
        ));
    };
    if request
        .revision
        .is_some_and(|revision| revision != state.revision)
    {
        return Err(failure(
            ErrorCode::StaleRevision,
            "Language changed; refresh the catalog",
            true,
        ));
    }
    let messages = translations(&paths, &state.language)
        .map_err(|_| failure(ErrorCode::Io, "Catalog could not be read", true))?;
    serde_json::to_value(page(&messages, &state, &request))
        .map_err(|_| failure(ErrorCode::Internal, "Catalog encoding failed", false))
}

/// One page of `namespace` ids after the cursor, sorted, within the page bounds.
fn page(messages: &Value, state: &I18nState, request: &I18nCatalogParams) -> I18nCatalog {
    let prefix = format!("{}.", request.namespace);
    let pseudo = state.language == "pseudo";
    let mut ids: Vec<(&String, Message)> = messages
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(id, _)| {
            id.len() <= 128
                && id.starts_with(&prefix)
                && request.after.as_ref().is_none_or(|after| *id > after)
        })
        .filter_map(|(id, value)| Some((id, message(value, pseudo)?)))
        .collect();
    ids.sort_by(|a, b| a.0.cmp(b.0));
    let (mut entries, mut bytes, mut next) = (BTreeMap::new(), 0, None);
    for (id, value) in ids {
        bytes += id.len() + serde_json::to_string(&value).map_or(0, |text| text.len());
        if entries.len() == MAX_PAGE_ENTRIES || (bytes > MAX_PAGE_BYTES && !entries.is_empty()) {
            next = entries.keys().next_back().cloned();
            break;
        }
        entries.insert(id.clone(), value);
    }
    I18nCatalog {
        revision: state.revision,
        entries,
        next,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn state(language: &str) -> I18nState {
        I18nState {
            language: language.into(),
            fallback: "en".into(),
            revision: 3,
        }
    }
    fn request(namespace: &str, after: Option<&str>) -> I18nCatalogParams {
        I18nCatalogParams {
            namespace: namespace.into(),
            revision: None,
            after: after.map(str::to_owned),
        }
    }

    #[test]
    fn pages_are_namespaced_sorted_bounded_and_resumable() {
        let mut messages = serde_json::Map::new();
        for index in 0..450 {
            messages.insert(format!("many.id{index:03}"), json!("x"));
        }
        messages.insert("other.id".into(), json!("not in this namespace"));
        messages.insert("manyother.id".into(), json!("prefix is a whole segment"));
        messages.insert("many.bad".into(), json!(7));
        let messages = Value::Object(messages);
        let (mut seen, mut after, mut pages) = (0, None, 0);
        loop {
            let page = page(&messages, &state("en"), &request("many", after.as_deref()));
            assert!(page.entries.len() <= MAX_PAGE_ENTRIES);
            seen += page.entries.len();
            pages += 1;
            match page.next {
                Some(next) => after = Some(next),
                None => break,
            }
        }
        assert_eq!((seen, pages), (450, 3));
        assert!(page(&messages, &state("en"), &request("absent", None))
            .entries
            .is_empty());
    }

    #[test]
    fn pages_stop_at_the_byte_bound_and_pseudo_keeps_placeholders() {
        let long = "y".repeat(900);
        let messages: Value = (0..100)
            .map(|index| (format!("big.id{index:03}"), json!(long)))
            .collect::<serde_json::Map<_, _>>()
            .into();
        let first = page(&messages, &state("en"), &request("big", None));
        assert!(first.next.is_some() && first.entries.len() < 40);
        assert!(serde_json::to_vec(&first).unwrap().len() <= MAX_PAGE_BYTES + 4096);
        let messages = json!({"bead.n":"Hello {name}","bead.p":{"one":"{count} item","other":"{count} items"}});
        let pseudo = page(&messages, &state("pseudo"), &request("bead", None));
        assert!(
            matches!(&pseudo.entries["bead.n"], Message::Text(text) if text.contains("{name}") && text.starts_with('['))
        );
        assert!(
            matches!(&pseudo.entries["bead.p"], Message::Plural(p) if p.one.contains("{count}") && p.other.contains("{count}"))
        );
        assert_eq!(pseudo.revision, 3);
    }
}
