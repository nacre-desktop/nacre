//! Developer-only phase timings, with synthetic apps and disposable HOME/XDG.
use nacre_helper::{launcher_workflows, menu, software_library, Result};
use nacre_menu::{config, context::Context};
use serde_json::{json, Value};
use std::{hint::black_box, time::Instant};
fn main() -> Result<()> {
    let mut c = Context::new(
        config::merge(
            include_str!("../../../nacre/shell-tools/launcher-menu.json"),
            "",
        )
        .map_err(|s| -> nacre_helper::Error { s.into() })?,
        serde_json::from_str(include_str!("../../nacre-menu/data/defaults.json"))?,
        "en-US",
        1720000000.0,
    )?;
    c.snapshot = menu::snapshot(true)?;
    c.library = software_library::execute(&["load".into()], None)?;
    c.workflows = launcher_workflows::execute(&["load".into()], None)?;
    c.all_apps=(0..320).map(|i|json!({"id":format!("fixture-{i:04}"),"name":format!("Application {i:04}"),"genericName":"Fixture tool","comment":"Synthetic application catalog","command":["fixture-command",i.to_string()],"icon":"fixture","categories":["Utility"],"keywords":["fixture","app"],"noDisplay":false,"runInTerminal":false,"workingDirectory":""})).collect();
    c.visible_apps = c.all_apps.clone();
    let global = c.global_rows("app")?;
    let mut output = serde_json::Map::new();
    output.insert("sizes".into(),json!({"apps":c.all_apps.len(),"fonts":c.snapshot["fonts"].as_array().map(Vec::len),"packages":c.snapshot["packages"].as_array().map(Vec::len),"global":global.len()}));
    macro_rules! phase {($name:expr,$call:expr)=>{{let started=Instant::now();let mut count=0;for _ in 0..100 {let rows=$call;count=rows.len();black_box(rows);}output.insert($name.into(),json!({"ms":started.elapsed().as_secs_f64()*10.0,"rows":count}));}}}
    phase!("apps", c.apps());
    phase!("static", c.all_static());
    phase!("interface-fonts", c.font_rows(false));
    phase!("mono-fonts", c.font_rows(true));
    phase!("software", c.software_rows("software.discover"));
    phase!("global", c.global_rows("app")?);
    phase!("rank", c.rank(&global, "app"));
    phase!("content", c.content("apps", "app", "all", "all")?);
    c.prepare_queries()?;
    phase!(
        "indexed-content",
        c.indexed_content("apps", "app", "all", "all")?
    );
    println!("{}", Value::Object(output));
    Ok(())
}
