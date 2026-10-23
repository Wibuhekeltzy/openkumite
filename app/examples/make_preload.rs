//! web/preload.json from the reads the native build recorded (`--record-reads`, one JSON per screen in DIR):
//! `cargo run --release --example make_preload -- DIR web/preload.json` (web/make_preload.sh runs it). A fight's
//! characters (models, movesets), stages, movies and music are left out: fetched when needed.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // a tool: a failure stops it

use serde_json::{json, Value};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (dir, out) = (&args[0], &args[1]);
    let mut files: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.retain(|p| p.extension().is_some_and(|e| e == "json"));
    files.sort();
    let (mut seen, mut list) = (std::collections::HashSet::new(), Vec::new());
    let skip = ["characters/", "stages/", "video/", "audio/music/"].map(|d| format!("assets/{d}"));
    for f in files {
        let reads: Vec<(String, u64)> = serde_json::from_slice(&std::fs::read(&f).unwrap()).unwrap();
        for (path, size) in reads {
            let fighter = path.starts_with("assets/gamedata/movesets/moveset_") && !path.ends_with("_embu.json");
            if fighter || skip.iter().any(|d| path.starts_with(d.as_str())) || !seen.insert(path.clone()) {
                continue;
            }
            list.push(json!([path, size]));
        }
    }
    // read in a browser only (the native build drives its pads otherwise)
    let assets = std::env::var("OPENKUMITE_ASSETS").unwrap_or_else(|_| "assets".into());
    let path = "assets/gamedata/rumble.json";
    let local = std::path::Path::new(&assets).join(&path["assets/".len()..]);
    if let (false, Ok(m)) = (seen.contains(path), local.metadata()) {
        list.push(json!([path, m.len()]));
    }
    let total: u64 = list.iter().filter_map(|e| e[1].as_u64()).sum();
    std::fs::write(out, serde_json::to_string(&Value::Array(list.clone())).unwrap()).unwrap();
    println!("{out}: {} files, {:.1} MB", list.len(), total as f64 / 1e6);
}
