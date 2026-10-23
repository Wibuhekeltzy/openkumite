//! The embu on the converted assets (skipped without them).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // tests and tools: a failure stops them

use std::sync::Arc;

/// Every actor of a cast variant stands on the floor: its feet's mean lowest height over the show stays near the
/// ground (the root motion is scaled by each model's size; the embu's own character 11 and its alternate are 70 %).
#[test]
fn embu_actors_stand_on_the_floor() {
    let root = std::env::var_os("OPENKUMITE_ASSETS").map_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"), std::path::PathBuf::from);
    let Ok(assets) = kumite_assets::Assets::new(&root) else { return };
    let Ok(data) = kumite_game::embu::EmbuData::load(&assets) else { return };
    let data = Arc::new(data);
    let mut s = kumite_game::embu::EmbuScene::new(&assets, data, 2).unwrap();
    let mut feet = std::collections::BTreeMap::<usize, (f32, u32)>::new();
    for _ in 0..3600 {
        s.step();
        if s.done {
            break;
        }
        for k in 0..2 {
            if let Some((pose, place)) = s.pose(k) {
                let m = s.models[k];
                let ch = &s.characters[&m];
                let low = pose.iter().zip(&ch.nodes).filter(|(_, n)| n.name.starts_with("foot")).map(|(p, _)| place.transform_point3(p.w_axis.truncate()).y).fold(f32::MAX, f32::min);
                let e = feet.entry(m).or_insert((0.0, 0));
                (e.0, e.1) = (e.0 + low, e.1 + 1);
            }
        }
    }
    for m in [22, 23] {
        let (sum, n) = feet[&m];
        let mean = sum / n as f32;
        assert!(mean < 0.15, "model {m}'s feet are {mean:.2} m above the floor on average");
    }
}
