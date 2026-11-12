use nacre_menu::{config, context::Context, Result};
use serde_json::{json, Value};

fn context() -> Result<Context> {
    Context::new(
        config::merge(
            include_str!("../../../nacre/shell-tools/launcher-menu.json"),
            "",
        )?,
        serde_json::from_str(include_str!("../data/defaults.json"))?,
        "en-US",
        0.,
    )
}
fn ids(rows: &[Value]) -> Vec<&str> {
    rows.iter().filter_map(|row| row["id"].as_str()).collect()
}

#[test]
fn optional_rows_and_empty_static_groups_follow_capabilities() -> Result<()> {
    let mut c = context()?;
    for (ai, brain) in [(false, false), (true, false), (false, true), (true, true)] {
        c.snapshot = json!({"capabilities":{"ai":ai,"brain":brain}});
        let rows = c.all_static();
        assert_eq!(ids(&rows).contains(&"actions.assistant"), ai || brain);
        for row in c.rows() {
            if matches!(row["brand"].as_str(), Some("ai" | "brain")) {
                let enabled = if row["brand"] == "ai" { ai } else { brain };
                assert_eq!(ids(&rows).contains(&row["id"].as_str().unwrap()), enabled);
            }
        }
        assert_eq!(
            !c.content("home", "? question", "all", "all")?.is_empty(),
            ai
        );
    }
    // The rule works for custom groups too; it does not depend on a Nacre group ID.
    c.catalog = json!({"rows":[{"id":"custom","kind":"group"},
        {"id":"custom.ai","kind":"action","brand":"ai"}]});
    c.snapshot = json!({"capabilities":{"ai":false}});
    assert!(c.all_static().is_empty());
    Ok(())
}

#[test]
fn empty_prefix_scopes_show_useful_defaults_and_keep_optional_gates() -> Result<()> {
    let mut c = context()?;
    c.snapshot = json!({"capabilities":{"ai":true,"brain":true}});
    c.can_ask = true;
    let ai = c.content("home", "?", "all", "all")?;
    assert!(ai.iter().any(|row| row["value"] == "new"));
    assert!(ai.iter().any(|row| row["value"] == "resume"));
    assert!(ai.iter().any(|row| row["value"] == "load"));
    assert!(ai
        .iter()
        .any(|row| row["operation"] == "scope" && row["value"] == "? "));
    c.snapshot["capabilities"]["ai"] = json!(false);
    assert!(c.content("home", "?", "all", "all")?.is_empty());

    c.snapshot["capabilities"]["brain"] = json!(true);
    assert!(!c.content("home", "#", "all", "all")?.is_empty());
    c.snapshot["capabilities"]["brain"] = json!(false);
    assert!(c.content("home", "#", "all", "all")?.is_empty());
    Ok(())
}

#[test]
fn backend_rows_are_hidden_instead_of_disabled() -> Result<()> {
    let mut c = context()?;
    for backend in ["niri", "hyprland", ""] {
        c.compositor = json!({"backendName":backend});
        let rows = c.all_static();
        for (id, expected) in [
            ("learn.niri", backend == "niri"),
            ("learn.hyprland", backend == "hyprland"),
            ("actions.bring-visitors", backend == "niri"),
        ] {
            assert_eq!(ids(&rows).contains(&id), expected, "{backend}: {id}");
        }
    }
    Ok(())
}

#[test]
fn brain_results_need_both_source_and_capability_and_open_the_absolute_file() -> Result<()> {
    let mut c = context()?;
    c.brain_query = "note".into();
    c.notes = vec![json!({"path":"folder/note.md","file":"/brain/folder/note.md","title":"Note"})];
    for enabled in [false, true] {
        for source in [false, true] {
            c.snapshot = json!({"capabilities":{"brain":enabled}});
            c.source_options["brain"] = json!(source);
            let rows = c.brain_rows("note");
            assert_eq!(rows.len(), usize::from(enabled && source));
            if enabled && source {
                assert_eq!(rows[0]["operation"], "note");
                assert_eq!(rows[0]["value"], "/brain/folder/note.md");
            }
        }
    }
    Ok(())
}
