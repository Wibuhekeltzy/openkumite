//! Print both fighters' state every N frames for an input script: `cargo run --example trace -- "f*120,1" [N] [dummy]`
//! (`dummy`: P2 is a CPU-style dummy without neutral guard).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // tests and tools: a failure stops them
use kumite_game::{parse_script, Game};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let assets = kumite_assets::Assets::new(std::env::var_os("OPENKUMITE_ASSETS").map_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"), std::path::PathBuf::from))?;
    let models: Vec<usize> = std::env::var("MODELS").unwrap_or("71,71".into()).split(',').map(|s| s.parse().unwrap()).collect();
    let mut g = Game::load(&assets, [models[0], models[1]])?;
    g.fight.fighters[1].guard_input = args.get(3).map(String::as_str) != Some("dummy");
    let every: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(10);
    let p2 = parse_script(&std::env::var("P2").unwrap_or_default())?;
    for (i, pad) in parse_script(args.get(1).map(String::as_str).unwrap_or(""))?.into_iter().enumerate() {
        g.step([pad, p2.get(i).copied().unwrap_or_default()]);
        for h in &g.fight.hits {
            println!("{i:4}   hit by P{}: {} dmg {} blocked {} counter {}", h.attacker + 1, h.anim, h.damage, h.blocked, h.counter);
        }
        if i % every == 0 {
            let f = &g.fight.fighters;
            let (a0, f0) = f[0].animation(&g.fight.data);
            let (a1, f1) = f[1].animation(&g.fight.data);
            let v = g.camera.view;
            println!(
                "{i:4} P1 {a0}@{f0} root ({:.0}, {:.0}) yaw {:.0}° | P2 {a1}@{f1} root ({:.0}, {:.0}) | hp {} {} | cam yaw {}",
                f[0].root().x,
                f[0].root().z,
                f[0].yaw.to_degrees(),
                f[1].root().x,
                f[1].root().z,
                f[0].health >> 16,
                f[1].health >> 16,
                v.yaw
            );
        }
    }
    Ok(())
}
