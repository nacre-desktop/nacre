use nacre_protocol::{artifacts, framing, i18n::*, Message as Frame, Reply};

#[test]
fn goldens_decode_into_the_published_types_and_registry_matches() {
    let files = artifacts::generate();
    let (mut calls, mut pages) = (0, 0);
    for line in files["protocol/fixtures/i18n.jsonl"].lines() {
        match framing::decode(line.as_bytes()).unwrap() {
            Frame::Snap { topic, data, .. } => {
                assert_eq!(topic, TOPIC);
                serde_json::from_value::<I18nState>(data)
                    .unwrap()
                    .validate()
                    .unwrap();
            }
            Frame::Call { m, p, .. } => {
                assert_eq!(m, "i18n.catalog");
                serde_json::from_value::<I18nCatalogParams>(p)
                    .unwrap()
                    .validate()
                    .unwrap();
                calls += 1;
            }
            Frame::Ret {
                reply: Reply::Ok { ok },
                ..
            } => {
                let page: I18nCatalog = serde_json::from_value(ok).unwrap();
                assert!(page.entries.len() <= MAX_PAGE_ENTRIES);
                pages += 1;
            }
            _ => {}
        }
    }
    assert_eq!((calls, pages), (3, 2));
    let methods = methods();
    assert_eq!(methods["i18n.catalog"].v, 1);
}
