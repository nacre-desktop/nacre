use nacre_protocol::{artifacts, colors, framing, Message};

#[test]
fn transcript_decodes_and_every_call_is_a_method_of_this_unit() {
    let fixture = artifacts::generate()
        .remove("protocol/fixtures/colors.jsonl")
        .unwrap();
    let (mut calls, mut replies) = (0, 0);
    for line in fixture.lines() {
        match framing::decode(line.as_bytes()).unwrap() {
            Message::Call { m, p, .. } => {
                assert!(colors::methods().contains_key(&m), "{m}");
                assert!(p.is_object());
                calls += 1;
            }
            Message::Ret { .. } => replies += 1,
            other => panic!("unexpected frame {other:?}"),
        }
    }
    assert_eq!((calls, replies), (6, 6));
}

#[test]
fn schemas_and_types_are_published() {
    let generated = artifacts::generate();
    for path in [
        "protocol/schema/colors-preview.json",
        "protocol/schema/colors-preview-result.json",
        "protocol/luau/colors.d.luau",
        "protocol/fixtures/colors.jsonl",
    ] {
        assert!(generated.contains_key(path), "missing {path}");
    }
    assert!(generated["protocol/luau/colors.d.luau"].contains("export type ColorsPreviewResult ="));
}
