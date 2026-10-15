//! A separate dependency graph catches feature leakage hidden by Nacre's app.
use nacre_protocol::{framing, Id, Message};
use serde_json::{Map, Value};

fn main() {
    let mut map = Map::new();
    map.insert("z".into(), Value::Null);
    map.insert("a".into(), Value::Null);
    assert_eq!(
        map.keys().map(String::as_str).collect::<Vec<_>>(),
        ["a", "z"]
    );
    let message = Message::Ping { id: Id::Integer(7) };
    let wire = framing::encode(&message).expect("Valid frame");
    assert_eq!(framing::decode(&wire).expect("Round trip"), message);
    println!(
        "Minimal protocol consumer passed; JSON Value occupies {} bytes",
        std::mem::size_of::<Value>()
    );
}
