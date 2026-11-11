// SPDX-License-Identifier: GPL-3.0-only
//! Native route behavior and shipped keybind/wrapper dispatch coverage.
use crate::commands::{from_strings, map_views};
use serde_json::Value;

fn call(target: &str, method: &str, args: &[&str]) -> bead::services::ipc::Call {
    from_strings(
        target.into(),
        method.into(),
        &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
    )
    .unwrap()
}
fn code(call: bead::services::ipc::Call) -> String {
    map_views(call).unwrap_err().error.unwrap().code
}

#[test]
fn unsupported_arguments_are_refused_explicitly_not_dropped() {
    for (target, method, args) in [
        ("drawers", "toggle", &["osd"][..]),
        ("drawers", "toggle", &["leftPinned"][..]),
        ("drawers", "toggle", &["dashboardPinned"][..]),
        ("drawers", "toggle", &["previewOnly"][..]),
    ] {
        assert_eq!(
            code(call(target, method, args)),
            "unavailable",
            "{target} {method} {args:?}"
        );
    }
}

#[test]
fn extra_sections_and_drawers_map_to_bead_views() {
    for (target, method, args, view, expected) in [
        ("nacre", "controls", &["session"][..], "session", "open"),
        (
            "nacre",
            "controls",
            &["notifications"][..],
            "dashboard-notifications",
            "open",
        ),
        ("drawers", "toggle", &["session"][..], "session", "toggle"),
    ] {
        let mapped = map_views(call(target, method, args)).unwrap();
        assert_eq!(mapped.target, "bead.panel");
        assert_eq!(mapped.method, expected);
        assert_eq!(mapped.args, vec![Value::String(view.into())]);
    }
    let launcher = map_views(call("drawers", "toggle", &["launcher"])).unwrap();
    assert!(matches!(crate::key_routes::launcher(&launcher), Some(Ok(l)) if l.toggle));
    // The control centre is a pearl kind now.
    let pearl = map_views(call("pearls", "open", &["controls"])).unwrap();
    assert_eq!(pearl.args, vec![Value::String("control".into())]);
}

/// B0093: `menuPrompt open|cancel ID` go to the on-demand prompt app (unconfigured in unit tests); an id
/// that is not 32 hex digits is refused before anything starts.
#[test]
fn a_selector_prompt_routes_to_its_app_and_bad_ids_are_refused() {
    let ident = "0123456789abcdef0123456789abcdef";
    for method in ["open", "cancel"] {
        let reply = map_views(call("menuPrompt", method, &[ident])).unwrap_err();
        assert_eq!(
            reply.error.as_ref().map(|e| e.message.as_str()),
            Some("apps are not configured in this shell"),
            "{method}"
        );
        assert_eq!(
            code(call("menuPrompt", method, &["../../etc"])),
            "invalid-route"
        );
    }
}

/// B0084: every `nacrectl ui` call Nacre's niri/Hyprland binds and app wrappers issue
/// (`shell/keybind-calls.json`) is accepted by the provider's catalog check and then mapped, forwarded
/// or acknowledged. Only a stated refusal may differ; a call Bead rejects as unknown fails here.
#[test]
fn every_shipped_keybind_and_wrapper_call_is_accepted_by_the_provider() {
    let json: Value =
        serde_json::from_str(include_str!("../../../shell/keybind-calls.json")).unwrap();
    let calls = json["calls"].as_array().unwrap();
    assert!(calls.len() > 15, "the snapshot lost its calls");
    for words in calls {
        let words: Vec<String> = words
            .as_array()
            .unwrap()
            .iter()
            .map(|w| w.as_str().unwrap().to_owned())
            .collect();
        let label = format!("{words:?}");
        let c = from_strings(words[0].clone(), words[1].clone(), &words[2..])
            .unwrap_or_else(|e| panic!("{label}: {e}"));
        crate::commands::validate(&c)
            .unwrap_or_else(|e| panic!("{label}: not in the catalog: {e:?}"));
        match map_views(c) {
            Ok(_) => {}
            // A documented acknowledgement, or an on-demand app (unconfigured in unit tests).
            Err(reply) => assert!(
                reply.error.is_none()
                    || reply.error.as_ref().map(|e| e.message.as_str())
                        == Some("apps are not configured in this shell"),
                "{label}: refused: {reply:?}"
            ),
        }
    }
}
