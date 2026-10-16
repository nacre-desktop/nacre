use nacre_menu::{browser, model, search, sources, text::OrderingRules, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::Read;

fn reference() -> Result<Value> {
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(include_bytes!("fixtures/javascript.json.gz").as_slice())
        .read_to_end(&mut bytes)?;
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        "cee58914e0d7632e0272346ae86539d6aa9e890ff7dfb84afb6aa3d4d18a523c"
    );
    Ok(serde_json::from_slice(&bytes)?)
}
#[test]
fn frozen_current_javascript_outputs_are_exact() -> Result<()> {
    let reference = reference()?;
    let ordering = OrderingRules::new("en-US")?;
    let base: Value = serde_json::from_str(include_str!("fixtures/base-menu.json"))?;
    let rows = base["entries"].as_array().ok_or("Missing menu rows")?;
    let mut failures = Vec::new();
    for case in reference["cases"]
        .as_array()
        .ok_or("Missing reference cases")?
    {
        let catalog = reference["catalogs"][case["catalog"].as_str().unwrap_or("")]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let query = case["query"].as_str().unwrap_or("");
        let ids = |values: Vec<&Value>| {
            Value::Array(values.into_iter().map(|v| v["id"].clone()).collect())
        };
        let result = match case["op"].as_str().ok_or("Missing operation")? {
            "app.normalize" => json!(search::normalize(case["value"].as_str().unwrap_or(""))),
            "app.score" => json!(search::score(case["value"].as_str().unwrap_or(""), query)),
            "app.query" => {
                let prepared: Vec<_> = catalog.into_iter().map(search::Prepared::new).collect();
                ids(search::query(&prepared, query, &ordering))
            }
            "model.normalized" => json!(model::normalized(case["value"].as_str().unwrap_or(""))),
            "model.scope" => model::scope(query),
            "model.ranked" => ids(model::ranked(
                &catalog,
                query,
                &case["usage"],
                case["recent"].as_array().unwrap(),
                case["now"].as_f64().unwrap(),
            )),
            "model.distance" => json!(model::distance(
                case["a"].as_str().unwrap(),
                case["b"].as_str().unwrap(),
                case["limit"].as_u64().unwrap() as usize
            )),
            "model.stable" => json!(model::stable(&case["row"])),
            "model.alphabetical" => ids(model::alphabetical(&catalog, &ordering)),
            "model.application" => model::application(&case["entry"]),
            "model.resolve" => json!(model::resolve(case["route"].as_str().unwrap(), rows)),
            "model.trail" => json!(model::trail(case["route"].as_str().unwrap(), rows)),
            "model.children" => json!(model::children(case["route"].as_str().unwrap(), rows)),
            "model.rail" => json!(model::rail(rows, &case["preferences"])),
            "browser.available" => json!(browser::available(&catalog)),
            "browser.actions" => json!(browser::actions(&catalog)),
            "browser.rankedActions" => json!(browser::ranked_actions(&catalog, query)),
            "sources.decode" => sources::decode(case["text"].as_str().unwrap()),
            "sources.webUrl" => json!(sources::web_url(case["engine"].as_str().unwrap(), query)),
            op => return Err(format!("Unhandled frozen operation {op}").into()),
        };
        if result != case["expected"] {
            failures.push(format!(
                "case {} {} query{:?}",
                case["id"], case["op"], query
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} differing reference cases: {:?}",
        failures.len(),
        &failures[..failures.len().min(15)]
    );
    Ok(())
}
