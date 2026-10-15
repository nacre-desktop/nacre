use nacre_protocol::{artifacts, framing, patch, Id, Message, Mirror, Update, MAX_FRAME};
use proptest::prelude::*;
use serde_json::{json, Value};

#[test]
fn generated_contract_is_fresh() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for (path, expected) in artifacts::generate() {
        assert_eq!(
            std::fs::read_to_string(root.join(&path)).unwrap_or_else(|_| panic!(
                "Missing {path}; run cargo run -p nacre-protocol --example export"
            )),
            expected,
            "Stale {path}"
        );
    }
}
#[test]
fn framing_bounds_types_duplicates_and_unknown_optional_fields() {
    for bad in ["[]","null","{}","{\"t\":\"hello\",\"t\":\"hello\",\"proto\":1,\"client\":\"x\"}","{\"t\":\"call\",\"id\":1,\"m\":\"x\",\"p\":{\"a\":1,\"a\":2}}","{\"t\":\"ret\",\"id\":1,\"ok\":null,\"err\":{\"code\":\"io\",\"message\":\"x\",\"retry\":true}}","{\"t\":\"ping\",\"id\":9007199254740992}","{\"t\":\"call\",\"id\":1,\"m\":\"x\",\"timeout\":0}"] { assert!(framing::decode(bad.as_bytes()).is_err(),"{bad}"); }
    assert!(framing::decode(&[0xff]).is_err());
    assert!(framing::decode(&vec![b' '; MAX_FRAME + 1]).is_err());
    let wire = b"{\"t\":\"hello\",\"proto\":1,\"client\":\"fixture\",\"newOptional\":true}\r\n";
    let mut decoder = framing::Decoder::default();
    for byte in &wire[..wire.len() - 1] {
        assert!(decoder.push(&[*byte]).unwrap().is_empty());
    }
    assert_eq!(decoder.push(b"\n").unwrap().len(), 1);
    assert_eq!(decoder.pending_bytes(), 0);
    let mut decoder = framing::Decoder::default();
    assert!(decoder.push(&vec![b' '; MAX_FRAME]).unwrap().is_empty());
    assert!(decoder.push(b" ").is_err());
    assert_eq!(decoder.pending_bytes(), 0);
    assert!(Id::Integer(-9_007_199_254_740_991).valid());
    let mut maximum = b"{\"t\":\"ping\",\"id\":0}".to_vec();
    maximum.resize(MAX_FRAME, b' ');
    assert!(framing::decode(&maximum).is_ok());
    maximum.push(b' ');
    assert!(framing::decode(&maximum).is_err());
}
#[test]
fn coalesced_sequences_and_gaps_replay_without_mutating_last_good_value() {
    let trace = artifacts::transcripts()
        .remove("protocol/fixtures/state.jsonl")
        .unwrap();
    let mut mirror = Mirror::default();
    let mut updates = Vec::new();
    for line in trace.lines() {
        updates.push(mirror.receive(&framing::decode(line.as_bytes()).unwrap()));
    }
    assert_eq!(updates[2], Update::Changed("compositor".into()));
    assert_eq!(updates[3], Update::Resync("compositor".into()));
    assert_eq!(mirror.topics["compositor"].0, 4);
    assert_eq!(
        mirror.topics["compositor"].1["windows"]["w1"]["focused"],
        false
    );
    assert_eq!(
        mirror.receive(&Message::Patch {
            topic: "compositor".into(),
            base: 0,
            seq: 3,
            data: json!({"obsolete":true})
        }),
        Update::Unchanged
    );
}
#[test]
fn null_members_need_snapshots_and_restarts_retain_display_but_refuse_old_patches() {
    assert!(matches!(
        patch::diff(&json!({}), &json!({"v":null})),
        patch::Diff::Snapshot(_)
    ));
    let mut mirror = Mirror::default();
    mirror.receive(&Message::Snap {
        topic: "x".into(),
        seq: 3,
        data: json!({"old":true}),
    });
    let trace = artifacts::transcripts()
        .remove("protocol/fixtures/restart.jsonl")
        .unwrap();
    for line in trace.lines().take(1) {
        assert_eq!(
            mirror.receive(&framing::decode(line.as_bytes()).unwrap()),
            Update::Boot
        );
    }
    assert_eq!(mirror.topics["x"].1, json!({"old":true}));
    assert_eq!(
        mirror.receive(&Message::Patch {
            topic: "x".into(),
            base: 3,
            seq: 4,
            data: json!({"old":false})
        }),
        Update::Resync("x".into())
    );
}
fn values() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i32>().prop_map(|n| json!(n)),
        "[a-z]{0,16}".prop_map(Value::String)
    ];
    leaf.prop_recursive(4, 64, 8, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..6).prop_map(Value::Array),
            prop::collection::btree_map("[a-z]{1,8}", inner, 0..6)
                .prop_map(|v| Value::Object(v.into_iter().collect()))
        ]
    })
}
#[tokio::test(flavor = "current_thread")]
async fn cancelled_read_keeps_partial_bytes_and_pipelined_frames() {
    use tokio::io::{AsyncWriteExt, BufReader};
    let (mut writer, stream) = tokio::io::duplex(256);
    let first = Message::Ping { id: Id::Integer(1) };
    let second = Message::Ping { id: Id::Integer(2) };
    let wire = framing::encode(&first).unwrap();
    let split = wire.len() / 2;
    writer.write_all(&wire[..split]).await.unwrap();
    let mut reader = framing::Reader::new(BufReader::new(stream));
    tokio::select! {result=reader.next()=>panic!("Incomplete read returned {result:?}"),_=tokio::task::yield_now()=>{}}
    writer.write_all(&wire[split..]).await.unwrap();
    writer
        .write_all(&framing::encode(&second).unwrap())
        .await
        .unwrap();
    assert_eq!(reader.next().await.unwrap(), Some(first));
    assert_eq!(reader.next().await.unwrap(), Some(second));
}
proptest! {
    #[test]
    fn random_gaps_resync_to_latest_without_changing_the_last_good_snapshot(
        updates in prop::collection::vec((any::<i32>(),any::<bool>()),1..32)
    ) {
        let mut mirror=Mirror::default();
        mirror.receive(&Message::Snap {topic:"x".into(),seq:0,data:json!({"v":0})});
        let mut source=json!({"v":0});
        for (i,(value,drop_update)) in updates.into_iter().enumerate() {
            let seq=i as u64+1; let next=json!({"v":value});
            if !drop_update {
                let before=mirror.topics["x"].clone();
                let change=match patch::diff(&source,&next) {patch::Diff::Patch(p)=>p,_=>json!({})};
                let result=mirror.receive(&Message::Patch {topic:"x".into(),base:seq-1,seq,data:change});
                if result==Update::Resync("x".into()) {
                    prop_assert_eq!(&mirror.topics["x"],&before);
                    mirror.receive(&Message::Snap {topic:"x".into(),seq,data:next.clone()});
                }
                prop_assert_eq!(&mirror.topics["x"].1,&next);
            }
            source=next;
        }
        let seq=100;
        mirror.receive(&Message::Snap {topic:"x".into(),seq,data:source.clone()});
        prop_assert_eq!(&mirror.topics["x"].1,&source);
    }
    #[test]
    fn diff_apply_round_trip(before in values(),after in values()) {
        let change=patch::diff(&before,&after); let mut observed=before; patch::apply_diff(&mut observed,&change); prop_assert_eq!(observed,after);
    }
    #[test]
    fn arbitrary_chunking_round_trips(cuts in prop::collection::vec(1usize..40,1..30)) {
        let message=Message::Ping {id:Id::Text("fixture".into())}; let wire=framing::encode(&message).unwrap(); let mut decoder=framing::Decoder::default(); let mut offset=0; let mut observed=Vec::new();
        for size in cuts { let end=(offset+size).min(wire.len()); observed.extend(decoder.push(&wire[offset..end]).unwrap()); offset=end; }
        observed.extend(decoder.push(&wire[offset..]).unwrap()); prop_assert_eq!(observed,vec![message]);
    }
}
