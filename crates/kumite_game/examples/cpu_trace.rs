#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // tests and tools: a failure stops them
use kumite_game::{bout::Bout, Game};
use kumite_sim::input::Pad;
fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let m: Vec<usize> = args[1].split(',').map(|s| s.parse().unwrap()).collect();
    let every: u32 = args.get(2).map_or(60, |s| s.parse().unwrap());
    let assets = kumite_assets::Assets::new(std::env::var_os("OPENKUMITE_ASSETS").map_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"), std::path::PathBuf::from))?;
    let mut b = Bout::new(Game::load(&assets, [m[0], m[1]])?, kumite_game::ai_data(&assets)?, 2, false);
    b.set_cpu(0, Some((2, 9)));
    b.set_cpu(1, Some((2, 9)));
    for i in 0..7200u32 {
        b.step([Pad::default(); 2]);
        let g = &b.game;
        for h in &g.fight.hits {
            println!("{i:5} HIT P{} {} dmg {} blocked {}", h.attacker + 1, h.anim, h.damage, h.blocked);
        }
        if i % every == 0 {
            let f = &g.fight.fighters;
            let mut line = format!("{i:5} {:?}", b.phase);
            for (k, fk) in f.iter().enumerate() {
                let (a, fr) = fk.animation(&g.fight.data);
                line += &format!(" | P{} ({:.0},{:.0}) {a}@{fr} hp {} stun {} {}", k + 1, fk.root().x, fk.root().z, fk.health >> 16, fk.stun, fk.stun_timer);
            }
            println!("{line}");
        }
        if b.finished() {
            println!("finished {i}");
            break;
        }
    }
    Ok(())
}
